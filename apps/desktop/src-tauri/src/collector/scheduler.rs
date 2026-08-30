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

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::storage::models::NewMessage;
use crate::storage::Database;

use super::claude_code::ClaudeCodeCollector;
use super::codebuddy::CodeBuddyCollector;
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

    /// 执行全量采集：遍历所有启用的数据源，采集 → 去重 → 写入
    pub fn collect_all(&self) -> SyncResult {
        let mut total_result = SyncResult::default();

        let enabled_ids: Vec<&str> = self
            .config
            .enabled_sources()
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        log::info!(
            "本次同步将采集的数据源 id 列表（仅已启用）: {:?}；未在列表中的数据源不会扫描",
            enabled_ids
        );

        for source in self.config.enabled_sources() {
            log::info!("开始采集数据源: {} ({})", source.name, source.id);
            let scan_dirs = source.resolved_scan_dirs();

            let sessions: Vec<NormalizedSession> = match source.id.as_str() {
                "claude-code" => {
                    let collector = ClaudeCodeCollector::new(scan_dirs);
                    collector.collect()
                }
                "cursor" => {
                    let collector = CursorCollector::new(scan_dirs);
                    collector.collect()
                }
                "codebuddy" => {
                    let collector = CodeBuddyCollector::new(scan_dirs);
                    collector.collect()
                }
                other => {
                    log::warn!("未知数据源类型: {}", other);
                    continue;
                }
            };

            // 去重写入，并累计 sync_log
            let source_result = self.dedup_and_write(&source.id, &sessions);
            log::info!(
                "数据源 {} 采集完成: 发现={}, 新增={}, 更新={}, 跳过={}",
                source.name, source_result.found, source_result.new,
                source_result.updated, source_result.skipped
            );

            // 记录 sync_log
            if let Err(e) = self.write_sync_log(&source.id, &source_result) {
                log::error!("写入同步日志失败: {}", e);
            }

            total_result.found += source_result.found;
            total_result.new += source_result.new;
            total_result.updated += source_result.updated;
            total_result.skipped += source_result.skipped;
        }

        log::info!(
            "全量采集完成: 发现={}, 新增={}, 更新={}, 跳过={}",
            total_result.found, total_result.new,
            total_result.updated, total_result.skipped
        );

        total_result
    }

    /// 对采集到的会话逐条去重写入。
    ///
    /// 去重策略：
    /// - session_id + source_host 不存在 → INSERT（新增）
    /// - 存在 + message_count 增加    → 标记 has_updates（更新）
    /// - 存在 + message_count 不变    → SKIP（跳过）
    fn dedup_and_write(&self, _source_id: &str, sessions: &[NormalizedSession]) -> SyncResult {
        let mut result = SyncResult {
            found: sessions.len() as u32,
            ..Default::default()
        };

        for session in sessions {
            match self.write_one_session(session) {
                Ok(WriteAction::New) => result.new += 1,
                Ok(WriteAction::Updated) => result.updated += 1,
                Ok(WriteAction::Skipped) => result.skipped += 1,
                Err(e) => {
                    log::error!(
                        "写入会话失败: session_id={}, error={}",
                        session.session_id, e
                    );
                    result.skipped += 1;
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

        // Step 1: 检查是否已存在
        let existing_id = self.db.check_duplicate(&session.session_id, source_host)?;

        // 将 NormalizedMessage 转换为存储层的 NewMessage
        let to_new_messages = |session: &NormalizedSession| -> Vec<NewMessage> {
            session.messages.iter().map(|m| NewMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                timestamp: m.timestamp.clone(),
                tokens_in: m.tokens_in as i32,
                tokens_out: m.tokens_out as i32,
            }).collect()
        };

        match existing_id {
            None => {
                // 新会话 → 写入 session + messages
                let db_id = self.db.insert_session(
                    &session.source_id,
                    &session.session_id,
                    source_host,
                    session.project_path.as_deref(),
                    session.project_name.as_deref(),
                    message_count,
                    None, // content_hash（v0.1 暂不计算）
                    &session.raw_path,
                    &session.created_at,
                    &session.updated_at,
                    session.analysis_title.as_deref(),
                )?;

                self.db.insert_messages(&db_id, &to_new_messages(session))?;
                Ok(WriteAction::New)
            }
            Some(existing_db_id) => {
                // 已存在 → 检查 message_count 是否变化
                let existing_session = self.db.get_session(&existing_db_id)?;

                if message_count as i64 > existing_session.message_count {
                    // 有新消息 → 删除旧消息并重新全量导入，保证数据一致性
                    self.db.delete_session_messages(&existing_db_id)?;
                    self.db.insert_messages(&existing_db_id, &to_new_messages(session))?;
                    self.db.mark_has_updates(&existing_db_id)?;
                    self.db.update_session_resync_metadata(
                        &existing_db_id,
                        message_count,
                        session.project_path.as_deref(),
                        session.project_name.as_deref(),
                        session.analysis_title.as_deref(),
                    )?;
                    log::info!(
                        "会话消息已更新: session_id={}, {} → {} 条消息",
                        session.session_id,
                        existing_session.message_count,
                        message_count
                    );
                    Ok(WriteAction::Updated)
                } else {
                    Ok(WriteAction::Skipped)
                }
            }
        }
    }

    /// 写入同步日志
    fn write_sync_log(
        &self,
        source_id: &str,
        result: &SyncResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO sync_log (
                id, source_id, started_at, finished_at,
                sessions_found, sessions_new, sessions_updated, status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id, source_id, now, now,
                result.found as i64, result.new as i64, result.updated as i64,
                "completed"
            ],
        )?;
        Ok(())
    }
}

/// 去重写入的动作结果
enum WriteAction {
    /// 新会话，已写入
    New,
    /// 已存在但有新消息，已标记更新
    Updated,
    /// 已存在且无变化，跳过
    Skipped,
}
