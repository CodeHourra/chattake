//! Claude Code JSONL 数据源解析器。
//!
//! Claude Code 将对话存储为 JSONL 文件，目录结构：
//! ```text
//! ~/.claude/projects/
//! └── {project-path-hash}/           # 目录名是项目路径 "-" 连接
//!     ├── {session-uuid}.jsonl       # 每个会话一个 JSONL 文件
//!     └── {session-uuid}/            # 子目录（文件快照，忽略）
//! ```
//!
//! JSONL 每行一个 JSON 对象，关键字段：
//! - type: "user" | "assistant" | "system" | "progress" | "file-history-snapshot" | ...
//! - isSidechain: bool — 子代理循环内的消息，采集时跳过
//! - message.content: string | Array<{type: "text", text: "..."} | {type: "tool_use", ...}>
//! - message.usage: { input_tokens, output_tokens, ... }
//! - timestamp: RFC 3339
//! - cwd: 项目工作目录（可用于推导 project_path）
//! - sessionId: 会话 UUID

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::normalizer::{NormalizedMessage, NormalizedSession};

/// Claude Code 数据采集器
pub struct ClaudeCodeCollector {
    /// 扫描目录列表（已展开 ~ 为绝对路径）
    scan_dirs: Vec<PathBuf>,
}

impl ClaudeCodeCollector {
    pub fn new(scan_dirs: Vec<PathBuf>) -> Self {
        Self { scan_dirs }
    }

    /// 扫描所有配置的目录，返回解析后的标准化会话列表
    pub fn collect(&self) -> Vec<NormalizedSession> {
        let mut sessions = Vec::new();

        for dir in &self.scan_dirs {
            let projects_dir = dir.join("projects");
            if !projects_dir.exists() {
                log::debug!("Claude Code projects 目录不存在，跳过: {}", projects_dir.display());
                continue;
            }

            match self.scan_projects_dir(&projects_dir) {
                Ok(mut found) => {
                    log::info!(
                        "从 {} 扫描到 {} 个会话",
                        projects_dir.display(),
                        found.len()
                    );
                    sessions.append(&mut found);
                }
                Err(e) => {
                    log::error!("扫描 Claude Code 目录失败: {} - {}", projects_dir.display(), e);
                }
            }
        }

        log::info!("Claude Code 采集完成，共 {} 个会话", sessions.len());
        sessions
    }

    /// 扫描 projects/ 下所有子目录中的 .jsonl 文件
    fn scan_projects_dir(&self, projects_dir: &Path) -> Result<Vec<NormalizedSession>, std::io::Error> {
        let mut sessions = Vec::new();

        for entry in fs::read_dir(projects_dir)? {
            let entry = entry?;
            let project_dir = entry.path();
            if !project_dir.is_dir() {
                continue;
            }

            // 目录名形如 "-Users-steve-Codes-myspace-ai-project-xunji"
            // 可以反推项目路径：将 - 替换为 /（首个 - 是分隔符）
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let (project_path, project_name) = derive_project_info(&dir_name);

            for file_entry in fs::read_dir(&project_dir)? {
                let file_entry = file_entry?;
                let file_path = file_entry.path();

                // 只处理 .jsonl 文件
                if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }

                // 文件名（去掉 .jsonl）即为 session_id
                let session_id = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();

                if session_id.is_empty() {
                    continue;
                }

                match parse_jsonl(&file_path) {
                    Ok(parse_result) => {
                        if parse_result.messages.is_empty() {
                            log::debug!("会话无有效消息，跳过: {}", file_path.display());
                            continue;
                        }

                        // 优先使用 JSONL 内的 cwd 作为项目路径
                        let final_project_path = parse_result
                            .cwd
                            .clone()
                            .or_else(|| project_path.clone());
                        let final_project_name = final_project_path
                            .as_ref()
                            .and_then(|p| Path::new(p).file_name())
                            .and_then(|n| n.to_str())
                            .map(String::from)
                            .or_else(|| project_name.clone());

                        let created_at = parse_result
                            .first_timestamp
                            .clone()
                            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                        let updated_at = parse_result
                            .last_timestamp
                            .clone()
                            .unwrap_or_else(|| created_at.clone());

                        sessions.push(NormalizedSession {
                            source_id: "claude-code".to_string(),
                            session_id,
                            project_path: final_project_path,
                            project_name: final_project_name,
                            analysis_title: None,
                            messages: parse_result.messages,
                            raw_path: file_path.to_string_lossy().to_string(),
                            created_at,
                            updated_at,
                        });
                    }
                    Err(e) => {
                        log::warn!("解析 JSONL 文件失败: {} - {}", file_path.display(), e);
                    }
                }
            }
        }

        Ok(sessions)
    }
}

// ── JSONL 解析 ──

/// JSONL 文件解析的中间结果
struct ParseResult {
    messages: Vec<NormalizedMessage>,
    /// JSONL 中首次出现的 cwd 字段（项目工作目录）
    cwd: Option<String>,
    /// 第一条消息的时间戳
    first_timestamp: Option<String>,
    /// 最后一条消息的时间戳
    last_timestamp: Option<String>,
}

/// 解析单个 JSONL 文件，提取 user/assistant 消息
fn parse_jsonl(path: &Path) -> Result<ParseResult, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut cwd: Option<String> = None;
    let mut first_timestamp: Option<String> = None;
    let mut last_timestamp: Option<String> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                log::trace!("跳过无法解析的行: {}", e);
                continue;
            }
        };

        // 跳过子代理（sidechain）内的消息
        if entry["isSidechain"].as_bool().unwrap_or(false) {
            continue;
        }

        // 提取 cwd（取第一个出现的值）
        if cwd.is_none() {
            if let Some(c) = entry["cwd"].as_str() {
                cwd = Some(c.to_string());
            }
        }

        let msg_type = entry["type"].as_str().unwrap_or("");
        if msg_type != "user" && msg_type != "assistant" {
            continue;
        }

        let message = &entry["message"];
        let extracted = extract_content(message);
        if extracted.text.trim().is_empty() {
            continue;
        }

        // 当 API 层 role 为 "user" 但内容全是 tool_result 时，标记为 "tool"
        let role = if extracted.is_tool_result {
            "tool".to_string()
        } else {
            message["role"]
                .as_str()
                .unwrap_or(msg_type)
                .to_string()
        };

        let timestamp = entry["timestamp"].as_str().map(String::from);

        // 记录首尾时间戳
        if let Some(ref ts) = timestamp {
            if first_timestamp.is_none() {
                first_timestamp = Some(ts.clone());
            }
            last_timestamp = Some(ts.clone());
        }

        // usage 在 message.usage 中
        let usage = &message["usage"];
        let tokens_in = usage["input_tokens"].as_u64().unwrap_or(0) as u32;
        let tokens_out = usage["output_tokens"].as_u64().unwrap_or(0) as u32;

        messages.push(NormalizedMessage {
            role,
            content: extracted.text,
            timestamp,
            tokens_in,
            tokens_out,
        });
    }

    Ok(ParseResult {
        messages,
        cwd,
        first_timestamp,
        last_timestamp,
    })
}

/// extract_content 的返回值
struct ExtractedContent {
    /// 提取的文本内容
    text: String,
    /// 该消息内容是否全部由 tool_result 组成（无真正的用户输入）
    is_tool_result: bool,
}

/// 从 message 对象中提取文本内容。
///
/// Claude Code 的 content 有两种格式：
/// - 纯字符串（user 消息常见）
/// - 数组（assistant 消息常见）：[{type: "text", text: "..."}, {type: "tool_use", name: "...", input: {...}}, ...]
///
/// 当 content 为数组且仅包含 tool_result 类型时，标记 is_tool_result = true，
/// 调用方据此将 role 从 "user" 改为 "tool"。
fn extract_content(message: &serde_json::Value) -> ExtractedContent {
    let content = &message["content"];

    match content {
        serde_json::Value::String(s) => ExtractedContent {
            text: s.clone(),
            is_tool_result: false,
        },
        serde_json::Value::Array(arr) => {
            let mut parts = Vec::new();
            let mut has_text = false;
            let mut has_tool_result = false;

            for item in arr {
                match item["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = item["text"].as_str() {
                            if !text.trim().is_empty() {
                                has_text = true;
                                parts.push(text.to_string());
                            }
                        }
                    }
                    Some("tool_use") => {
                        // 保留工具调用的摘要信息，供 LLM 分析时理解上下文
                        let name = item["name"].as_str().unwrap_or("unknown");
                        parts.push(format!("[Tool: {}]", name));
                    }
                    Some("tool_result") => {
                        has_tool_result = true;
                        // tool_result 可能包含嵌套 content
                        if let Some(nested) = item["content"].as_str() {
                            let preview = truncate_str(nested, 200);
                            parts.push(format!("[Tool Result: {}]", preview));
                        } else if let Some(nested_arr) = item["content"].as_array() {
                            for nested_item in nested_arr {
                                if nested_item["type"].as_str() == Some("text") {
                                    if let Some(text) = nested_item["text"].as_str() {
                                        let preview = truncate_str(text, 200);
                                        parts.push(format!("[Tool Result: {}]", preview));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // 没有 text 块且有 tool_result → 整条消息是工具返回值
            let is_tool_result = !has_text && has_tool_result;

            ExtractedContent {
                text: parts.join("\n\n"),
                is_tool_result,
            }
        }
        _ => ExtractedContent {
            text: String::new(),
            is_tool_result: false,
        },
    }
}

/// 截断字符串到指定字符数，返回不超过 max_chars 的前缀切片
fn truncate_str(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    // 找到不超过 max_chars 的字符边界
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// 从 Claude Code 的 projects 目录名反推项目路径和名称。
///
/// 目录名格式: "-Users-steve-Codes-myspace-ai-project-xunji"
/// 转换规则: 将 "-" 替换为 "/"，得到 "/Users/steve/Codes/myspace/ai-project/xunji"
///
/// 注意：此推导不完全准确（路径中的 "-" 会被混淆），所以优先使用 JSONL 内的 cwd 字段。
fn derive_project_info(dir_name: &str) -> (Option<String>, Option<String>) {
    if dir_name.is_empty() {
        return (None, None);
    }

    // 目录名以 "-" 开头，将 "-" 替换为 "/"
    let path = dir_name.replace('-', "/");

    // 尝试提取项目名称（路径最后一段）
    let name = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);

    (Some(path), name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_content_string() {
        let msg = serde_json::json!({
            "content": "Hello, world!"
        });
        let result = extract_content(&msg);
        assert_eq!(result.text, "Hello, world!");
        assert!(!result.is_tool_result);
    }

    #[test]
    fn test_extract_content_array_with_text() {
        let msg = serde_json::json!({
            "content": [
                { "type": "text", "text": "First paragraph" },
                { "type": "text", "text": "Second paragraph" }
            ]
        });
        let result = extract_content(&msg);
        assert_eq!(result.text, "First paragraph\n\nSecond paragraph");
        assert!(!result.is_tool_result);
    }

    #[test]
    fn test_extract_content_array_with_tool_use() {
        let msg = serde_json::json!({
            "content": [
                { "type": "text", "text": "Let me read the file." },
                { "type": "tool_use", "name": "Read", "input": { "path": "/foo/bar.rs" } }
            ]
        });
        let result = extract_content(&msg);
        assert!(result.text.contains("Let me read the file."));
        assert!(result.text.contains("[Tool: Read]"));
        assert!(!result.is_tool_result, "有 text 块时不应标记为 tool_result");
    }

    #[test]
    fn test_extract_content_pure_tool_result() {
        let msg = serde_json::json!({
            "content": [
                { "type": "tool_result", "content": "AGENTS.md\nblueprints\n创作进度\n正文" }
            ]
        });
        let result = extract_content(&msg);
        assert!(result.text.contains("[Tool Result:"));
        assert!(result.is_tool_result, "纯 tool_result 内容应标记为 is_tool_result");
    }

    #[test]
    fn test_extract_content_empty() {
        let msg = serde_json::json!({});
        let result = extract_content(&msg);
        assert_eq!(result.text, "");
        assert!(!result.is_tool_result);
    }

    #[test]
    fn test_derive_project_info() {
        let (path, name) = derive_project_info("-Users-steve-Codes-myspace-ai-project-xunji");
        assert_eq!(path, Some("/Users/steve/Codes/myspace/ai/project/xunji".to_string()));
        assert_eq!(name, Some("xunji".to_string()));
    }

    #[test]
    fn test_derive_project_info_empty() {
        let (path, name) = derive_project_info("");
        assert!(path.is_none());
        assert!(name.is_none());
    }

    #[test]
    fn test_parse_jsonl_basic() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-session.jsonl");
        let mut file = File::create(&file_path).unwrap();

        // 写入测试数据
        writeln!(file, r#"{{"type":"progress","isSidechain":false,"timestamp":"2026-03-20T05:27:27.885Z","cwd":"/Users/steve/project","sessionId":"test-123"}}"#).unwrap();
        writeln!(file, r#"{{"type":"user","isSidechain":false,"message":{{"role":"user","content":"Hello"}},"timestamp":"2026-03-20T05:30:00.000Z","sessionId":"test-123"}}"#).unwrap();
        writeln!(file, r#"{{"type":"assistant","isSidechain":false,"message":{{"role":"assistant","content":[{{"type":"text","text":"Hi there!"}}],"usage":{{"input_tokens":100,"output_tokens":50}}}},"timestamp":"2026-03-20T05:30:05.000Z","sessionId":"test-123"}}"#).unwrap();
        // sidechain 消息应被跳过
        writeln!(file, r#"{{"type":"assistant","isSidechain":true,"message":{{"role":"assistant","content":"should be skipped"}},"timestamp":"2026-03-20T05:30:10.000Z","sessionId":"test-123"}}"#).unwrap();

        let result = parse_jsonl(&file_path).unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[0].content, "Hello");
        assert_eq!(result.messages[1].role, "assistant");
        assert_eq!(result.messages[1].content, "Hi there!");
        assert_eq!(result.messages[1].tokens_in, 100);
        assert_eq!(result.messages[1].tokens_out, 50);
        assert_eq!(result.cwd, Some("/Users/steve/project".to_string()));
        assert_eq!(result.first_timestamp, Some("2026-03-20T05:30:00.000Z".to_string()));
        assert_eq!(result.last_timestamp, Some("2026-03-20T05:30:05.000Z".to_string()));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("你好世界", 2), "你好");
    }

    /// 使用本地真实 Claude Code 数据验证采集器（需要 --ignored 才会运行）
    #[test]
    #[ignore]
    fn test_real_local_data() {
        let home = dirs::home_dir().unwrap();
        let collector = ClaudeCodeCollector::new(vec![home.join(".claude")]);

        let sessions = collector.collect();
        println!("\n=== Claude Code 采集结果 ===");
        println!("共发现 {} 个会话\n", sessions.len());
        assert!(!sessions.is_empty(), "本地应有 Claude Code 对话数据");

        for s in &sessions {
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
