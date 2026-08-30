//! 会话列表 / 详情 / 提炼（Sidecar + DB）命令。
//!
//! ```text
//! distill_session (spawn_blocking)
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
use uuid::Uuid;

use crate::config::AppConfig;
use crate::sidecar::SidecarManager;
use crate::storage::models::{
    Card, Message, NewCard, PaginatedResult, Session, SessionFilters, SessionSummary,
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
    #[serde(rename = "type")]
    card_type: String,
    /// 低/无价值时的原因（也用于构造标题和摘要）
    reason: String,
    #[allow(dead_code)]
    prompt_tokens: i64,
    #[allow(dead_code)]
    completion_tokens: i64,
}

/// Sidecar `distill_full` 返回结构
#[derive(Debug, serde::Deserialize)]
struct DistillFullResult {
    title: String,
    #[serde(rename = "type")]
    card_type: String,
    value: String,
    summary: String,
    note: String,
    #[serde(default)]
    tags: Vec<String>,
    /// 与 prompt 约定为 snake_case；兼容少数模型输出 camelCase `techStack`
    #[serde(default, alias = "techStack")]
    tech_stack: Vec<String>,
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

/// 拉取会话下全部消息（按 seq_order 排序），供详情页对话回放。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_session_messages(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<Message>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.get_session_messages(&session_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("get_session_messages join 失败: {}", e))?
}

/// `distill_session` 返回结果
///
/// ```text
/// is_low_value = true  → low / none，DB 已记录价值，无 Card 产出
/// is_low_value = false → medium / high，Card 已写库
/// ```
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistillSessionResult {
    /// 本次提炼流水线 id，与 sidecar / API 日志中的 traceId 一致，便于串联排查（如 tech_stack）
    pub trace_id: String,
    /// 分析后的价值等级（high / medium / low / none）
    pub value: String,
    /// true = 低/无价值，未生成笔记；false = 已生成笔记
    pub is_low_value: bool,
    /// 低/无价值时：由 reason 构造的简短标题
    pub card_title: Option<String>,
    /// 低/无价值时：judge_value 返回的对话类型（debug / learning / …）
    pub card_type: Option<String>,
    /// 低/无价值时的原因说明（作为摘要展示）
    pub reason: Option<String>,
    /// 生成的卡片（仅 is_low_value = false 时有值）
    pub card: Option<Card>,
}

/// 对指定会话执行价值判断 +（可选）完整提炼，并写入 `cards` 表。
#[tauri::command(rename_all = "camelCase")]
pub async fn distill_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DistillSessionResult, String> {
    let sidecar = state.sidecar.clone().ok_or_else(|| {
        "未找到 xunji-sidecar 可执行文件，请先构建 packages/sidecar 或安装到 ~/.xunji/bin/"
            .to_string()
    })?;

    let db = state.db.clone();
    // 取当前配置快照（RwLock → Clone），传入 spawn_blocking
    let config = state.config_snapshot();

    tokio::task::spawn_blocking(move || {
        run_distill_pipeline(&db, &config, sidecar.as_ref(), &session_id)
    })
    .await
    .map_err(|e| format!("distill_session join 失败: {}", e))?
}

/// 在阻塞线程内跑完 DB + Sidecar 全流程（避免阻塞 tokio worker）。
///
/// 流程：
/// ```text
/// analyzing 状态 → init → judge_value (PROMPT_B_LIGHT)
///   ├── low / none → 更新 DB 为 analyzed + value → Ok(isLowValue=true)
///   └── medium / high → distill_full (PROMPT_B_FULL) → insert_card → Ok(isLowValue=false)
/// ```
fn run_distill_pipeline(
    db: &Database,
    config: &AppConfig,
    sidecar: &SidecarManager,
    session_db_id: &str,
) -> Result<DistillSessionResult, String> {
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

    // 设置 analyzing 状态
    db.update_session_status(session_db_id, "analyzing", None)
        .map_err(|e| e.to_string())?;

    // 用闭包包裹后续逻辑，确保任何真正失败都能将状态回退为 error
    let result = (|| -> Result<DistillSessionResult, String> {
        let init_params = config.sidecar_init_params(None)?;

        sidecar
            .call_with_timeout::<serde_json::Value>(
                "init",
                init_params,
                std::time::Duration::from_secs(30),
            )
            .map_err(|e| format!("Sidecar init 失败：{}", e))?;

        // ── 第一步：轻量价值判断（PROMPT_B_LIGHT） ──
        let judge: JudgeValueResult = sidecar
            .call_with_timeout(
                "judge_value",
                serde_json::json!({ "content": content, "traceId": trace_id }),
                std::time::Duration::from_secs(120),
            )
            .map_err(|e| format!("价值判断（judge_value）失败：{}", e))?;

        let v_norm = judge.value.to_lowercase();
        log::info!(
            "trace_id={} 价值判断结果: {} (session={})",
            trace_id,
            v_norm,
            session_db_id
        );

        // ── low / none：更新价值 + 持久化标题/类型/原因 → 正常返回（非错误） ──
        if v_norm == "low" || v_norm == "none" {
            if let Err(e) = db.update_session_status(session_db_id, "analyzed", Some(&judge.value))
            {
                log::error!("低/无价值会话状态回写失败: {}", e);
            }

            // 由 reason 截取前 30 字符作为显示标题（不截断词，保留语义）
            let title = build_analysis_title(&judge.reason);

            // 将标题、类型、原因一并持久化，刷新后列表仍可正确展示
            if let Err(e) = db.update_session_analysis_meta(
                session_db_id,
                &title,
                &judge.card_type,
                &judge.reason,
            ) {
                log::error!("写入 analysis_meta 失败: {}", e);
            }

            return Ok(DistillSessionResult {
                trace_id: trace_id.clone(),
                value: judge.value,
                is_low_value: true,
                card_title: Some(title),
                card_type: Some(judge.card_type),
                reason: Some(judge.reason),
                card: None,
            });
        }

        if v_norm != "medium" && v_norm != "high" {
            return Err(format!("未知的价值等级: {}", judge.value));
        }

        // ── 第二步：完整笔记提炼（PROMPT_B_FULL，仅 medium / high） ──
        let full: DistillFullResult = sidecar
            .call_with_timeout(
                "distill_full",
                serde_json::json!({ "content": content, "traceId": trace_id }),
                std::time::Duration::from_secs(300),
            )
            .map_err(|e| format!("完整提炼（distill_full）失败：{}", e))?;

        log::info!(
            "trace_id={} distill_full 解析入库: tags 条目数={} {:?} | tech_stack 条目数={} {:?}",
            trace_id,
            full.tags.len(),
            full.tags,
            full.tech_stack.len(),
            full.tech_stack
        );

        db.delete_cards_for_session(session_db_id)
            .map_err(|e| e.to_string())?;

        let source_name = config.source_display_name(&session.source_id);
        let project_name = session.project_name.clone();

        let new_card = NewCard {
            session_id: session_db_id,
            title: full.title.as_str(),
            card_type: Some(full.card_type.as_str()),
            value: Some(full.value.as_str()),
            summary: Some(full.summary.as_str()),
            note: full.note.as_str(),
            source_name: source_name.as_deref(),
            project_name: project_name.as_deref(),
            prompt_tokens: full.prompt_tokens.clamp(0, i32::MAX as i64) as i32,
            completion_tokens: full.completion_tokens.clamp(0, i32::MAX as i64) as i32,
            cost_yuan: 0.0,
            tags: &full.tags,
            tech_stack: &full.tech_stack,
        };

        let card_id = db.insert_card(&new_card).map_err(|e| e.to_string())?;

        db.update_session_status(session_db_id, "analyzed", Some(&full.value))
            .map_err(|e| e.to_string())?;

        let card = db.get_card(&card_id).map_err(|e| e.to_string())?;
        Ok(DistillSessionResult {
            trace_id: trace_id.clone(),
            value: full.value,
            is_low_value: false,
            card_title: None,
            card_type: None,
            reason: None,
            card: Some(card),
        })
    })();

    // 真正失败时（非低价值场景），状态回退为 error
    if let Err(ref e) = result {
        if let Ok(s) = db.get_session(session_db_id) {
            if s.status == "analyzing" {
                let _ = db.update_session_error(session_db_id, e);
            }
        }
    }

    result
}

/// 从 LLM 判断原因截取简短标题（最多 30 个 Unicode 字符，超出时附加省略号）。
///
/// 示例：
///   "问题过于简单，答案是常识" → "问题过于简单，答案是常识"（未超长）
///   "内容高度重复，没有新知识点，用户反复询问相同内容，建议归档。" → "内容高度重复，没有新知识点，用户反…"
fn build_analysis_title(reason: &str) -> String {
    const MAX_CHARS: usize = 30;
    let chars: Vec<char> = reason.chars().collect();
    if chars.len() <= MAX_CHARS {
        reason.to_string()
    } else {
        chars[..MAX_CHARS].iter().collect::<String>() + "…"
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
/// 设置环境变量 `XUNJI_LOG_DISTILL_PAYLOAD=1` 可打印完整正文（排查后关闭）。
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
    if std::env::var("XUNJI_LOG_DISTILL_PAYLOAD")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        log::info!(
            "trace_id={} | {}: [FULL 由 XUNJI_LOG_DISTILL_PAYLOAD=1 开启]\n{}",
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
