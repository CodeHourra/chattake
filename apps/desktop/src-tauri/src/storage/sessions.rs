use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;
use rusqlite::params;
use rusqlite::Transaction;
use uuid::Uuid;

use super::db::{Database, DbError, DbResult};
use super::models::*;

/// 侧栏树第三层占位：与 `Sidebar.vue` 中 `item.project ?? '(未关联项目)'` 一致，
/// 对应数据库中 `project_name` 为 NULL 的会话行。
const UNLINKED_PROJECT_LABEL: &str = "(未关联项目)";

/// 与 `list_sessions` / `delete_sessions_by_filter_groups` 共用的动态 WHERE 片段。
///
/// 返回 `(WHERE 子句含关键字或空串, 绑定参数)`；无筛选条件时 WHERE 为空。
fn session_filters_where_clause(
    filters: &SessionFilters,
) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
    let mut conditions = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref source) = filters.source {
        conditions.push("source_id = ?");
        param_values.push(Box::new(source.clone()));
    }
    if let Some(ref host) = filters.host {
        conditions.push("source_host = ?");
        param_values.push(Box::new(host.clone()));
    }
    if let Some(ref project) = filters.project {
        if project == UNLINKED_PROJECT_LABEL {
            // 未关联项目：库内为 NULL，不能与字面量「(未关联项目)」等值匹配
            conditions.push("project_name IS NULL");
        } else {
            conditions.push("(project_name = ? OR project_path = ?)");
            param_values.push(Box::new(project.clone()));
            param_values.push(Box::new(project.clone()));
        }
    }
    if let Some(ref status) = filters.status {
        conditions.push("status = ?");
        param_values.push(Box::new(status.clone()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    (where_clause, param_values)
}

/// 单会话级联删除（与 `cards::delete_cards_for_session` 逻辑一致，放在同一事务内供批量调用）。
fn delete_session_cascade_tx(tx: &Transaction<'_>, session_db_id: &str) -> DbResult<()> {
    let mut stmt = tx.prepare("SELECT id FROM cards WHERE session_id = ?")?;
    let card_ids: Vec<String> = stmt
        .query_map(params![session_db_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for cid in &card_ids {
        tx.execute("DELETE FROM card_tags WHERE card_id = ?", params![cid])?;
        tx.execute(
            "DELETE FROM cards_fts WHERE rowid = (SELECT rowid FROM cards WHERE id = ?)",
            params![cid],
        )?;
        tx.execute("DELETE FROM cards WHERE id = ?", params![cid])?;
    }

    tx.execute(
        "DELETE FROM messages WHERE session_id = ?",
        params![session_db_id],
    )?;
    let n = tx.execute("DELETE FROM sessions WHERE id = ?", params![session_db_id])?;
    if n == 0 {
        return Err(DbError::NotFound(format!("session {}", session_db_id)));
    }
    Ok(())
}

/// 从查询行映射 SessionSummary（列顺序需与 SESSION_SUMMARY_COLUMNS 一致）
fn session_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSummary> {
    Ok(SessionSummary {
        id: row.get(0)?,
        source_id: row.get(1)?,
        session_id: row.get(2)?,
        source_host: row.get(3)?,
        project_path: row.get(4)?,
        project_name: row.get(5)?,
        message_count: row.get(6)?,
        status: row.get(7)?,
        value: row.get(8)?,
        updated_at: row.get(9)?,
        // SQLite 存储 INTEGER 0/1，需手动转 bool
        has_updates: row.get::<_, i64>(10)? != 0,
        created_at: row.get(11)?,
        card_id: row.get(12)?,
        raw_size_bytes: row.get::<_, i64>(13).unwrap_or(0),
        card_title: row.get(14)?,
        card_summary: row.get(15)?,
        card_type: row.get(16)?,
        card_tags: row.get(17)?,
        raw_path: row.get(18)?,
        error_message: row.get(19)?,
        first_user_preview: row.get(20)?,
    })
}

/// 列表查询列：须与 `session_summary_from_row` 下标一致。
///
/// 列索引：
/// - 0-11 : sessions 基础字段
/// - 12   : card_id
/// - 13   : raw_size_bytes
/// - 14   : card_title  → COALESCE(card.title,  s.analysis_title)
/// - 15   : card_summary → COALESCE(card.summary, s.analysis_note)
/// - 16   : card_type   → COALESCE(card.type,   s.analysis_type)
/// - 17   : card_tags
/// - 18   : raw_path
/// - 19   : error_message
/// - 20   : first_user_preview（首条 user，SUBSTR 防超大文本）
///
/// card_title / card_summary / card_type 均以 analysis_* 列兜底，
/// 确保低/无价值会话（无 Card 产出）在列表中也能展示类型徽章、标题和摘要。
const SESSION_SUMMARY_COLUMNS: &str = "\
    s.id, s.source_id, s.external_session_id, s.source_host, s.project_path, s.project_name, \
    s.message_count, s.status, s.value, s.updated_at, s.has_updates, s.created_at, \
    (SELECT c.id      FROM cards c WHERE c.session_id = s.id ORDER BY c.created_at DESC LIMIT 1), \
    (SELECT COALESCE(SUM(LENGTH(m.content)), 0) FROM messages m WHERE m.session_id = s.id), \
    COALESCE(\
        (SELECT c.title   FROM cards c WHERE c.session_id = s.id ORDER BY c.created_at DESC LIMIT 1), \
        s.analysis_title\
    ), \
    COALESCE(\
        (SELECT c.summary FROM cards c WHERE c.session_id = s.id ORDER BY c.created_at DESC LIMIT 1), \
        s.analysis_note\
    ), \
    COALESCE(\
        (SELECT c.\"type\" FROM cards c WHERE c.session_id = s.id ORDER BY c.created_at DESC LIMIT 1), \
        s.analysis_type\
    ), \
    (SELECT GROUP_CONCAT(t.name, ',') \
       FROM card_tags ct \
       JOIN tags t ON ct.tag_id = t.id \
       WHERE ct.card_id = ( \
           SELECT c.id FROM cards c WHERE c.session_id = s.id ORDER BY c.created_at DESC LIMIT 1 \
       )), \
    s.raw_path, s.error_message, \
    (SELECT SUBSTR(m.content, 1, 4096) FROM messages m \
       WHERE m.session_id = s.id AND m.role = 'user' \
       ORDER BY m.seq_order ASC LIMIT 1)";

impl Database {
    pub fn source_file_unchanged(
        &self,
        source_id: &str,
        raw_path: &Path,
        mtime_ms: i64,
        size_bytes: i64,
    ) -> DbResult<bool> {
        let path = raw_path.to_string_lossy();
        self.read_conn()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE source_id=?1 AND raw_path=?2 AND raw_mtime_ms=?3 AND raw_size_bytes=?4)",
            params![source_id, path.as_ref(), mtime_ms, size_bytes],
            |row| row.get(0),
        ).map_err(DbError::from)
    }

    /// 导入会话。使用 INSERT OR IGNORE 实现去重（唯一键: source + external id + host）。
    ///
    /// 返回数据库主键 ID（新插入时为新 UUID，冲突时返回已有记录的 ID）。
    pub fn insert_session(
        &self,
        source_id: &str,
        session_id: &str,
        source_host: &str,
        project_path: Option<&str>,
        project_name: Option<&str>,
        message_count: i32,
        content_hash: Option<&str>,
        raw_path: &str,
        created_at: &str,
        updated_at: &str,
        analysis_title: Option<&str>,
    ) -> DbResult<String> {
        let id = Uuid::new_v4().to_string();
        let conn = self.conn();
        let rows = conn.execute(
            "INSERT OR IGNORE INTO sessions (
                id, source_id, external_session_id, source_host, project_path, project_name,
                message_count, content_hash, raw_path, created_at, updated_at, analysis_title
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &id,
                source_id,
                session_id,
                source_host,
                project_path,
                project_name,
                message_count,
                content_hash,
                raw_path,
                created_at,
                updated_at,
                analysis_title,
            ],
        )?;

        if rows == 0 {
            // 唯一约束冲突 → 查询已有记录的 ID 返回
            let existing: String = conn.query_row(
                "SELECT id FROM sessions WHERE source_id = ?1 AND external_session_id = ?2 AND source_host = ?3",
                params![source_id, session_id, source_host],
                |row| row.get(0),
            )?;
            log::debug!("会话已存在: session_id={}, db_id={}", session_id, existing);
            Ok(existing)
        } else {
            log::info!(
                "导入会话: source={}, project={:?}, messages={}",
                source_id,
                project_name,
                message_count
            );
            Ok(id)
        }
    }

    /// 批量写入消息（事务内执行，保证原子性）
    pub fn insert_messages(&self, session_db_id: &str, messages: &[NewMessage]) -> DbResult<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        for (seq_order, msg) in messages.iter().enumerate() {
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO messages (
                    id, session_id, role, content, timestamp, tokens_in, tokens_out, seq_order
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    session_db_id,
                    msg.role,
                    msg.content,
                    msg.timestamp,
                    msg.tokens_in,
                    msg.tokens_out,
                    seq_order as i32
                ],
            )?;
        }
        tx.commit()?;
        log::debug!("写入 {} 条消息 → session {}", messages.len(), session_db_id);
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> DbResult<Session> {
        let conn = self.read_conn()?;
        conn.query_row(
            "SELECT id, source_id, external_session_id, source_host, project_path, project_name,
                    message_count, content_hash, raw_path, created_at, updated_at,
                    status, value, has_updates, analyzed_at, error_message, analysis_title
             FROM sessions WHERE id = ?1",
            params![id],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    session_id: row.get(2)?,
                    source_host: row.get(3)?,
                    project_path: row.get(4)?,
                    project_name: row.get(5)?,
                    message_count: row.get(6)?,
                    content_hash: row.get(7)?,
                    raw_path: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    status: row.get(11)?,
                    value: row.get(12)?,
                    has_updates: row.get::<_, i64>(13)? != 0,
                    analyzed_at: row.get(14)?,
                    error_message: row.get(15)?,
                    analysis_title: row.get(16)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(id.to_string()),
            _ => e.into(),
        })
    }

    pub fn get_session_messages(&self, session_id: &str) -> DbResult<Vec<Message>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, timestamp, tokens_in, tokens_out, seq_order
             FROM messages WHERE session_id = ?1 ORDER BY seq_order ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                timestamp: row.get(4)?,
                tokens_in: row.get(5)?,
                tokens_out: row.get(6)?,
                seq_order: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// 分页查询会话列表，支持按数据源/项目/状态动态筛选。
    ///
    /// 分页参数: page 从 1 开始，page=0 等同于 page=1。
    pub fn list_sessions(
        &self,
        filters: &SessionFilters,
        page: u32,
        page_size: u32,
    ) -> DbResult<PaginatedResult<SessionSummary>> {
        let (where_clause, param_values) = session_filters_where_clause(filters);

        let conn = self.read_conn()?;

        // 先查总数
        let count_sql = format!("SELECT COUNT(*) FROM sessions{}", where_clause);
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();
        let total: i64 = conn.query_row(&count_sql, param_refs.as_slice(), |row| row.get(0))?;

        // 再查当页数据
        let offset = page.saturating_sub(1) as i64 * page_size as i64;
        let list_sql = format!(
            "SELECT {} FROM sessions s{} ORDER BY s.created_at DESC LIMIT ? OFFSET ?",
            SESSION_SUMMARY_COLUMNS, where_clause
        );

        let mut data_params = param_values;
        data_params.push(Box::new(page_size as i64));
        data_params.push(Box::new(offset));
        let data_refs: Vec<&dyn rusqlite::types::ToSql> =
            data_params.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&list_sql)?;
        let items = stmt
            .query_map(data_refs.as_slice(), |row| session_summary_from_row(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedResult {
            items,
            total: total.max(0) as u64,
            page,
            page_size,
        })
    }

    /// 多组筛选条件的会话 id 并集（去重），每组语义与 `list_sessions` 单组筛选一致。
    ///
    /// ```text
    /// group1 OR group2 OR ...  →  HashSet<id>
    /// ```
    fn collect_session_ids_union(&self, groups: &[SessionFilters]) -> DbResult<Vec<String>> {
        let mut set: HashSet<String> = HashSet::new();
        for g in groups {
            let (where_clause, param_values) = session_filters_where_clause(g);
            let list_sql = format!("SELECT id FROM sessions{}", where_clause);
            let conn = self.read_conn()?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|b| b.as_ref()).collect();
            let mut stmt = conn.prepare(&list_sql)?;
            let chunk: Vec<String> = stmt
                .query_map(param_refs.as_slice(), |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for id in chunk {
                set.insert(id);
            }
        }
        Ok(set.into_iter().collect())
    }

    /// 统计多组筛选并集下的会话数量（用于确认弹窗）。
    pub fn count_sessions_by_filter_groups(&self, groups: &[SessionFilters]) -> DbResult<u64> {
        let ids = self.collect_session_ids_union(groups)?;
        Ok(ids.len() as u64)
    }

    /// 按多组筛选并集批量删除会话（侧栏「会话整理」多选）。
    ///
    /// 级联删除：card_tags → cards_fts → cards → messages → sessions。
    /// 仅删除应用库内记录，不修改本地源文件。
    pub fn delete_sessions_by_filter_groups(&self, groups: &[SessionFilters]) -> DbResult<u64> {
        let ids = self.collect_session_ids_union(groups)?;
        if ids.is_empty() {
            log::info!("按多组筛选批量删除会话：无匹配行");
            return Ok(0);
        }

        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for id in &ids {
            delete_session_cascade_tx(&tx, id)?;
        }
        tx.commit()?;
        log::info!(
            "按多组筛选批量删除会话完成：共 {} 条（组数={}）",
            ids.len(),
            groups.len()
        );
        Ok(ids.len() as u64)
    }

    /// 检查是否已存在相同的会话（去重键: source_id + external_session_id + source_host）
    pub fn check_duplicate(
        &self,
        source_id: &str,
        session_id: &str,
        source_host: &str,
    ) -> DbResult<Option<String>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM sessions WHERE source_id = ?1 AND external_session_id = ?2 AND source_host = ?3 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![source_id, session_id, source_host])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// 将会话标记为分析失败，写入 error_message 供前端展示。
    pub fn update_session_error(&self, id: &str, message: &str) -> DbResult<()> {
        let n = self.conn().execute(
            "UPDATE sessions SET status = 'error', error_message = ?1 WHERE id = ?2",
            params![message, id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(id.to_string()));
        }
        log::warn!("会话分析失败: id={}, msg={}", id, message);
        Ok(())
    }

    /// 更新会话分析状态。status 为 "analyzed" 时自动设置 analyzed_at 为当前时间。
    pub fn update_session_status(
        &self,
        id: &str,
        status: &str,
        value: Option<&str>,
    ) -> DbResult<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE sessions SET status = ?1, value = ?2,
                analyzed_at = CASE WHEN ?3 = 'analyzed' THEN ?4 ELSE analyzed_at END
             WHERE id = ?5",
            params![status, value, status, now, id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(id.to_string()));
        }
        log::info!(
            "会话状态变更: id={}, status={}, value={:?}",
            id,
            status,
            value
        );
        Ok(())
    }

    /// 将低/无价值判断的原因、标题、类型批量写入 sessions 表，
    /// 供列表展示（刷新后仍可见），无需依赖前端内存状态。
    pub fn update_session_analysis_meta(
        &self,
        id: &str,
        title: &str,
        card_type: &str,
        note: &str,
    ) -> DbResult<()> {
        let n = self.conn().execute(
            "UPDATE sessions SET analysis_title = ?1, analysis_type = ?2, analysis_note = ?3 WHERE id = ?4",
            params![title, card_type, note, id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(id.to_string()));
        }
        log::debug!(
            "写入 analysis_meta: id={}, type={}, title={}",
            id,
            card_type,
            title
        );
        Ok(())
    }

    /// 标记会话有新消息（增量同步检测到 message_count 变化时调用）
    pub fn mark_has_updates(&self, id: &str) -> DbResult<()> {
        let n = self.conn().execute(
            "UPDATE sessions SET has_updates = 1 WHERE id = ?1",
            params![id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn update_session_message_count(&self, id: &str, count: i32) -> DbResult<()> {
        let n = self.conn().execute(
            "UPDATE sessions SET message_count = ?1 WHERE id = ?2",
            params![count, id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// 增量同步后刷新会话元数据（CodeBuddy 等数据源重扫时更新项目名与展示标题）
    ///
    /// `analysis_title`：采集端有值（如 CodeBuddy 工作区 index）时写入；为 `None` 时不覆盖库内已有列，
    /// 避免 Claude/Cursor 等恒为 `None` 的重同步把 `update_session_analysis_meta` 写入的展示标题抹成 NULL。
    pub fn update_session_resync_metadata(
        &self,
        id: &str,
        message_count: i32,
        project_path: Option<&str>,
        project_name: Option<&str>,
        analysis_title: Option<&str>,
    ) -> DbResult<()> {
        let n = self.conn().execute(
            "UPDATE sessions SET message_count = ?1, project_path = ?2, project_name = ?3, analysis_title = COALESCE(?4, analysis_title) WHERE id = ?5",
            params![message_count, project_path, project_name, analysis_title, id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// 将所有残留的 `analyzing` 状态重置为 `pending`。
    ///
    /// 应用启动时调用：如果上一次运行中途退出，可能有会话永远卡在 `analyzing`。
    /// 重置为 `pending` 允许用户重新触发分析。
    pub fn reset_stale_analyzing(&self) -> DbResult<usize> {
        let n = self.conn().execute(
            "UPDATE sessions SET status = 'pending', error_message = NULL WHERE status = 'analyzing'",
            [],
        )?;
        if n > 0 {
            log::info!(
                "启动清理：已将 {} 个残留 analyzing 状态的会话重置为 pending",
                n
            );
        }
        Ok(n)
    }

    /// 删除会话下的所有消息（用于增量同步时重新导入）
    pub fn delete_session_messages(&self, session_db_id: &str) -> DbResult<u64> {
        let conn = self.conn();
        let deleted = conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_db_id],
        )?;
        log::debug!("删除会话 {} 的 {} 条旧消息", session_db_id, deleted);
        Ok(deleted as u64)
    }

    /// 按 source_id → source_host → project_name 分组统计会话数量，用于构建侧栏目录树。
    ///
    /// 返回扁平的分组列表，前端负责组装成树结构。
    pub fn get_session_groups(&self) -> DbResult<Vec<SessionGroupCount>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT source_id, source_host, project_name, COUNT(*) as cnt
             FROM sessions
             GROUP BY source_id, source_host, project_name
             ORDER BY source_id, source_host, project_name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionGroupCount {
                source_id: row.get(0)?,
                source_host: row.get(1)?,
                project_name: row.get(2)?,
                count: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::Database;

    /// 回归：Claude/Cursor 重同步时采集端 `analysis_title` 为 None，不得抹掉
    /// `update_session_analysis_meta` 已写入的展示标题（见 docs/踩坑 同名文档）。
    #[test]
    fn resync_metadata_preserves_analysis_title_when_collector_sends_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("t.db");
        let db = Database::open(&db_path).expect("open db");

        let id = db
            .insert_session(
                "cursor",
                "sess-1",
                "local",
                None,
                None,
                1,
                None,
                "/tmp/raw",
                "2025-01-01T00:00:00Z",
                "2025-01-01T00:00:00Z",
                None,
            )
            .expect("insert");

        db.update_session_analysis_meta(&id, "分析写入的标题", "low", "note")
            .expect("analysis meta");

        db.update_session_resync_metadata(&id, 3, None, None, None)
            .expect("resync");

        let s = db.get_session(&id).expect("get");
        assert_eq!(
            s.analysis_title.as_deref(),
            Some("分析写入的标题"),
            "COALESCE(NULL, analysis_title) 应保留库内标题"
        );
    }

    /// 采集端显式提供标题时仍应更新（如 CodeBuddy index 名称）。
    #[test]
    fn resync_metadata_updates_analysis_title_when_collector_provides_some() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("t2.db");
        let db = Database::open(&db_path).expect("open db");

        let id = db
            .insert_session(
                "codebuddy",
                "sess-2",
                "local",
                None,
                None,
                1,
                None,
                "/tmp/raw2",
                "2025-01-01T00:00:00Z",
                "2025-01-01T00:00:00Z",
                None,
            )
            .expect("insert");

        db.update_session_resync_metadata(&id, 2, None, None, Some("来自采集的标题"))
            .expect("resync");

        let s = db.get_session(&id).expect("get");
        assert_eq!(s.analysis_title.as_deref(), Some("来自采集的标题"));
    }
}
