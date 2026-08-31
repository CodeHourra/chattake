//! Cursor SQLite 数据源解析器。
//!
//! Cursor 将对话数据分散存储在多个 SQLite 文件（.vscdb）中：
//!
//! ```text
//! ~/Library/Application Support/Cursor/
//! ├── User/globalStorage/state.vscdb          ← 全局数据库（cursorDiskKV 表）
//! │   ├── composerData:{composerId}           ← 会话元数据 + bubble 引用列表
//! │   └── bubbleId:{composerId}:{bubbleId}    ← 单条消息数据
//! └── User/workspaceStorage/{hash}/
//!     ├── workspace.json                       ← { folder: "file:///project/path" }
//!     └── state.vscdb                          ← composer.composerData → allComposers 列表
//! ```
//!
//! bubble 类型：type=1 为 user，type=2 为 assistant

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::path_local::decode_cursor_folder_to_local_path;

use super::normalizer::{
    file_fingerprint, normalize_user_content, CollectionBatch, CollectionFailure,
    NormalizedMessage, NormalizedSession,
};

/// Cursor 数据采集器
pub struct CursorCollector {
    /// 扫描目录列表（如 ~/Library/Application Support/Cursor）
    scan_dirs: Vec<PathBuf>,
}

impl CursorCollector {
    pub fn new(scan_dirs: Vec<PathBuf>) -> Self {
        Self { scan_dirs }
    }

    /// 扫描所有配置的目录，返回解析后的标准化会话列表
    #[cfg(test)]
    pub fn collect(&self) -> Vec<NormalizedSession> {
        self.collect_changed(|_, _, _| false).sessions
    }

    pub fn collect_changed<F>(&self, unchanged: F) -> CollectionBatch
    where
        F: Fn(&Path, i64, i64) -> bool,
    {
        let mut batch = CollectionBatch::default();

        for base_dir in &self.scan_dirs {
            let global_db_path = base_dir
                .join("User")
                .join("globalStorage")
                .join("state.vscdb");
            let ws_base = base_dir.join("User").join("workspaceStorage");

            if !global_db_path.exists() {
                log::debug!(
                    "Cursor 全局数据库不存在，跳过: {}",
                    global_db_path.display()
                );
                continue;
            }
            batch.found += 1;
            let (raw_mtime_ms, raw_size_bytes) = match sqlite_fingerprint(&global_db_path) {
                Some(fingerprint) => fingerprint,
                None => continue,
            };
            if unchanged(&global_db_path, raw_mtime_ms, raw_size_bytes) {
                batch.skipped += 1;
                batch
                    .skipped_paths
                    .push(global_db_path.to_string_lossy().into_owned());
                continue;
            }

            match self.collect_from_dir(&global_db_path, &ws_base, raw_mtime_ms, raw_size_bytes) {
                Ok(mut found) => {
                    log::info!(
                        "从 {} 扫描到 {} 个 Cursor 会话",
                        base_dir.display(),
                        found.sessions.len()
                    );
                    batch.sessions.append(&mut found.sessions);
                    batch.failures.append(&mut found.failures);
                    batch.skipped_paths.append(&mut found.skipped_paths);
                }
                Err(e) => {
                    batch.failures.push(CollectionFailure {
                        raw_path: global_db_path.to_string_lossy().into_owned(),
                        error: e.to_string(),
                    });
                }
            }
        }

        log::info!("Cursor 采集完成，共 {} 个会话", batch.sessions.len());
        batch
    }

    /// 从单个 Cursor 安装目录采集会话
    fn collect_from_dir(
        &self,
        global_db_path: &Path,
        ws_base: &Path,
        raw_mtime_ms: i64,
        raw_size_bytes: i64,
    ) -> Result<CollectionBatch, Box<dyn std::error::Error>> {
        // Step 1: 扫描所有 workspace，建立 composerId → (project_path, name) 映射
        let workspace_map = scan_workspaces(ws_base)?;
        log::debug!("发现 {} 个 workspace", workspace_map.len());

        // Step 2: 打开全局数据库（只读），读取所有 composer 的 bubble 数据
        let conn = Connection::open_with_flags(
            global_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        let mut batch = CollectionBatch::default();

        // 遍历所有 workspace 中的 composer
        for (composer_id, ws_info) in &workspace_map {
            match read_composer_session(
                &conn,
                composer_id,
                ws_info,
                global_db_path,
                raw_mtime_ms,
                raw_size_bytes,
            ) {
                Ok(Some(session)) => {
                    batch.sessions.push(session);
                }
                Ok(None) => {
                    // 会话无有效消息，跳过
                }
                Err(e) => {
                    batch.failures.push(CollectionFailure {
                        raw_path: format!("{}#{}", global_db_path.display(), composer_id),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(batch)
    }
}

fn sqlite_fingerprint(db_path: &Path) -> Option<(i64, i64)> {
    let mut fingerprint = file_fingerprint(db_path)?;
    let wal_path = PathBuf::from(format!("{}-wal", db_path.to_string_lossy()));
    if let Some((mtime, size)) = file_fingerprint(&wal_path) {
        fingerprint.0 = fingerprint.0.max(mtime);
        fingerprint.1 = fingerprint.1.saturating_add(size);
    }
    Some(fingerprint)
}

// ── Workspace 扫描 ──

/// Workspace 信息（从 workspaceStorage 目录解析）
struct WorkspaceInfo {
    /// 项目路径
    project_path: Option<String>,
    /// 项目名称（从路径推导）
    project_name: Option<String>,
    /// Composer 显示名称（v0.2 用于列表展示，暂未读取）
    #[allow(dead_code)]
    composer_name: Option<String>,
    /// Composer 创建时间（毫秒时间戳）
    created_at: Option<i64>,
}

/// 扫描 workspaceStorage 目录，返回 composerId → WorkspaceInfo 映射
fn scan_workspaces(
    ws_base: &Path,
) -> Result<HashMap<String, WorkspaceInfo>, Box<dyn std::error::Error>> {
    let mut map = HashMap::new();

    if !ws_base.exists() {
        return Ok(map);
    }

    for entry in std::fs::read_dir(ws_base)? {
        let entry = entry?;
        let ws_dir = entry.path();
        if !ws_dir.is_dir() {
            continue;
        }

        // 读取 workspace.json 获取项目路径
        let workspace_json = ws_dir.join("workspace.json");
        let project_path = read_workspace_project_path(&workspace_json);
        let project_name = project_path
            .as_ref()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(String::from);

        // 从 state.vscdb 读取该 workspace 关联的 composer 列表
        let state_db = ws_dir.join("state.vscdb");
        if !state_db.exists() {
            continue;
        }

        match read_workspace_composers(&state_db) {
            Ok(composers) => {
                for (id, name, created_at) in composers {
                    map.insert(
                        id,
                        WorkspaceInfo {
                            project_path: project_path.clone(),
                            project_name: project_name.clone(),
                            composer_name: name,
                            created_at,
                        },
                    );
                }
            }
            Err(e) => {
                log::trace!(
                    "读取 workspace composers 失败: {} - {}",
                    state_db.display(),
                    e
                );
            }
        }
    }

    Ok(map)
}

/// 从 workspace.json 读取项目路径（本地绝对路径字符串）
///
/// 解码逻辑见 [`crate::path_local::decode_cursor_folder_to_local_path`]。
fn read_workspace_project_path(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&content).ok()?;
    let folder = data["folder"].as_str()?;
    let out = decode_cursor_folder_to_local_path(folder);
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// 从 workspace 的 state.vscdb 读取 allComposers 列表
/// 返回 Vec<(composerId, name, createdAt)>
fn read_workspace_composers(
    db_path: &Path,
) -> Result<Vec<(String, Option<String>, Option<i64>)>, Box<dyn std::error::Error>> {
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let result: Result<String, _> = conn.query_row(
        "SELECT value FROM ItemTable WHERE key = 'composer.composerData'",
        [],
        |row| row.get(0),
    );

    let text = match result {
        Ok(t) => t,
        Err(_) => return Ok(vec![]),
    };

    let data: serde_json::Value = serde_json::from_str(&text)?;
    let all_composers = match data["allComposers"].as_array() {
        Some(arr) => arr,
        None => return Ok(vec![]),
    };

    let mut composers = Vec::new();
    for c in all_composers {
        if let Some(id) = c["composerId"].as_str() {
            let name = c["name"].as_str().map(String::from);
            let created_at = c["createdAt"].as_i64();
            composers.push((id.to_string(), name, created_at));
        }
    }

    Ok(composers)
}

// ── Composer 会话读取 ──

/// 从全局数据库读取单个 composer 的完整对话
fn read_composer_session(
    conn: &Connection,
    composer_id: &str,
    ws_info: &WorkspaceInfo,
    raw_path: &Path,
    raw_mtime_ms: i64,
    raw_size_bytes: i64,
) -> Result<Option<NormalizedSession>, Box<dyn std::error::Error>> {
    // 读取 composerData 获取 bubble 列表
    let key = format!("composerData:{}", composer_id);
    let result: Result<String, _> = conn.query_row(
        "SELECT value FROM cursorDiskKV WHERE key = ?1",
        [&key],
        |row| row.get(0),
    );

    let text = match result {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let data: serde_json::Value = serde_json::from_str(&text)?;
    let headers = match data["fullConversationHeadersOnly"].as_array() {
        Some(arr) => arr,
        None => return Ok(None),
    };

    if headers.is_empty() {
        return Ok(None);
    }

    // 从 composerData 中获取 createdAt（毫秒时间戳），作为备选
    let composer_created_at = data["createdAt"].as_i64().or(ws_info.created_at);

    // 逐个读取 bubble 内容
    let mut messages = Vec::new();
    for header in headers {
        let bubble_id = match header["bubbleId"].as_str() {
            Some(id) => id,
            None => continue,
        };
        let bubble_type = header["type"].as_i64().unwrap_or(0);

        let role = match bubble_type {
            1 => "user",
            2 => "assistant",
            _ => continue,
        };

        // 读取 bubble 完整数据
        let bubble_key = format!("bubbleId:{}:{}", composer_id, bubble_id);
        let bubble_result: Result<String, _> = conn.query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1",
            [&bubble_key],
            |row| row.get(0),
        );

        let bubble_text = match bubble_result {
            Ok(t) => t,
            Err(_) => continue,
        };

        let bubble_data: serde_json::Value = match serde_json::from_str(&bubble_text) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // 提取文本内容：优先 text 字段，备选 richText（Lexical 格式）
        let content = extract_bubble_text(&bubble_data);
        if content.trim().is_empty() {
            continue;
        }
        let content = if role == "user" {
            match normalize_user_content(&content) {
                Some(content) => content,
                None => continue,
            }
        } else {
            content
        };

        let token_count = &bubble_data["tokenCount"];
        let tokens_in = token_count["inputTokens"].as_u64().unwrap_or(0) as u32;
        let tokens_out = token_count["outputTokens"].as_u64().unwrap_or(0) as u32;

        let timestamp = bubble_data["createdAt"]
            .as_i64()
            .map(|ms| millis_to_rfc3339(ms));

        messages.push(NormalizedMessage {
            role: role.to_string(),
            content,
            timestamp,
            tokens_in,
            tokens_out,
        });
    }

    if messages.is_empty() {
        return Ok(None);
    }

    // 时间戳处理
    let created_at = messages
        .first()
        .and_then(|m| m.timestamp.clone())
        .or_else(|| composer_created_at.map(|ms| millis_to_rfc3339(ms)))
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let updated_at = messages
        .last()
        .and_then(|m| m.timestamp.clone())
        .unwrap_or_else(|| created_at.clone());

    Ok(Some(NormalizedSession {
        source_id: "cursor".to_string(),
        session_id: composer_id.to_string(),
        project_path: ws_info.project_path.clone(),
        project_name: ws_info.project_name.clone(),
        analysis_title: None,
        messages,
        raw_path: raw_path.to_string_lossy().to_string(),
        raw_mtime_ms: Some(raw_mtime_ms),
        raw_size_bytes: Some(raw_size_bytes),
        created_at,
        updated_at,
    }))
}

// ── 文本提取 ──

/// 从 bubble 数据中提取文本内容
fn extract_bubble_text(bubble: &serde_json::Value) -> String {
    // 优先使用 text 字段
    if let Some(text) = bubble["text"].as_str() {
        if !text.trim().is_empty() {
            return text.to_string();
        }
    }

    // 备选：从 richText（Lexical 格式）提取
    if let Some(rich_text) = bubble["richText"].as_str() {
        if !rich_text.is_empty() {
            if let Ok(lexical) = serde_json::from_str::<serde_json::Value>(rich_text) {
                let extracted = extract_lexical_text(&lexical);
                if !extracted.trim().is_empty() {
                    return extracted;
                }
            }
        }
    }

    // richText 也可能是对象而非字符串
    if bubble["richText"].is_object() {
        let extracted = extract_lexical_text(&bubble["richText"]);
        if !extracted.trim().is_empty() {
            return extracted;
        }
    }

    String::new()
}

/// 递归遍历 Lexical AST 节点，提取纯文本内容。
///
/// Lexical 格式示例：
/// ```json
/// { "root": { "children": [
///   { "type": "paragraph", "children": [
///     { "type": "text", "text": "Hello world" }
///   ]}
/// ]}}
/// ```
fn extract_lexical_text(node: &serde_json::Value) -> String {
    match node["type"].as_str() {
        Some("text") => node["text"].as_str().unwrap_or("").to_string(),
        Some("linebreak") => "\n".to_string(),
        Some("code") => {
            let code = node["code"].as_str().unwrap_or("");
            format!("```\n{}\n```", code)
        }
        _ => {
            // 对 root / paragraph / 其他容器节点，递归处理 children
            if let Some(children) = node["children"].as_array() {
                children
                    .iter()
                    .map(extract_lexical_text)
                    .collect::<Vec<_>>()
                    .join("")
            } else if node["root"].is_object() {
                // 顶层 { root: { children: [...] } } 格式
                extract_lexical_text(&node["root"])
            } else {
                String::new()
            }
        }
    }
}

// ── 工具函数 ──

/// 毫秒时间戳转 RFC 3339 字符串
fn millis_to_rfc3339(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    let secs = ms / 1000;
    let nsecs = ((ms % 1000) * 1_000_000) as u32;
    match Utc.timestamp_opt(secs, nsecs) {
        chrono::LocalResult::Single(dt) => dt.to_rfc3339(),
        _ => chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_lexical_text_simple() {
        let lexical = serde_json::json!({
            "root": {
                "children": [
                    {
                        "type": "paragraph",
                        "children": [
                            { "type": "text", "text": "Hello " },
                            { "type": "text", "text": "world" }
                        ]
                    }
                ]
            }
        });
        assert_eq!(extract_lexical_text(&lexical), "Hello world");
    }

    #[test]
    fn test_extract_lexical_text_with_linebreak() {
        let lexical = serde_json::json!({
            "root": {
                "children": [
                    {
                        "type": "paragraph",
                        "children": [
                            { "type": "text", "text": "Line 1" },
                            { "type": "linebreak" },
                            { "type": "text", "text": "Line 2" }
                        ]
                    }
                ]
            }
        });
        assert_eq!(extract_lexical_text(&lexical), "Line 1\nLine 2");
    }

    #[test]
    fn test_extract_lexical_text_with_code() {
        let lexical = serde_json::json!({
            "type": "code",
            "code": "fn main() {}"
        });
        assert_eq!(extract_lexical_text(&lexical), "```\nfn main() {}\n```");
    }

    #[test]
    fn test_extract_lexical_text_empty() {
        let lexical = serde_json::json!({});
        assert_eq!(extract_lexical_text(&lexical), "");
    }

    #[test]
    fn test_extract_bubble_text_prefers_plain_text() {
        let bubble = serde_json::json!({
            "text": "Plain text content",
            "richText": "{\"root\":{\"children\":[{\"type\":\"paragraph\",\"children\":[{\"type\":\"text\",\"text\":\"Rich text\"}]}]}}"
        });
        assert_eq!(extract_bubble_text(&bubble), "Plain text content");
    }

    #[test]
    fn test_extract_bubble_text_falls_back_to_rich_text() {
        let bubble = serde_json::json!({
            "text": "",
            "richText": "{\"root\":{\"children\":[{\"type\":\"paragraph\",\"children\":[{\"type\":\"text\",\"text\":\"From rich text\"}]}]}}"
        });
        assert_eq!(extract_bubble_text(&bubble), "From rich text");
    }

    #[test]
    fn test_extract_bubble_text_rich_text_as_object() {
        let bubble = serde_json::json!({
            "text": "",
            "richText": {
                "root": {
                    "children": [
                        {
                            "type": "paragraph",
                            "children": [
                                { "type": "text", "text": "Object rich text" }
                            ]
                        }
                    ]
                }
            }
        });
        assert_eq!(extract_bubble_text(&bubble), "Object rich text");
    }

    #[test]
    fn test_millis_to_rfc3339() {
        let result = millis_to_rfc3339(1763367049142);
        assert!(result.contains("2025") || result.contains("2026"));
        assert!(result.contains("T"));
    }

    #[test]
    fn test_read_workspace_project_path() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("workspace.json");
        std::fs::write(&json_path, r#"{"folder":"file:///Users/steve/myproject"}"#).unwrap();

        let result = read_workspace_project_path(&json_path);
        assert_eq!(result, Some("/Users/steve/myproject".to_string()));
    }

    #[test]
    fn test_read_workspace_project_path_percent_encoded() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("workspace.json");
        // 路径末段经百分号编码（UTF-8）：向善数据
        std::fs::write(
            &json_path,
            r#"{"folder":"file:///Users/steve/%E5%90%91%E5%96%84%E6%95%B0%E6%8D%AE"}"#,
        )
        .unwrap();

        let result = read_workspace_project_path(&json_path);
        assert_eq!(result, Some("/Users/steve/向善数据".to_string()));
    }

    /// 使用本地真实 Cursor 数据验证（需要 --ignored 运行）
    #[test]
    #[ignore]
    fn test_real_local_data() {
        let home = dirs::home_dir().unwrap();
        let collector = CursorCollector::new(vec![home.join("Library/Application Support/Cursor")]);

        let sessions = collector.collect();
        println!("\n=== Cursor 采集结果 ===");
        println!("共发现 {} 个会话\n", sessions.len());
        assert!(!sessions.is_empty(), "本地应有 Cursor 对话数据");

        for s in sessions.iter().take(10) {
            println!("Session: {}", s.session_id);
            println!("  Project: {:?}", s.project_name);
            println!("  Path: {:?}", s.project_path);
            println!("  Messages: {}", s.messages.len());
            println!("  Created: {}", s.created_at);
            if let Some(first) = s.messages.first() {
                let preview: String = first.content.chars().take(80).collect();
                println!("  First msg: [{}] {}", first.role, preview);
            }
            println!();
        }
    }
}
