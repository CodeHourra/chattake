//! 会话列表 / 详情 / 提炼（Sidecar + DB）命令。
//!
//! ```text
//! 会话查询与后台分析流水线
//!   ├── 读 Session + Messages → 拼接 content（仅 user / assistant，排除 tool 等）
//!   ├── RPC init → judge_value
//!   ├── value ∈ {medium, high} → distill_full → insert_card
//!   └── value ∈ {low, none} → 仅更新会话价值，返回业务错误（无 Card）
//! ```
//!
//! # 参数命名约定
//! 使用 `#[tauri::command(rename_all = "camelCase")]` 让前端 camelCase
//! 自动映射到 Rust snake_case 参数，无需 `args` 包装结构体。

use tauri::State;
use url::Url;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::sidecar::SidecarManager;
use crate::storage::models::{
    CursorPage, Message, NewCard, PaginatedResult, Session, SessionFilters, SessionSummary,
};
use crate::storage::Database;
use crate::AppState;

/// 校验多组会话筛选：非空，且每组至少包含 source / host / project / status 之一。
fn validate_session_filter_groups(groups: &[SessionFilters]) -> Result<(), String> {
    if groups.is_empty() {
        return Err("请至少指定一组筛选条件".to_string());
    }
    for (i, g) in groups.iter().enumerate() {
        if g.source.is_none() && g.host.is_none() && g.project.is_none() && g.status.is_none() {
            return Err(format!("第 {} 组筛选条件为空", i + 1));
        }
    }
    Ok(())
}

/// Sidecar `judge_value` 返回结构（字段名与 TS 一致）
#[derive(Debug, serde::Deserialize)]
struct JudgeValueResult {
    value: String,
    reason: String,
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[derive(Debug, serde::Deserialize)]
struct PreprocessResult {
    content: String,
}

#[derive(Debug, serde::Deserialize)]
struct KnowledgeItem {
    title: String,
    #[serde(rename = "type")]
    card_type: String,
    summary: String,
    note: String,
    #[serde(default)]
    topic_tags: Vec<String>,
    #[serde(default, alias = "techStack")]
    technologies: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ExtractKnowledgeResult {
    items: Vec<KnowledgeItem>,
    prompt_tokens: i64,
    completion_tokens: i64,
}

/// 分页查询会话列表。
/// 参数全部 camelCase，与前端 `SessionListParams` 字段一致。
#[tauri::command(rename_all = "camelCase")]
pub async fn list_sessions(
    state: State<'_, AppState>,
    source: Option<String>,
    host: Option<String>,
    project: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<PaginatedResult<SessionSummary>, String> {
    let db = state.db.clone();
    let filters = SessionFilters {
        source,
        host,
        project,
        status,
        search: None,
    };
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).max(1).min(200);

    tokio::task::spawn_blocking(move || {
        db.list_sessions(&filters, page, page_size)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("list_sessions join 失败: {}", e))?
}

/// 统计多组筛选并集下的会话数量（确认弹窗用，与 `delete_sessions_by_filter_groups` 范围一致）。
#[tauri::command(rename_all = "camelCase")]
pub async fn count_sessions_by_filter_groups(
    state: State<'_, AppState>,
    groups: Vec<SessionFilters>,
) -> Result<u64, String> {
    validate_session_filter_groups(&groups)?;
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.count_sessions_by_filter_groups(&groups)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("count_sessions_by_filter_groups join 失败: {}", e))?
}

/// 按多组筛选并集批量删除会话（侧栏「会话整理」多选）。
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_sessions_by_filter_groups(
    state: State<'_, AppState>,
    groups: Vec<SessionFilters>,
) -> Result<u64, String> {
    validate_session_filter_groups(&groups)?;
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.delete_sessions_by_filter_groups(&groups)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("delete_sessions_by_filter_groups join 失败: {}", e))?
}

/// 获取会话完整信息。
#[tauri::command]
pub async fn get_session(state: State<'_, AppState>, id: String) -> Result<Session, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.get_session(&id).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("get_session join 失败: {}", e))?
}

/// 游标分页拉取会话消息，单页最多 100 条。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_session_messages(
    state: State<'_, AppState>,
    session_id: String,
    cursor: Option<i64>,
    limit: Option<u32>,
) -> Result<CursorPage<Message>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.get_session_messages_page(&session_id, cursor, limit.unwrap_or(100))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("get_session_messages join 失败: {}", e))?
}

const PROMPT_VERSION: &str = "v0.2.0-knowledge-1";

pub(crate) fn run_distill_pipeline(
    db: &Database,
    config: &AppConfig,
    sidecar: &SidecarManager,
    session_db_id: &str,
    job_id: &str,
    provider_profile_id: Option<&str>,
    on_phase: Option<&dyn Fn(&str)>,
) -> Result<(), String> {
    let started = std::time::Instant::now();
    let session = db.get_session(session_db_id).map_err(|e| e.to_string())?;
    let messages = db
        .get_session_messages(session_db_id)
        .map_err(|e| e.to_string())?;

    if messages.is_empty() {
        return Err("该会话没有消息，无法提炼".to_string());
    }

    let trace_id = Uuid::new_v4().to_string();
    log::info!(
        "distill 开始 trace_id={} session_id={}",
        trace_id,
        session_db_id
    );

    let included_count = messages
        .iter()
        .filter(|m| is_included_distill_message(m))
        .count();
    log::info!(
        "trace_id={} distill transcript: 数据库消息 {} 条，纳入 user/assistant/model {} 条",
        trace_id,
        messages.len(),
        included_count
    );

    let content = build_transcript(&messages);
    log_rpc_distill_payload(
        &trace_id,
        "RPC content（Rust→Sidecar Params.content，preprocess 前）",
        &content,
    );
    if content.trim().is_empty() {
        return Err("会话正文为空，无法提炼".to_string());
    }

    let profile = config.provider_profile(provider_profile_id)?;
    let base_url_host = if profile.kind == "cli" {
        "local-cli".to_string()
    } else {
        Url::parse(&profile.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "invalid-base-url".to_string())
    };
    let content_hash = session
        .content_hash
        .clone()
        .unwrap_or_else(|| format!("{:x}", md5::compute(content.as_bytes())));
    let run_id = db
        .create_analysis_run(
            job_id,
            session_db_id,
            &profile.id,
            &profile.provider,
            &base_url_host,
            &profile.model,
            &content_hash,
            PROMPT_VERSION,
        )
        .map_err(|e| e.to_string())?;

    db.update_session_status(session_db_id, "analyzing", None)
        .map_err(|e| e.to_string())?;

    let mut value: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut judge_tokens = (0, 0);
    let mut extract_tokens = (0, 0);
    let result = (|| -> Result<(), String> {
        let processed: PreprocessResult = sidecar
            .call_with_timeout(
                "preprocess",
                serde_json::json!({ "content": content, "traceId": trace_id }),
                std::time::Duration::from_secs(30),
            )
            .map_err(|e| format!("对话预处理失败：{e}"))?;

        if let Some(callback) = on_phase {
            callback("judging");
        }
        let judge: JudgeValueResult = sidecar
            .call_with_timeout(
                "judge_value",
                serde_json::json!({ "content": processed.content, "traceId": trace_id }),
                std::time::Duration::from_secs(120),
            )
            .map_err(|e| format!("价值判断失败：{e}"))?;

        let v_norm = judge.value.to_lowercase();
        if !matches!(v_norm.as_str(), "high" | "medium" | "low" | "none") {
            return Err(format!("响应格式错误：未知价值等级 {}", judge.value));
        }
        value = Some(v_norm.clone());
        reason = Some(judge.reason.clone());
        judge_tokens = (judge.prompt_tokens, judge.completion_tokens);
        log::info!(
            "trace_id={} 价值判断结果: {} (session={})",
            trace_id,
            v_norm,
            session_db_id
        );

        if v_norm == "low" || v_norm == "none" {
            db.update_session_status(session_db_id, "analyzed", Some(&v_norm))
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        if let Some(callback) = on_phase {
            callback("extracting");
        }
        let extracted: ExtractKnowledgeResult = sidecar
            .call_with_timeout(
                "extract_knowledge",
                serde_json::json!({ "content": processed.content, "traceId": trace_id }),
                std::time::Duration::from_secs(300),
            )
            .map_err(|e| format!("知识提取失败：{e}"))?;
        if extracted.items.is_empty() || extracted.items.len() > 3 {
            return Err("响应格式错误：知识项数量必须为 1–3".into());
        }
        extract_tokens = (extracted.prompt_tokens, extracted.completion_tokens);

        let source_name = config.source_display_name(&session.source_id);
        let project_name = session.project_name.clone();
        let publication_status = if v_norm == "high"
            && !db
                .session_has_cards(session_db_id)
                .map_err(|e| e.to_string())?
        {
            "published"
        } else {
            "draft"
        };
        for item in &extracted.items {
            db.insert_card(&NewCard {
                session_id: session_db_id,
                analysis_run_id: &run_id,
                title: &item.title,
                card_type: &item.card_type,
                value: &v_norm,
                summary: &item.summary,
                note: &item.note,
                publication_status,
                source_name: source_name.as_deref(),
                project_name: project_name.as_deref(),
                prompt_tokens: extracted.prompt_tokens.clamp(0, i32::MAX as i64) as i32,
                completion_tokens: extracted.completion_tokens.clamp(0, i32::MAX as i64) as i32,
                cost_yuan: 0.0,
                tags: &item.topic_tags,
                tech_stack: &item.technologies,
            })
            .map_err(|e| e.to_string())?;
        }

        db.update_session_status(session_db_id, "analyzed", Some(&v_norm))
            .map_err(|e| e.to_string())?;
        Ok(())
    })();

    let error = result.as_ref().err().map(String::as_str);
    let error_kind = error.map(classify_analysis_error);
    db.finish_analysis_run(
        &run_id,
        value.as_deref(),
        reason.as_deref(),
        judge_tokens,
        extract_tokens,
        started.elapsed().as_millis().min(i64::MAX as u128) as i64,
        error_kind,
        error,
    )
    .map_err(|e| e.to_string())?;

    if let Err(error) = result {
        let kind = classify_analysis_error(&error);
        let visible = format!(
            "{}/{} · {}：{}",
            profile.provider, profile.model, kind, error
        );
        let _ = db.update_session_error(session_db_id, &visible);
        Err(visible)
    } else {
        Ok(())
    }
}

fn classify_analysis_error(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("401")
        || text.contains("403")
        || text.contains("api key")
        || text.contains("鉴权")
    {
        "鉴权失败"
    } else if text.contains("429") || text.contains("rate limit") || text.contains("限流") {
        "请求限流"
    } else if text.contains("model")
        && (text.contains("404") || text.contains("not found") || text.contains("不存在"))
    {
        "模型不存在"
    } else if text.contains("timeout") || text.contains("超时") {
        "请求超时"
    } else if text.contains("json") || text.contains("响应格式") {
        "响应格式错误"
    } else if text.contains("network")
        || text.contains("connect")
        || text.contains("dns")
        || text.contains("网络")
    {
        "网络错误"
    } else {
        "供应商请求失败"
    }
}

/// 是否纳入送给提炼模型的正文（仅保留人类用户与助手可见轮次）。
///
/// ```text
/// 包含: user, assistant, model（model 为部分 API 对助手回复的别名；大小写不敏感）
/// 排除: tool（工具回传/bash 输出等）、system 及未知角色
/// ```
///
/// 说明：采集层已将「纯 tool_result」标为 role=tool（见 claude_code），此处直接跳过即可，
/// 避免浪费 token、并减少与「真实对话」无关的噪声。
fn is_distill_dialogue_role(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "user" | "assistant" | "model"
    )
}

/// 是否纳入 transcript：角色符合且正文非空（与 `build_transcript` 规则一致）。
fn is_included_distill_message(m: &Message) -> bool {
    is_distill_dialogue_role(&m.role) && !m.content.trim().is_empty()
}

/// 将消息列表拼成单一字符串，供 LLM 前处理（角色标签 + 换行分隔）。
///
/// 仅拼接 user / assistant / model 对应消息（输出仍带原始 role 标签）；`tool`、空内容条目不写入。
fn build_transcript(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages.iter().filter(|m| is_included_distill_message(m)) {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push('[');
        out.push_str(&m.role);
        out.push_str("]\n");
        out.push_str(&m.content);
    }
    out
}

// ── 提炼载荷日志（与 sidecar `payload-log.ts` 可对拍）──────────────────────────

fn utf8_safe_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn utf8_safe_suffix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// 记录 JSON-RPC 中传给 sidecar 的 `content`（未经 clean/truncate）。
/// 与 sidecar 日志里 `user.md5[0:16]`：若预处理未改长度则应与 API 侧 user 一致。
/// 设置环境变量 `CHATTAKE_LOG_DISTILL_PAYLOAD=1` 可打印完整正文（排查后关闭）。
fn log_rpc_distill_payload(trace_id: &str, label: &str, content: &str) {
    let digest = md5::compute(content.as_bytes());
    let hex_full = format!("{:x}", digest);
    let md5_prefix: String = hex_full.chars().take(16).collect();
    log::info!(
        "trace_id={} | {}: UTF-8 字节数={}, md5[0:16]={}（与 sidecar `user.md5[0:16]` 对照；若仅截断则尾部不同）",
        trace_id,
        label,
        content.len(),
        md5_prefix
    );
    const PREVIEW: usize = 2500;
    let head = utf8_safe_prefix(content, PREVIEW);
    log::info!(
        "trace_id={} | {}: [HEAD {} bytes]\n{}",
        trace_id,
        label,
        head.len(),
        head
    );
    if content.len() > PREVIEW {
        let tail = utf8_safe_suffix(content, PREVIEW);
        log::info!(
            "trace_id={} | {}: [TAIL {} bytes]\n{}",
            trace_id,
            label,
            tail.len(),
            tail
        );
    }
    if std::env::var("CHATTAKE_LOG_DISTILL_PAYLOAD")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        log::info!(
            "trace_id={} | {}: [FULL 由 CHATTAKE_LOG_DISTILL_PAYLOAD=1 开启]\n{}",
            trace_id,
            label,
            content
        );
    }
}

#[cfg(test)]
mod transcript_tests {
    use super::*;

    fn sample_message(role: &str, content: &str) -> Message {
        Message {
            id: "m1".into(),
            session_id: "s1".into(),
            role: role.into(),
            content: content.into(),
            timestamp: None,
            tokens_in: 0,
            tokens_out: 0,
            seq_order: 0,
        }
    }

    #[test]
    fn build_transcript_drops_tool_and_keeps_order() {
        let messages = vec![
            sample_message("user", "问题"),
            sample_message("tool", "ls -la\nfoo"),
            sample_message("Assistant", "回答"),
            sample_message("tool", "stderr..."),
        ];
        let t = build_transcript(&messages);
        assert!(!t.contains("[tool]"), "不应包含 tool 段: {}", t);
        assert!(t.starts_with("[user]"));
        assert!(t.contains("[Assistant]")); // 保留采集层原始大小写
        assert!(t.contains("问题") && t.contains("回答"));
    }

    #[test]
    fn build_transcript_accepts_model_as_assistant_alias() {
        let messages = vec![sample_message("model", "来自 model 角色的回复")];
        let t = build_transcript(&messages);
        assert!(t.contains("[model]"));
        assert!(t.contains("来自 model"));
    }

    #[test]
    fn build_transcript_skips_empty_user_assistant() {
        let messages = vec![
            sample_message("user", "   "),
            sample_message("assistant", "有内容"),
        ];
        let t = build_transcript(&messages);
        assert!(!t.contains("[user]"));
        assert!(t.contains("[assistant]"));
    }
}
