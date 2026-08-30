//! CodeBuddy 产品会话采集（与问渠 `scanner/codebuddy.py` 路径一致）。
//!
//! ```text
//! 扫描根目录（默认）：
//!   macOS  ~/Library/Application Support/CodeBuddyExtension/Data
//!   Linux  ~/.local/share/CodeBuddyExtension
//!
//! 在根目录下递归查找「会话叶目录」：
//!   含 index.json（且存在顶层 messages 数组）+ messages/*.json
//!   （工作区目录仅有 conversations 列表、无 messages，会自动排除）
//!
//! 单条消息：messages/{id}.json 内 message 为 JSON 字符串，解析后取 content（字符串 / text 块数组 / 旧版单字符数组，见 `docs/CodeBuddy 会话记录提取逻辑文档.md`）。
//! ```
//!
//! source_id 为 `codebuddy`。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use super::normalizer::{file_fingerprint, NormalizedMessage, NormalizedSession};

const SOURCE_ID: &str = "codebuddy";

/// CodeBuddy（扩展）采集器
pub struct CodeBuddyCollector {
    /// 来自配置的扫描根目录（已展开 ~），与问渠 LOCALHOST_CANDIDATES 一致
    scan_dirs: Vec<PathBuf>,
}

impl CodeBuddyCollector {
    pub fn new(scan_dirs: Vec<PathBuf>) -> Self {
        Self { scan_dirs }
    }

    /// 扫描所有配置根目录下的会话目录，产出标准化会话列表
    pub fn collect_changed<F>(&self, unchanged: F) -> Vec<NormalizedSession>
    where
        F: Fn(&Path, i64, i64) -> bool,
    {
        log::info!(
            "CodeBuddy 采集开始，解析后的扫描根目录共 {} 个: {:?}",
            self.scan_dirs.len(),
            self.scan_dirs
        );
        let mut sessions = Vec::new();
        let mut seen = HashSet::<String>::new();

        for root in &self.scan_dirs {
            if !root.is_dir() {
                log::info!("CodeBuddy：扫描根目录不存在，跳过: {}", root.display());
                continue;
            }

            let mut leaf_candidates = 0u32;
            let mut parsed_ok = 0u32;
            for entry in WalkDir::new(root)
                .max_depth(32)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if !is_session_leaf_dir(path) {
                    continue;
                }
                leaf_candidates += 1;
                let key = path.to_string_lossy().to_string();
                if !seen.insert(key) {
                    continue;
                }
                let index_path = path.join("index.json");
                let (mtime, size) = match codebuddy_fingerprint(path) {
                    Some(fingerprint) => fingerprint,
                    None => continue,
                };
                if unchanged(&index_path, mtime, size) {
                    continue;
                }

                match parse_codebuddy_session_dir(path) {
                    Ok(Some(s)) => {
                        sessions.push(s);
                        parsed_ok += 1;
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("解析 CodeBuddy 会话目录失败: {} — {}", path.display(), e),
                }
            }
            log::info!(
                "CodeBuddy：根 {} — 叶目录 {} 个，成功解析 {} 个会话",
                root.display(),
                leaf_candidates,
                parsed_ok
            );
        }

        log::info!("CodeBuddy 扩展采集完成，共 {} 个会话", sessions.len());
        sessions
    }
}

/// 会话叶目录：含 index.json（顶层 `messages` 数组非空）且存在 messages 子目录
fn is_session_leaf_dir(path: &Path) -> bool {
    let index_path = path.join("index.json");
    let msgs_dir = path.join("messages");
    if !index_path.is_file() || !msgs_dir.is_dir() {
        return false;
    }
    let Ok(text) = fs::read_to_string(&index_path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    match v.get("messages").and_then(|m| m.as_array()) {
        Some(arr) if !arr.is_empty() => true,
        _ => false,
    }
}

/// 解析单条 message 文件外层 JSON 中的 `message` 字段（JSON 字符串或已解析对象）
fn parse_message_inner(raw: &Value) -> Value {
    match raw.get("message") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(v @ Value::Object(_)) => v.clone(),
        _ => Value::Null,
    }
}

/// 从首条 **用户** 消息的 inner `content` 文本中提取工作区路径，返回 (绝对路径, 目录名)
///
/// ```text
/// messages/{id}.json → raw["message"] → inner_content_to_text（字符串 / text 块数组 / 旧版单字符数组）
///     → Workspace Folder: 或 Workspace Path: → Path::file_name → "proj"
/// ```
fn try_extract_project_from_first_user_message(
    session_dir: &Path,
    msg_list: &[Value],
) -> Option<(String, String)> {
    for meta in msg_list {
        if meta["role"].as_str() != Some("user") {
            continue;
        }
        let id = match meta["id"].as_str() {
            Some(s) if !s.is_empty() && !s.starts_with('.') => s,
            _ => continue,
        };
        let msg_file = session_dir.join("messages").join(format!("{}.json", id));
        if !msg_file.is_file() {
            continue;
        }
        let text = fs::read_to_string(&msg_file).ok()?;
        let raw: Value = serde_json::from_str(&text).ok()?;
        let inner = parse_message_inner(&raw);
        let folder = extract_workspace_folder_from_inner(&inner)?;
        let name = Path::new(&folder)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())?;
        return Some((folder, name));
    }
    None
}

/// 将内层 `message` 的 `content` 展平为一段可搜索文本（与 `docs/CodeBuddy 会话记录提取逻辑文档.md` 一致）。
///
/// - **字符串**：整段 payload（新版常见）
/// - **对象数组**：`[{ "type": "text", "text": "..." }, ...]`（新版 block）
/// - **字符串数组**：`["<","u","s",...]` 单字符片段 `join("")`（旧版 ~2025-11）
fn inner_content_to_text(inner: &Value) -> String {
    if let Some(s) = inner.get("content").and_then(|c| c.as_str()) {
        return s.to_string();
    }
    let Some(arr) = inner.get("content").and_then(|c| c.as_array()) else {
        return String::new();
    };
    if arr.is_empty() {
        return String::new();
    }
    // 新版：首项为对象 → 取 type==text 的 text（及 assistant 侧 reasoning/tool 在各自函数中处理）
    if matches!(arr.first(), Some(Value::Object(_))) {
        let mut buf = String::new();
        for b in arr {
            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = b.get("text").and_then(|x| x.as_str()) {
                    buf.push_str(t);
                }
            }
        }
        return buf;
    }
    // 旧版：JSON 字符串数组，每元素多为单字符，等价于 JS `content.join("")`
    if arr.iter().all(|v| v.as_str().is_some()) {
        return arr.iter().filter_map(|v| v.as_str()).collect();
    }
    String::new()
}

/// 从 inner 展平文本后解析工作区绝对路径（先 `Workspace Folder:`，再兼容旧版 `Workspace Path:`）。
fn extract_workspace_folder_from_inner(inner: &Value) -> Option<String> {
    let text = inner_content_to_text(inner);
    extract_workspace_folder_from_text_blob(&text)
}

/// 工作区目录 `index.json` 中 `conversations[].name`，用于列表/详情标题（与 IDE 侧会话列表一致）
fn try_conversation_title_from_workspace_index(
    workspace_dir: &Path,
    conversation_id: &str,
) -> Option<String> {
    let p = workspace_dir.join("index.json");
    let text = fs::read_to_string(&p).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let arr = v.get("conversations")?.as_array()?;
    for c in arr {
        if c.get("id").and_then(|x| x.as_str()) == Some(conversation_id) {
            let name = c.get("name").and_then(|x| x.as_str())?.trim();
            if name.is_empty() {
                return None;
            }
            return Some(truncate_chars(name, 200));
        }
    }
    None
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect::<String>() + "…"
}

/// 目录名看起来像日期戳或长 hex（无辨识度）时视为「不友好」
fn is_ugly_dir_label(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    if t.chars().all(|c| c.is_ascii_digit()) && t.len() >= 8 {
        return true;
    }
    if t.len() >= 16 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    false
}

/// 侧栏分组：Workspace Folder 末级为纯数字/长 hex 时，尝试用上一级目录名（如 `…/wenqu/20251201` → wenqu）
fn friendly_project_folder_name(workspace_abs: &str) -> String {
    let p = Path::new(workspace_abs);
    let base = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if is_ugly_dir_label(&base) {
        if let Some(parent) = p.parent() {
            if let Some(pname) = parent.file_name() {
                let pn = pname.to_string_lossy().to_string();
                if !pn.is_empty() && !is_ugly_dir_label(&pn) {
                    return pn;
                }
            }
        }
    }
    base
}

/// 在一段文本中查找 `key`（如 `Workspace Folder:`）后的绝对路径，截断到换行或 `<`。
fn extract_line_after_workspace_key(s: &str, key: &str) -> Option<String> {
    let idx = s.find(key)?;
    let rest = s[idx + key.len()..].trim_start();
    let end = rest
        .find('\n')
        .or_else(|| rest.find('\r'))
        .or_else(|| rest.find('<'))
        .unwrap_or(rest.len());
    let path = rest[..end].trim().trim_end_matches('`').trim();
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// 优先 `Workspace Folder:`（新版），否则 `Workspace Path:`（旧版，见文档 §5、§6）
fn extract_workspace_folder_from_text_blob(s: &str) -> Option<String> {
    extract_line_after_workspace_key(s, "Workspace Folder:")
        .or_else(|| extract_line_after_workspace_key(s, "Workspace Path:"))
}

fn parse_codebuddy_session_dir(
    session_dir: &Path,
) -> Result<Option<NormalizedSession>, Box<dyn std::error::Error + Send + Sync>> {
    let index_path = session_dir.join("index.json");
    let text = fs::read_to_string(&index_path)?;
    let index: Value = serde_json::from_str(&text)?;

    let msg_list = match index["messages"].as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return Ok(None),
    };

    let workspace_parent = session_dir.parent();
    let workspace_id = workspace_parent
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let leaf = session_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    // 跨工作区避免 session_id 碰撞：workspace_id + 会话目录名
    let session_id = format!("{}__{}", workspace_id, leaf);

    let (created_at, updated_at) = session_times_rfc3339(&index, &index_path)?;

    // 与 IDE 一致：工作区目录下 index.json 里对当前会话（目录名 = conversation id）的 name → 列表/详情主标题
    let analysis_title =
        workspace_parent.and_then(|wd| try_conversation_title_from_workspace_index(wd, &leaf));

    // Workspace Folder 绝对路径 → 侧栏「项目」分组名；末级为日期戳/hex 时尝试用父目录名
    let (project_path, project_name) =
        match try_extract_project_from_first_user_message(session_dir, msg_list) {
            Some((path, _raw_basename)) => {
                let friendly = friendly_project_folder_name(&path);
                log::debug!(
                    "CodeBuddy 会话 {} 工作区路径 {} → 侧栏项目分组名 {:?}",
                    session_id,
                    path,
                    friendly
                );
                (Some(path), Some(friendly))
            }
            None => {
                log::debug!(
                    "CodeBuddy 会话 {} 未解析到 Workspace Folder，侧栏项目名回退为工作区目录 id",
                    session_id
                );
                (None, Some(workspace_id.clone()))
            }
        };

    let mut messages: Vec<NormalizedMessage> = Vec::new();
    for meta in msg_list {
        let role = meta["role"].as_str().unwrap_or("");
        if role == "tool" {
            continue;
        }
        if role != "user" && role != "assistant" {
            continue;
        }
        let id = meta["id"].as_str().unwrap_or("");
        if id.is_empty() || id.starts_with('.') {
            continue;
        }
        let msg_file = session_dir.join("messages").join(format!("{}.json", id));
        if !msg_file.is_file() {
            continue;
        }
        let (content, tokens_in, tokens_out, ts) = load_one_message(&msg_file, role)?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        messages.push(NormalizedMessage {
            role: role.to_string(),
            content: trimmed.to_string(),
            timestamp: ts,
            tokens_in,
            tokens_out,
        });
    }

    if messages.is_empty() {
        return Ok(None);
    }

    let raw_path = index_path.to_string_lossy().to_string();
    let (raw_mtime_ms, raw_size_bytes) = codebuddy_fingerprint(session_dir).unwrap_or((0, 0));

    Ok(Some(NormalizedSession {
        source_id: SOURCE_ID.to_string(),
        session_id,
        project_path,
        project_name,
        analysis_title,
        messages,
        raw_path,
        raw_mtime_ms: Some(raw_mtime_ms),
        raw_size_bytes: Some(raw_size_bytes),
        created_at,
        updated_at,
    }))
}

fn codebuddy_fingerprint(session_dir: &Path) -> Option<(i64, i64)> {
    let mut fingerprint = file_fingerprint(&session_dir.join("index.json"))?;
    let messages_dir = session_dir.join("messages");
    if let Ok(entries) = fs::read_dir(messages_dir) {
        for entry in entries.flatten() {
            if let Some((mtime, size)) = file_fingerprint(&entry.path()) {
                fingerprint.0 = fingerprint.0.max(mtime);
                fingerprint.1 = fingerprint.1.saturating_add(size);
            }
        }
    }
    Some(fingerprint)
}

/// 从 index.json 的 requests[].startedAt（毫秒）推导时间；失败则用 index 文件 mtime
fn session_times_rfc3339(
    index: &Value,
    index_path: &Path,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let meta = fs::metadata(index_path)?;
    let fallback = file_mtime_rfc3339(&meta)?;

    let reqs = index["requests"].as_array();
    let first_ms = reqs
        .and_then(|r| r.first())
        .and_then(|x| x["startedAt"].as_u64());
    let last_ms = reqs
        .and_then(|r| r.last())
        .and_then(|x| x["startedAt"].as_u64());

    let created = first_ms
        .and_then(ms_to_rfc3339)
        .unwrap_or_else(|| fallback.clone());
    let updated = last_ms
        .and_then(ms_to_rfc3339)
        .unwrap_or_else(|| fallback.clone());

    Ok((created, updated))
}

fn ms_to_rfc3339(ms: u64) -> Option<String> {
    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    DateTime::from_timestamp(secs, nanos).map(|dt| dt.to_rfc3339())
}

fn file_mtime_rfc3339(
    meta: &std::fs::Metadata,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let t = meta.modified().or_else(|_| meta.created())?;
    let secs = t.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    let dt = DateTime::<Utc>::from_timestamp(secs, 0).unwrap_or_else(Utc::now);
    Ok(dt.to_rfc3339())
}

/// 读取 messages 下单条 JSON，解析内层 message 字符串 → 可见文本；token 优先从 extra.usage 取
fn load_one_message(
    path: &Path,
    role: &str,
) -> Result<(String, u32, u32, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let raw: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let inner = parse_message_inner(&raw);

    let mut tokens_in = 0u32;
    let mut tokens_out = 0u32;
    if let Some(extra_s) = raw["extra"].as_str() {
        if let Ok(extra) = serde_json::from_str::<Value>(extra_s) {
            if let Some(u) = extra.get("usage") {
                tokens_in = u["inputTokens"].as_u64().unwrap_or(0) as u32;
                tokens_out = u["outputTokens"].as_u64().unwrap_or(0) as u32;
            }
        }
    }

    let text = if role == "user" {
        extract_user_text(&raw, &inner)
    } else {
        extract_assistant_text(&inner)
    };

    Ok((text, tokens_in, tokens_out, None))
}

/// 用户消息：优先 &lt;user_query&gt;；其次 extra.inputPhrase；最后拼接 text 块（可能含系统上下文，仅作兜底）
fn extract_user_text(outer: &Value, inner: &Value) -> String {
    let combined = join_inner_text_blocks(inner);
    if let Some(uq) = extract_xml_section(&combined, "user_query") {
        return uq;
    }
    if let Some(extra_s) = outer["extra"].as_str() {
        if let Ok(extra) = serde_json::from_str::<Value>(extra_s) {
            if let Some(phrase) = extra["inputPhrase"].as_array().and_then(|a| a.first()) {
                if let Some(c) = phrase["content"].as_str() {
                    if !c.trim().is_empty() {
                        return c.to_string();
                    }
                }
            }
            if let Some(blocks) = extra["sourceContentBlocks"].as_array() {
                let mut parts = Vec::new();
                for b in blocks {
                    if let Some(t) = b["text"].as_str() {
                        if !t.trim().is_empty() {
                            parts.push(t);
                        }
                    }
                }
                if !parts.is_empty() {
                    return parts.join("\n");
                }
            }
        }
    }
    combined
}

/// 与 `ChatReplay.vue` / Claude 采集约定一致：`[Tool: name]` 供 Markdown 渲染为行内代码；`<thinking>` 可折叠
///
/// 新版为对象数组；旧版可能为单字符字符串数组（与 `inner_content_to_text` 文档一致），无结构化块时拼接为整段正文。
fn extract_assistant_text(inner: &Value) -> String {
    if let Some(s) = inner.get("content").and_then(|c| c.as_str()) {
        return s.to_string();
    }
    let Some(arr) = inner.get("content").and_then(|c| c.as_array()) else {
        return String::new();
    };
    if arr.is_empty() {
        return String::new();
    }
    if matches!(arr.first(), Some(Value::Object(_))) {
        let mut parts = Vec::new();
        for b in arr {
            let typ = b["type"].as_str().unwrap_or("");
            match typ {
                "text" => {
                    if let Some(t) = b["text"].as_str() {
                        if !t.trim().is_empty() {
                            parts.push(t.to_string());
                        }
                    }
                }
                "reasoning" => {
                    if let Some(t) = b["text"].as_str() {
                        let tt = t.trim();
                        if !tt.is_empty() {
                            parts.push(format!("<thinking>{}</thinking>", tt));
                        }
                    }
                }
                "tool-call" => {
                    let name = b["toolName"].as_str().unwrap_or("tool");
                    parts.push(format!("[Tool: {}]", name));
                }
                _ => {}
            }
        }
        return parts.join("\n");
    }
    if arr.iter().all(|v| v.as_str().is_some()) {
        return arr.iter().filter_map(|v| v.as_str()).collect::<String>();
    }
    String::new()
}

fn join_inner_text_blocks(inner: &Value) -> String {
    inner_content_to_text(inner)
}

fn extract_xml_section(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = s.find(&open)?;
    let rest = &s[start + open.len()..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_user_query_from_xml() {
        let s = "foo<user_query>你好</user_query>bar";
        assert_eq!(
            extract_xml_section(s, "user_query").as_deref(),
            Some("你好")
        );
    }

    #[test]
    fn extract_workspace_folder_from_user_info() {
        let s = "<user_info>\nOS: darwin\nWorkspace Folder: /Users/steve/Code/ssv-ai/wenqu\nNote:\n</user_info>";
        assert_eq!(
            extract_workspace_folder_from_text_blob(s).as_deref(),
            Some("/Users/steve/Code/ssv-ai/wenqu")
        );
    }

    #[test]
    fn extract_workspace_folder_stops_at_angle_bracket() {
        let s = "Workspace Folder: /tmp/proj<foo>";
        assert_eq!(
            extract_workspace_folder_from_text_blob(s).as_deref(),
            Some("/tmp/proj")
        );
    }

    /// 与 IDE 一致：首条 user 的 inner 里 `content` 为整段字符串（非 text 块数组）时仍能解析工作区
    #[test]
    fn extract_workspace_folder_from_inner_string_content() {
        let inner: Value = serde_json::from_str(
            r#"{"role":"user","content":"<user_info>\nWorkspace Folder: /Users/x/ssv/activity-service-php\n</user_info>"}"#,
        )
        .unwrap();
        assert_eq!(
            extract_workspace_folder_from_inner(&inner).as_deref(),
            Some("/Users/x/ssv/activity-service-php")
        );
    }

    /// 旧版（~2025-11）字段名 `Workspace Path:`，与文档 §5.2、§6 一致
    #[test]
    fn extract_workspace_path_old_key_only() {
        let s = "<user_info>\nWorkspace Path: /Users/legacy/old-key-proj\n</user_info>";
        assert_eq!(
            extract_workspace_folder_from_text_blob(s).as_deref(),
            Some("/Users/legacy/old-key-proj")
        );
    }

    /// 同时出现时优先 `Workspace Folder:`（新版）
    #[test]
    fn extract_workspace_prefers_folder_over_path() {
        let s = "Workspace Path: /wrong\nWorkspace Folder: /Users/right/correct\n";
        assert_eq!(
            extract_workspace_folder_from_text_blob(s).as_deref(),
            Some("/Users/right/correct")
        );
    }

    /// 旧版 content 为单字符 JSON 字符串数组，拼接后含 `Workspace Path:`
    #[test]
    fn extract_workspace_from_legacy_char_array() {
        let payload = "<user_info>\nWorkspace Path: /tmp/legacy-char-array\n</user_info>\n";
        let content: Vec<Value> = payload
            .chars()
            .map(|c| Value::String(c.to_string()))
            .collect();
        let inner = json!({ "role": "user", "content": content });
        assert_eq!(
            extract_workspace_folder_from_inner(&inner).as_deref(),
            Some("/tmp/legacy-char-array")
        );
    }

    /// 新版 text 块数组 + 旧版字段名
    #[test]
    fn extract_workspace_from_text_blocks_workspace_path() {
        let inner = json!({
            "role": "user",
            "content": [
                { "type": "text", "text": "<user_info>\nWorkspace Path: /Users/blocks/ws-path\n</user_info>" }
            ]
        });
        assert_eq!(
            extract_workspace_folder_from_inner(&inner).as_deref(),
            Some("/Users/blocks/ws-path")
        );
    }
}
