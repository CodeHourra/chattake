//! 采集统一格式 —— 所有数据源解析后转换为此格式，再写入 SQLite。
//!
//! ```text
//! 数据源文件 → Source Parser → NormalizedSession → Dedup → SQLite
//! ```

use std::path::Path;

/// 统一的会话格式，各数据源解析器产出此结构后交给调度器去重写入。
#[derive(Debug, Clone)]
pub struct NormalizedSession {
    /// 数据源 ID（如 "claude-code"、"cursor"），对应 config.toml 中的 source.id
    pub source_id: String,
    /// 原始会话 ID（数据源内部标识，如 UUID）
    pub session_id: String,
    /// 关联项目的绝对路径
    pub project_path: Option<String>,
    /// 项目名称（从路径末段推导）
    pub project_name: Option<String>,
    /// 采集阶段可填写的会话展示标题（如 CodeBuddy 工作区 index 中的会话 name），写入 `sessions.analysis_title`
    pub analysis_title: Option<String>,
    /// 该会话包含的消息列表（按时间顺序）
    pub messages: Vec<NormalizedMessage>,
    /// 原始数据文件路径（如 JSONL 文件位置）
    pub raw_path: String,
    /// 原文件指纹；下次扫描先比较这两个值，命中时不再解析。
    pub raw_mtime_ms: Option<i64>,
    pub raw_size_bytes: Option<i64>,
    /// 会话创建时间 (RFC 3339)
    pub created_at: String,
    /// 会话最后更新时间 (RFC 3339)
    pub updated_at: String,
}

#[derive(Debug, Default)]
pub struct CollectionBatch {
    pub sessions: Vec<NormalizedSession>,
    pub failures: Vec<CollectionFailure>,
    pub found: u32,
    pub skipped: u32,
    pub skipped_paths: Vec<String>,
}

#[derive(Debug)]
pub struct CollectionFailure {
    pub raw_path: String,
    pub error: String,
}

/// 统一的单条消息格式
#[derive(Debug, Clone)]
pub struct NormalizedMessage {
    /// 角色: "user" | "assistant"
    pub role: String,
    /// 消息文本内容（仅保留用户与助手可见正文）
    pub content: String,
    /// 消息时间戳 (RFC 3339)
    pub timestamp: Option<String>,
    /// 该消息消耗的输入 token 数
    pub tokens_in: u32,
    /// 该消息消耗的输出 token 数
    pub tokens_out: u32,
}

/// 保留用户实际触发的 slash command，丢弃 CLI 伪装成 user 消息的运行时注入。
pub fn normalize_user_content(content: &str) -> Option<String> {
    let content = content.trim_start();
    if content.starts_with("<command-name>") || content.starts_with("<command-message>") {
        let command = tag_value(content, "command-name")
            .or_else(|| tag_value(content, "command-message"))
            .unwrap_or_default();
        let args = tag_value(content, "command-args").unwrap_or_default();
        let visible = format!("{command} {args}").trim().to_string();
        return (!visible.is_empty()).then_some(visible);
    }
    let internal = [
        "# AGENTS.md instructions",
        "<INSTRUCTIONS>",
        "<app-context>",
        "<environment_context>",
        "<skills_instructions>",
        "<permissions instructions>",
        "<collaboration_mode>",
        "<apps_instructions>",
        "<plugins_instructions>",
        "<recommended_plugins>",
        "<user_instructions>",
        "<subagent_notification>",
        "<task-notification>",
        "<turn_aborted>",
        "<skill>",
        "<in-app-browser-context ",
        "<user_action>",
        "<system-reminder>",
        "<local-command-caveat>",
        "<command-args>",
        "<local-command-stdout>",
        "<local-command-stderr>",
        "The following is the Codex agent history",
        "Base directory for this skill:",
    ]
    .iter()
    .any(|prefix| content.starts_with(prefix));
    (!internal).then(|| content.trim().to_string())
}

fn tag_value<'a>(content: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let (_, rest) = content.split_once(&open)?;
    rest.split_once(&close).map(|(value, _)| value.trim())
}

pub fn file_fingerprint(path: &Path) -> Option<(i64, i64)> {
    let metadata = path.metadata().ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some((
        modified.as_millis().min(i64::MAX as u128) as i64,
        metadata.len().min(i64::MAX as u64) as i64,
    ))
}

#[cfg(test)]
mod tests {
    use super::normalize_user_content;

    #[test]
    fn identifies_runtime_injections_without_hiding_real_requests() {
        assert!(normalize_user_content("<environment_context>secret").is_none());
        assert!(normalize_user_content("# AGENTS.md instructions\nrules").is_none());
        assert!(normalize_user_content("<local-command-caveat>generated").is_none());
        assert!(normalize_user_content("<command-args>orphan</command-args>").is_none());
        assert!(normalize_user_content(
            "The following is the Codex agent history whose request action you are assessing"
        )
        .is_none());
        assert!(
            normalize_user_content("<subagent_notification>done</subagent_notification>").is_none()
        );
        assert!(normalize_user_content("<task-notification>result</task-notification>").is_none());
        assert!(normalize_user_content(
            "<in-app-browser-context source=\"ambient-ui-state\">state"
        )
        .is_none());
        assert!(
            normalize_user_content("Base directory for this skill: /tmp/skills/demo").is_none()
        );
        assert_eq!(
            normalize_user_content("<command-name>/understand</command-name>\n<command-message>ignored duplicate</command-message>\n<command-args>--language zh</command-args>").as_deref(),
            Some("/understand --language zh")
        );
        assert_eq!(
            normalize_user_content("请检查 AGENTS.md instructions 是否合理").as_deref(),
            Some("请检查 AGENTS.md instructions 是否合理")
        );
    }
}
