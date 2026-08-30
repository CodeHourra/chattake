//! 采集调度器 —— 协调各数据源采集器，执行去重写入，记录同步日志。
//!
//! ```text
//! collect_all()
//!   ├── Claude Code Collector → Vec<NormalizedSession>
//!   ├── Cursor Collector       → Vec<NormalizedSession>
//!   ├── CodeBuddy Collector      → Vec<NormalizedSession>
//!   └── (future: other collectors...)
//!           │
//!           ▼
//!   dedup_and_write() — 逐条去重写入 SQLite
//!           │
//!           ▼
//!   SyncResult { found, new, updated, skipped }
//! ```

use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::storage::Database;

use super::claude_code::ClaudeCodeCollector;
use super::codebuddy::CodeBuddyCollector;
use super::codex::CodexCollector;
use super::cursor::CursorCollector;
use super::normalizer::NormalizedSession;

/// 同步结果统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// 扫描发现的会话总数
    pub found: u32,
    /// 新导入的会话数
    pub new: u32,
    /// 检测到有新消息的会话数
    pub updated: u32,
    /// 无变化跳过的会话数
    pub skipped: u32,
    pub failed: u32,
}

/// 采集调度器，持有配置和数据库引用
pub struct CollectorScheduler<'a> {
    config: &'a AppConfig,
    db: &'a Database,
}

impl<'a> CollectorScheduler<'a> {
    pub fn new(config: &'a AppConfig, db: &'a Database) -> Self {
        Self { config, db }
    }

    pub fn collect_source_with_progress(
        &self,
        source_id: &str,
        mut on_file: impl FnMut(&str, &str, Option<&str>),
    ) -> SyncResult {
        let Some(source) = self
            .config
            .enabled_sources()
            .into_iter()
            .find(|source| source.id == source_id)
        else {
            return SyncResult::default();
        };
        log::info!("开始采集数据源: {} ({})", source.name, source.id);
        let scan_dirs = source.resolved_scan_dirs();
        let sessions: Vec<NormalizedSession> = match source.id.as_str() {
            "claude-code" => {
                let collector = ClaudeCodeCollector::new(scan_dirs);
                collector.collect_changed(|path, mtime, size| {
                    self.db
                        .source_file_unchanged(&source.id, path, mtime, size)
                        .unwrap_or(false)
                })
            }
            "cursor" => {
                let collector = CursorCollector::new(scan_dirs);
                collector.collect_changed(|path, mtime, size| {
                    self.db
                        .source_file_unchanged(&source.id, path, mtime, size)
                        .unwrap_or(false)
                })
            }
            "codebuddy" => {
                let collector = CodeBuddyCollector::new(scan_dirs);
                collector.collect_changed(|path, mtime, size| {
                    self.db
                        .source_file_unchanged(&source.id, path, mtime, size)
                        .unwrap_or(false)
                })
            }
            "codex" => {
                let collector = CodexCollector::new(scan_dirs);
                collector.collect_changed(|path, mtime, size| {
                    self.db
                        .source_file_unchanged(&source.id, path, mtime, size)
                        .unwrap_or(false)
                })
            }
            other => {
                log::warn!("未知数据源类型: {}", other);
                return SyncResult::default();
            }
        };

        let result = self.dedup_and_write(&sessions, &mut on_file);
        log::info!(
            "数据源 {} 采集完成: 发现={}, 新增={}, 更新={}, 跳过={}",
            source.name,
            result.found,
            result.new,
            result.updated,
            result.skipped
        );
        result
    }

    /// 对采集到的会话逐条去重写入。
    ///
    /// 去重策略：
    /// - session_id + source_host 不存在 → INSERT（新增）
    /// - 存在 + message_count 增加    → 标记 has_updates（更新）
    /// - 存在 + message_count 不变    → SKIP（跳过）
    fn dedup_and_write(
        &self,
        sessions: &[NormalizedSession],
        on_file: &mut impl FnMut(&str, &str, Option<&str>),
    ) -> SyncResult {
        let mut result = SyncResult {
            found: sessions.len() as u32,
            ..Default::default()
        };

        for session in sessions {
            match self.write_one_session(session) {
                Ok(WriteAction::New) => {
                    result.new += 1;
                    on_file(&session.raw_path, "imported", None);
                }
                Ok(WriteAction::Updated) => {
                    result.updated += 1;
                    on_file(&session.raw_path, "updated", None);
                }
                Ok(WriteAction::Skipped) => {
                    result.skipped += 1;
                    on_file(&session.raw_path, "skipped", None);
                }
                Err(e) => {
                    log::error!(
                        "写入会话失败: session_id={}, error={}",
                        session.session_id,
                        e
                    );
                    result.failed += 1;
                    on_file(&session.raw_path, "failed", Some(&e.to_string()));
                }
            }
        }

        result
    }

    /// 处理单条会话的去重写入
    fn write_one_session(
        &self,
        session: &NormalizedSession,
    ) -> Result<WriteAction, Box<dyn std::error::Error>> {
        let source_host = "local";
        let message_count = session.messages.len() as i32;
        let content_hash = session_content_hash(session);
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let existing: Option<(String, Option<String>, i64)> = tx.query_row(
            "SELECT id, content_hash, message_count FROM sessions WHERE source_id=?1 AND external_session_id=?2 AND source_host=?3",
            params![session.source_id, session.session_id, source_host],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;

        let action = match existing {
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO sessions (id,source_id,external_session_id,source_host,project_path,project_name,message_count,content_hash,raw_path,raw_mtime_ms,raw_size_bytes,created_at,updated_at,analysis_title)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                    params![id, session.source_id, session.session_id, source_host, session.project_path,
                        session.project_name, message_count, content_hash, session.raw_path,
                        session.raw_mtime_ms, session.raw_size_bytes, session.created_at,
                        session.updated_at, session.analysis_title],
                )?;
                insert_messages_tx(&tx, &id, session)?;
                WriteAction::New
            }
            Some((id, old_hash, old_count)) => {
                if old_hash.as_deref() == Some(&content_hash) {
                    tx.execute(
                        "UPDATE sessions SET raw_mtime_ms=?1,raw_size_bytes=?2,updated_at=?3 WHERE id=?4",
                        params![session.raw_mtime_ms, session.raw_size_bytes, session.updated_at, id],
                    )?;
                    WriteAction::Skipped
                } else {
                    tx.execute("DELETE FROM messages WHERE session_id=?1", params![id])?;
                    insert_messages_tx(&tx, &id, session)?;
                    tx.execute(
                        "UPDATE sessions SET message_count=?1,content_hash=?2,raw_path=?3,raw_mtime_ms=?4,raw_size_bytes=?5,
                         project_path=?6,project_name=?7,analysis_title=COALESCE(?8,analysis_title),updated_at=?9,has_updates=1 WHERE id=?10",
                        params![message_count, content_hash, session.raw_path, session.raw_mtime_ms,
                            session.raw_size_bytes, session.project_path, session.project_name,
                            session.analysis_title, session.updated_at, id],
                    )?;
                    log::info!(
                        "会话消息已更新: session_id={}, {} → {} 条消息",
                        session.session_id,
                        old_count,
                        message_count
                    );
                    WriteAction::Updated
                }
            }
        };
        tx.commit()?;
        Ok(action)
    }
}

fn insert_messages_tx(
    tx: &Transaction<'_>,
    session_id: &str,
    session: &NormalizedSession,
) -> rusqlite::Result<()> {
    for (seq, message) in session.messages.iter().enumerate() {
        tx.execute(
            "INSERT INTO messages(id,session_id,role,content,timestamp,tokens_in,tokens_out,seq_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![uuid::Uuid::new_v4().to_string(), session_id, message.role, message.content,
                message.timestamp, message.tokens_in, message.tokens_out, seq as i64],
        )?;
    }
    Ok(())
}

fn session_content_hash(session: &NormalizedSession) -> String {
    let mut context = md5::Context::new();
    for message in &session.messages {
        context.consume(message.role.as_bytes());
        context.consume([0]);
        context.consume(message.content.as_bytes());
        context.consume([0]);
        if let Some(timestamp) = &message.timestamp {
            context.consume(timestamp.as_bytes());
        }
        context.consume([0xff]);
    }
    format!("{:x}", context.compute())
}

/// 去重写入的动作结果
#[derive(Debug, PartialEq, Eq)]
enum WriteAction {
    /// 新会话，已写入
    New,
    /// 已存在但有新消息，已标记更新
    Updated,
    /// 已存在且无变化，跳过
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::normalizer::NormalizedMessage;

    fn session(contents: &[&str], size: i64) -> NormalizedSession {
        NormalizedSession {
            source_id: "codex".into(),
            session_id: "same-id".into(),
            project_path: Some("/tmp/project".into()),
            project_name: Some("project".into()),
            analysis_title: None,
            messages: contents
                .iter()
                .enumerate()
                .map(|(index, content)| NormalizedMessage {
                    role: if index % 2 == 0 {
                        "user".into()
                    } else {
                        "assistant".into()
                    },
                    content: (*content).into(),
                    timestamp: None,
                    tokens_in: 0,
                    tokens_out: 0,
                })
                .collect(),
            raw_path: "/tmp/codex.jsonl".into(),
            raw_mtime_ms: Some(size),
            raw_size_bytes: Some(size),
            created_at: "2026-08-30T00:00:00Z".into(),
            updated_at: "2026-08-30T00:00:01Z".into(),
        }
    }

    #[test]
    fn detects_same_count_edits_and_message_removal() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let config = AppConfig::default();
        let scheduler = CollectorScheduler::new(&config, &db);

        assert_eq!(
            scheduler
                .write_one_session(&session(&["a", "b"], 1))
                .unwrap(),
            WriteAction::New
        );
        assert_eq!(
            scheduler
                .write_one_session(&session(&["a", "changed"], 2))
                .unwrap(),
            WriteAction::Updated
        );
        let id = db
            .check_duplicate("codex", "same-id", "local")
            .unwrap()
            .unwrap();
        assert_eq!(db.get_session_messages(&id).unwrap()[1].content, "changed");

        assert_eq!(
            scheduler.write_one_session(&session(&["a"], 3)).unwrap(),
            WriteAction::Updated
        );
        assert_eq!(db.get_session(&id).unwrap().message_count, 1);
        assert_eq!(db.get_session_messages(&id).unwrap().len(), 1);
    }
}
