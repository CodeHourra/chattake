use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row, Transaction};
use uuid::Uuid;

use super::db::{Database, DbError, DbResult};
use super::models::*;

// ─────────────────────────── 共享列定义与行映射 ────────────────────────
//
// cards.rs 和 search.rs 共用这些函数，避免列顺序不一致导致的映射错误。

pub(super) const CARD_SUMMARY_COLUMNS: &str = r#"c.id, c.session_id, c.title, c."type", c.value, c.summary, c.publication_status, c.source_name, c.project_name, c.created_at, c.updated_at"#;

/// 从查询行映射 CardSummary（列顺序需与 CARD_SUMMARY_COLUMNS 一致）
pub(super) fn card_summary_from_row(row: &Row<'_>) -> rusqlite::Result<CardSummary> {
    Ok(CardSummary {
        id: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
        card_type: row.get(3)?,
        value: row.get(4)?,
        summary: row.get(5)?,
        publication_status: row.get(6)?,
        source_name: row.get(7)?,
        project_name: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        match_snippet: None,
        tags: Vec::new(),
        technologies: Vec::new(),
    })
}

fn card_from_row(row: &Row<'_>) -> rusqlite::Result<Card> {
    Ok(Card {
        id: row.get(0)?,
        session_id: row.get(1)?,
        analysis_run_id: row.get(2)?,
        title: row.get(3)?,
        card_type: row.get(4)?,
        value: row.get(5)?,
        summary: row.get(6)?,
        note: row.get(7)?,
        publication_status: row.get(8)?,
        is_user_edited: row.get::<_, i64>(9)? != 0,
        source_name: row.get(10)?,
        project_name: row.get(11)?,
        prompt_tokens: row.get(12)?,
        completion_tokens: row.get(13)?,
        cost_yuan: row.get(14)?,
        feedback: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        tags: Vec::new(),
        tech_stack: Vec::new(),
        source_session_external_id: row.get(18)?,
        source_session_path: row.get(19)?,
    })
}

fn rebuild_card_fts(tx: &Transaction<'_>, card_id: &str) -> rusqlite::Result<()> {
    let row = tx.query_row(
        "SELECT rowid,title,summary,note FROM cards WHERE id=?1",
        params![card_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let labels = |kind: &str| -> rusqlite::Result<String> {
        tx.query_row(
            "SELECT COALESCE(GROUP_CONCAT(t.name, ','),'') FROM tags t JOIN card_tags ct ON ct.tag_id=t.id WHERE ct.card_id=?1 AND t.kind=?2",
            params![card_id, kind],
            |row| row.get(0),
        )
    };
    tx.execute("DELETE FROM cards_fts WHERE rowid=?1", params![row.0])?;
    tx.execute(
        "INSERT INTO cards_fts(rowid,title,summary,note,tags,technologies) VALUES(?1,?2,?3,?4,?5,?6)",
        params![row.0, row.1, row.2, row.3, labels("topic")?, labels("technology")?],
    )?;
    Ok(())
}

fn attach_labels(
    tx: &Transaction<'_>,
    card_id: &str,
    kind: &str,
    names: &[String],
) -> rusqlite::Result<()> {
    for name in names {
        let display = name.trim();
        let normalized = display.to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO tags(id,name,normalized_name,kind) VALUES(?1,?2,?3,?4)",
            params![Uuid::new_v4().to_string(), display, normalized, kind],
        )?;
        let tag_id: String = tx.query_row(
            "SELECT id FROM tags WHERE kind=?1 AND normalized_name=?2",
            params![kind, normalized],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO card_tags(card_id,tag_id) VALUES(?1,?2)",
            params![card_id, tag_id],
        )?;
    }
    Ok(())
}

// ─────────────────────────── 动态筛选条件构建 ─────────────────────────
//
// 被 list_cards 和 search_cards 共用。
//
// 标签、技术栈筛选均为 AND 语义（须同时满足）：
// - 标签：card_tags 子查询 HAVING COUNT(DISTINCT name) = N
// - 技术栈：cards.tech_stack 为逗号串，对每项用 instr 边界匹配（大小写不敏感）

/// 根据 CardFilters 构建 WHERE 子句和参数列表
pub(super) fn build_card_where(filters: &CardFilters) -> (String, Vec<String>) {
    let mut conds: Vec<String> = vec!["c.publication_status = ?".to_string()];
    let mut params: Vec<String> = Vec::new();
    params.push(
        filters
            .publication_status
            .as_deref()
            .unwrap_or("published")
            .to_string(),
    );

    if let Some(ref t) = filters.card_type {
        conds.push(r#"c."type" = ?"#.to_string());
        params.push(t.clone());
    }
    if let Some(ref v) = filters.value {
        conds.push("c.value = ?".to_string());
        params.push(v.clone());
    }
    if let Some(ref tags) = filters.tags {
        if !tags.is_empty() {
            let unique: Vec<&String> = {
                let mut seen = std::collections::HashSet::new();
                tags.iter().filter(|t| seen.insert(*t)).collect()
            };
            let n = unique.len();
            let placeholders = vec!["?"; n].join(",");
            conds.push(format!(
                "c.id IN (\
                    SELECT ct.card_id FROM card_tags ct \
                    INNER JOIN tags t ON ct.tag_id = t.id \
                    WHERE t.kind = 'topic' AND t.name IN ({placeholders}) \
                    GROUP BY ct.card_id HAVING COUNT(DISTINCT t.name) = {n}\
                )"
            ));
            params.extend(unique.into_iter().cloned());
        }
    }

    if let Some(ref stacks) = filters.tech_stack {
        let unique: Vec<&String> = {
            let mut seen = std::collections::HashSet::new();
            stacks
                .iter()
                .filter(|s| !s.trim().is_empty())
                .filter(|s| seen.insert(*s))
                .collect()
        };
        if !unique.is_empty() {
            let n = unique.len();
            let placeholders = vec!["?"; n].join(",");
            conds.push(format!(
                "c.id IN (SELECT ct.card_id FROM card_tags ct INNER JOIN tags t ON ct.tag_id=t.id \
                 WHERE t.kind='technology' AND t.name IN ({placeholders}) \
                 GROUP BY ct.card_id HAVING COUNT(DISTINCT t.name)={n})"
            ));
            params.extend(unique.into_iter().cloned());
        }
    }

    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };
    (where_sql, params)
}

// ─────────────────────────── Database 方法 ────────────────────────────

impl Database {
    /// 创建知识卡片。在单个事务内完成：
    ///   1. 写入 cards 表
    ///   2. 创建/关联标签（tags + card_tags）
    ///   3. 同步 FTS5 全文索引
    pub fn insert_card(&self, card: &NewCard<'_>) -> DbResult<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_joined = card.tags.join(",");
        let technologies_joined = card.tech_stack.join(",");
        let mut conn = self.conn();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO cards (
                id, session_id, analysis_run_id, title, type, value, summary, note,
                publication_status, source_name, project_name,
                prompt_tokens, completion_tokens, cost_yuan, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &id,
                card.session_id,
                card.analysis_run_id,
                card.title,
                card.card_type,
                card.value,
                card.summary,
                card.note,
                card.publication_status,
                card.source_name,
                card.project_name,
                card.prompt_tokens,
                card.completion_tokens,
                card.cost_yuan,
                &now,
                &now,
            ],
        )?;

        attach_labels(&tx, &id, "topic", card.tags)?;
        attach_labels(&tx, &id, "technology", card.tech_stack)?;
        rebuild_card_fts(&tx, &id)?;

        tx.commit()?;
        log::info!(
            "创建卡片: id={}, title={:?}, value={:?}, topics=[{}], technologies=[{}]",
            id,
            card.title,
            card.value,
            tags_joined,
            technologies_joined,
        );
        Ok(id)
    }

    /// 获取卡片完整信息（含关联标签）
    pub fn get_card(&self, id: &str) -> DbResult<Card> {
        let conn = self.read_conn()?;
        let mut card = conn
            .query_row(
                "SELECT c.id, c.session_id, c.analysis_run_id, c.title, c.type, c.value, c.summary, c.note, \
                 c.publication_status, c.is_user_edited, c.source_name, c.project_name, \
                 c.prompt_tokens, c.completion_tokens, c.cost_yuan, c.feedback, \
                 c.created_at, c.updated_at, \
                 sess.external_session_id, COALESCE(sess.raw_path, sess.project_path) \
                 FROM cards c \
                 INNER JOIN sessions sess ON c.session_id = sess.id \
                 WHERE c.id = ?",
                params![id],
                |r| card_from_row(r),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("card {}", id)))?;

        // 关联查询标签名
        let mut stmt = conn.prepare(
            "SELECT t.name, t.kind FROM tags t \
             INNER JOIN card_tags ct ON t.id = ct.tag_id \
             WHERE ct.card_id = ? ORDER BY t.kind, t.name",
        )?;
        for row in stmt.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (name, kind) = row?;
            if kind == "technology" {
                card.tech_stack.push(name);
            } else {
                card.tags.push(name);
            }
        }
        Ok(card)
    }

    pub fn update_card(&self, id: &str, update: &CardUpdate<'_>) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE cards SET title=?1,type=?2,summary=?3,note=?4,is_user_edited=1,updated_at=?5 WHERE id=?6",
            params![update.title, update.card_type, update.summary, update.note, Utc::now().to_rfc3339(), id],
        )?;
        if changed == 0 {
            return Err(DbError::NotFound(format!("card {id}")));
        }
        tx.execute("DELETE FROM card_tags WHERE card_id=?1", params![id])?;
        attach_labels(&tx, id, "topic", update.tags)?;
        attach_labels(&tx, id, "technology", update.technologies)?;
        rebuild_card_fts(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn publish_card(&self, id: &str, replace_existing: bool) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let session_id: String = tx
            .query_row(
                "SELECT session_id FROM cards WHERE id=?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("card {id}")))?;
        if replace_existing {
            let mut stmt = tx.prepare(
                "SELECT id,rowid FROM cards WHERE session_id=?1 AND id<>?2 AND publication_status='published' AND is_user_edited=0",
            )?;
            let old = stmt
                .query_map(params![session_id, id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);
            for (old_id, rowid) in old {
                tx.execute("DELETE FROM cards_fts WHERE rowid=?1", params![rowid])?;
                tx.execute("DELETE FROM cards WHERE id=?1", params![old_id])?;
            }
        }
        tx.execute(
            "UPDATE cards SET publication_status='published',updated_at=?1 WHERE id=?2",
            params![Utc::now().to_rfc3339(), id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_tag_records(&self) -> DbResult<Vec<Tag>> {
        let conn = self.read_conn()?;
        let mut stmt =
            conn.prepare("SELECT id,name,normalized_name,kind FROM tags ORDER BY kind,name")?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    normalized_name: row.get(2)?,
                    kind: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DbError::from)?;
        Ok(tags)
    }

    pub fn merge_tags(&self, kind: &str, sources: &[String], target: &str) -> DbResult<()> {
        if !matches!(kind, "topic" | "technology") || target.trim().is_empty() || sources.is_empty()
        {
            return Err(DbError::Invalid("标签类型、来源或目标无效".into()));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let target_name = target.trim();
        let target_normalized = target_name.to_lowercase();
        tx.execute(
            "INSERT OR IGNORE INTO tags(id,name,normalized_name,kind) VALUES(?1,?2,?3,?4)",
            params![
                Uuid::new_v4().to_string(),
                target_name,
                target_normalized,
                kind
            ],
        )?;
        let target_id: String = tx.query_row(
            "SELECT id FROM tags WHERE kind=?1 AND normalized_name=?2",
            params![kind, target_normalized],
            |row| row.get(0),
        )?;
        let normalized = sources
            .iter()
            .map(|name| name.trim().to_lowercase())
            .collect::<Vec<_>>();
        let placeholders = vec!["?"; normalized.len()].join(",");
        let sql = format!(
            "SELECT id FROM tags WHERE kind=? AND normalized_name IN ({placeholders}) AND id<>?"
        );
        let mut values = vec![kind.to_string()];
        values.extend(normalized);
        values.push(target_id.clone());
        let mut stmt = tx.prepare(&sql)?;
        let source_ids = stmt
            .query_map(rusqlite::params_from_iter(values.iter()), |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        if source_ids.is_empty() {
            return Ok(());
        }
        let source_placeholders = vec!["?"; source_ids.len()].join(",");
        let affected_sql = format!(
            "SELECT DISTINCT card_id FROM card_tags WHERE tag_id IN ({source_placeholders})"
        );
        let mut affected_stmt = tx.prepare(&affected_sql)?;
        let affected = affected_stmt
            .query_map(rusqlite::params_from_iter(source_ids.iter()), |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(affected_stmt);
        for card_id in &affected {
            tx.execute(
                "INSERT OR IGNORE INTO card_tags(card_id,tag_id) VALUES(?1,?2)",
                params![card_id, target_id],
            )?;
        }
        let delete_links = format!("DELETE FROM card_tags WHERE tag_id IN ({source_placeholders})");
        tx.execute(&delete_links, rusqlite::params_from_iter(source_ids.iter()))?;
        let delete_tags = format!("DELETE FROM tags WHERE id IN ({source_placeholders})");
        tx.execute(&delete_tags, rusqlite::params_from_iter(source_ids.iter()))?;
        for card_id in &affected {
            rebuild_card_fts(&tx, card_id)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 分页查询知识卡片列表，支持按类型/价值/标签筛选
    pub fn list_cards(
        &self,
        filters: &CardFilters,
        page: u32,
        page_size: u32,
    ) -> DbResult<PaginatedResult<CardSummary>> {
        let (where_sql, filter_params) = build_card_where(filters);
        let page = page.max(1);
        let limit = page_size as i64;
        let offset = (page - 1) as i64 * limit;

        let conn = self.read_conn()?;

        let count_sql = format!("SELECT COUNT(*) FROM cards c{}", where_sql);
        let total: i64 = conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(filter_params.iter()),
            |r| r.get(0),
        )?;

        let select_sql = format!(
            "SELECT {} FROM cards c{} ORDER BY c.created_at DESC LIMIT {} OFFSET {}",
            CARD_SUMMARY_COLUMNS, where_sql, limit, offset
        );
        let mut stmt = conn.prepare(&select_sql)?;
        let items = stmt
            .query_map(rusqlite::params_from_iter(filter_params.iter()), |r| {
                card_summary_from_row(r)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedResult {
            items,
            total: total as u64,
            page,
            page_size,
        })
    }

    /// 库内卡片总数（不受筛选；供「导出全部」前提示条数）
    pub fn count_all_cards(&self) -> DbResult<u64> {
        let n: i64 = self.read_conn()?.query_row(
            "SELECT COUNT(*) FROM cards WHERE publication_status='published'",
            [],
            |r| r.get(0),
        )?;
        Ok(n.max(0) as u64)
    }

    /// 全部卡片 id（按创建时间升序，批量导出顺序稳定）
    pub fn list_all_card_ids(&self) -> DbResult<Vec<String>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM cards WHERE publication_status='published' ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// 删除卡片（级联清理关联表和 FTS 索引，不存在时返回 NotFound）
    /// 删除某会话关联的全部卡片（含 card_tags 与 FTS 行）。
    ///
    /// 用于重新分析前清理旧笔记，避免同一 session 下多张卡片。
    /// 注意：在单事务内完成，避免与 `delete_card` 嵌套加锁导致死锁。
    pub fn delete_cards_for_session(&self, session_db_id: &str) -> DbResult<u64> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut stmt = tx.prepare("SELECT id FROM cards WHERE session_id = ?")?;
        let ids: Vec<String> = stmt
            .query_map(params![session_db_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for id in &ids {
            tx.execute("DELETE FROM card_tags WHERE card_id = ?", params![id])?;
            tx.execute(
                "DELETE FROM cards_fts WHERE rowid = (SELECT rowid FROM cards WHERE id = ?)",
                params![id],
            )?;
            tx.execute("DELETE FROM cards WHERE id = ?", params![id])?;
        }
        tx.commit()?;
        if !ids.is_empty() {
            log::info!("已删除会话 {} 下的 {} 张旧卡片", session_db_id, ids.len());
        }
        Ok(ids.len() as u64)
    }

    pub fn delete_card(&self, id: &str) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM card_tags WHERE card_id = ?", params![id])?;
        tx.execute(
            "DELETE FROM cards_fts WHERE rowid = (SELECT rowid FROM cards WHERE id = ?)",
            params![id],
        )?;
        let n = tx.execute("DELETE FROM cards WHERE id = ?", params![id])?;
        if n == 0 {
            // 事务会在 drop 时自动回滚
            return Err(DbError::NotFound(format!("card {}", id)));
        }
        tx.commit()?;
        log::info!("删除卡片: id={}", id);
        Ok(())
    }

    pub fn update_card_feedback(&self, id: &str, feedback: &str) -> DbResult<()> {
        let n = self.conn().execute(
            "UPDATE cards SET feedback = ? WHERE id = ?",
            params![feedback, id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(format!("card {}", id)));
        }
        log::debug!("卡片反馈更新: id={}, feedback={}", id, feedback);
        Ok(())
    }

    /// 查询所有标签及其关联的卡片数量（按数量降序），用于知识库侧栏标签筛选。
    pub fn list_all_tags(&self) -> DbResult<Vec<TagCount>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT t.name, COUNT(ct.card_id) as cnt
             FROM tags t
             LEFT JOIN card_tags ct ON t.id = ct.tag_id
             LEFT JOIN cards c ON c.id = ct.card_id
             WHERE t.kind = 'topic' AND c.publication_status = 'published'
             GROUP BY t.id, t.name
             HAVING cnt > 0
             ORDER BY cnt DESC, t.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TagCount {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// 聚合 tags.kind=technology 的已发布卡片数量。
    pub fn list_all_tech_stack_counts(&self) -> DbResult<Vec<TagCount>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT t.name, COUNT(ct.card_id) AS cnt
             FROM tags t
             JOIN card_tags ct ON ct.tag_id = t.id
             JOIN cards c ON c.id = ct.card_id
             WHERE t.kind = 'technology' AND c.publication_status = 'published'
             GROUP BY t.id, t.name
             ORDER BY cnt DESC, t.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TagCount {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// 按知识类型统计卡片数量（按数量降序），用于知识库侧栏类型筛选。
    pub fn list_card_type_counts(&self) -> DbResult<Vec<TypeCount>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            r#"SELECT "type", COUNT(*) as cnt
               FROM cards
               WHERE publication_status = 'published'
               GROUP BY "type"
               ORDER BY cnt DESC, "type" ASC"#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TypeCount {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_topic_and_technology_tags_and_hides_drafts_from_library() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("cards.db")).unwrap();
        let session_id = db
            .insert_session(
                "codex",
                "external",
                "local",
                None,
                None,
                0,
                Some("hash"),
                "/tmp/source",
                "2026-08-30T00:00:00Z",
                "2026-08-30T00:00:00Z",
                None,
            )
            .unwrap();
        let topics = vec!["全文检索".to_string()];
        let technologies = vec!["SQLite".to_string(), "Rust".to_string()];
        let job = db
            .create_job(
                "analysis",
                "queued",
                Some((
                    "profile",
                    "siliconflow",
                    "https://api.siliconflow.cn/v1",
                    "model",
                )),
                &[NewJobItem {
                    session_id: Some(&session_id),
                    source_id: None,
                    raw_path: None,
                }],
            )
            .unwrap();
        let run_id = db
            .create_analysis_run(
                &job.id,
                &session_id,
                "profile",
                "siliconflow",
                "api.siliconflow.cn",
                "model",
                "hash",
                "test",
            )
            .unwrap();
        let high_id = db
            .insert_card(&NewCard {
                session_id: &session_id,
                analysis_run_id: &run_id,
                title: "SQLite FTS5 索引",
                card_type: "implementation",
                value: "high",
                summary: "建立 trigram 索引",
                note: "SQLite FTS5 trigram",
                publication_status: "published",
                source_name: Some("Codex"),
                project_name: None,
                prompt_tokens: 1,
                completion_tokens: 1,
                cost_yuan: 0.0,
                tags: &topics,
                tech_stack: &technologies,
            })
            .unwrap();
        assert_eq!(
            db.get_session(&session_id).unwrap().card_id,
            Some(high_id.clone())
        );
        let draft_id = db
            .insert_card(&NewCard {
                session_id: &session_id,
                analysis_run_id: &run_id,
                title: "草稿",
                card_type: "explanation",
                value: "medium",
                summary: "待确认",
                note: "draft body",
                publication_status: "draft",
                source_name: Some("Codex"),
                project_name: None,
                prompt_tokens: 1,
                completion_tokens: 1,
                cost_yuan: 0.0,
                tags: &topics,
                tech_stack: &technologies,
            })
            .unwrap();

        assert_eq!(
            db.get_card(&high_id).unwrap().publication_status,
            "published"
        );
        assert_eq!(db.get_card(&draft_id).unwrap().publication_status, "draft");
        assert_eq!(
            db.list_cards(&CardFilters::default(), 1, 20).unwrap().total,
            1
        );
        assert_eq!(db.list_all_tags().unwrap()[0].name, "全文检索");
        assert_eq!(db.list_all_tech_stack_counts().unwrap().len(), 2);
        assert_eq!(
            db.search_cards("SQLite", &CardFilters::default())
                .unwrap()
                .len(),
            1
        );

        db.update_card(
            &draft_id,
            &CardUpdate {
                title: "人工整理草稿",
                card_type: "explanation",
                summary: "已校订",
                note: "manual SQLite note",
                tags: &["数据库".into()],
                technologies: &["SQLite".into()],
            },
        )
        .unwrap();
        assert!(db.get_card(&draft_id).unwrap().is_user_edited);
        db.publish_card(&draft_id, false).unwrap();
        db.merge_tags("topic", &["数据库".into(), "全文检索".into()], "数据检索")
            .unwrap();
        assert_eq!(db.get_card(&draft_id).unwrap().tags, vec!["数据检索"]);

        let replacement = db
            .insert_card(&NewCard {
                session_id: &session_id,
                analysis_run_id: &run_id,
                title: "替换草稿",
                card_type: "decision",
                value: "high",
                summary: "替换旧自动知识",
                note: "replacement",
                publication_status: "draft",
                source_name: Some("Codex"),
                project_name: None,
                prompt_tokens: 1,
                completion_tokens: 1,
                cost_yuan: 0.0,
                tags: &[],
                tech_stack: &[],
            })
            .unwrap();
        db.publish_card(&replacement, true).unwrap();
        assert!(db.get_card(&high_id).is_err());
        assert!(
            db.get_card(&draft_id).is_ok(),
            "人工编辑的旧知识不得自动删除"
        );
    }
}
