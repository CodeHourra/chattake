//! 知识卡片相关命令 —— 列表、详情、全文搜索。
//!
//! `#[tauri::command(rename_all = "camelCase")]` 让前端 camelCase 参数
//! 自动映射到 Rust snake_case，无需 `args` 包装结构体。

use tauri::State;

use crate::storage::models::{Card, CardFilters, CardSummary, CardUpdate, PaginatedResult, Tag};
use crate::AppState;

/// FTS5 全文搜索知识卡片。
#[tauri::command(rename_all = "camelCase")]
pub async fn search_cards(
    state: State<'_, AppState>,
    query: String,
    tags: Option<Vec<String>>,
    card_type: Option<String>,
    tech_stack: Option<Vec<String>>,
) -> Result<Vec<CardSummary>, String> {
    let db = state.db.clone();
    let filters = CardFilters {
        publication_status: None,
        tags,
        card_type,
        value: None,
        search: None,
        tech_stack,
    };
    tokio::task::spawn_blocking(move || {
        db.search_cards(&query, &filters).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("search_cards join 失败: {}", e))?
}

/// 分页查询知识卡片列表。
#[tauri::command(rename_all = "camelCase")]
pub async fn list_cards(
    state: State<'_, AppState>,
    tags: Option<Vec<String>>,
    card_type: Option<String>,
    value: Option<String>,
    tech_stack: Option<Vec<String>>,
    publication_status: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<PaginatedResult<CardSummary>, String> {
    let db = state.db.clone();
    let filters = CardFilters {
        publication_status,
        tags,
        card_type,
        value,
        search: None,
        tech_stack,
    };
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).max(1).min(200);
    tokio::task::spawn_blocking(move || {
        db.list_cards(&filters, page, page_size)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("list_cards join 失败: {}", e))?
}

/// 获取单张知识卡片完整信息（含关联标签）。
#[tauri::command]
pub async fn get_card(state: State<'_, AppState>, id: String) -> Result<Card, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.get_card(&id).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("get_card join 失败: {}", e))?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn update_card(
    state: State<'_, AppState>,
    id: String,
    title: String,
    card_type: String,
    summary: String,
    note: String,
    tags: Vec<String>,
    technologies: Vec<String>,
) -> Result<Card, String> {
    if title.trim().is_empty() || summary.trim().is_empty() || note.trim().is_empty() {
        return Err("标题、摘要和正文不能为空".into());
    }
    if !matches!(
        card_type.as_str(),
        "decision" | "troubleshooting" | "implementation" | "explanation" | "snippet"
    ) {
        return Err("知识类型无效".into());
    }
    if tags.len() > 3 || technologies.len() > 5 {
        return Err("主题标签最多 3 个，技术项最多 5 个".into());
    }
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.update_card(
            &id,
            &CardUpdate {
                title: title.trim(),
                card_type: &card_type,
                summary: summary.trim(),
                note: note.trim(),
                tags: &tags,
                technologies: &technologies,
            },
        )
        .and_then(|_| db.get_card(&id))
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("update_card join 失败: {e}"))?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn publish_card(
    state: State<'_, AppState>,
    id: String,
    replace_existing: bool,
) -> Result<Card, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.publish_card(&id, replace_existing)
            .and_then(|_| db.get_card(&id))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("publish_card join 失败: {e}"))?
}

#[tauri::command]
pub async fn list_tag_records(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.list_tag_records().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("list_tag_records join 失败: {e}"))?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn merge_tags(
    state: State<'_, AppState>,
    kind: String,
    sources: Vec<String>,
    target: String,
) -> Result<(), String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        db.merge_tags(&kind, &sources, &target)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("merge_tags join 失败: {e}"))?
}
