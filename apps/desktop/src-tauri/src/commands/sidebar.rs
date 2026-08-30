//! 侧栏数据命令 —— 会话目录树、标签列表、卡片类型统计。

use tauri::State;

use crate::storage::models::{SessionGroupCount, TagCount, TypeCount};
use crate::AppState;

/// 返回会话按 source → host → project 的分组统计，前端据此构建目录树。
#[tauri::command]
pub async fn get_session_groups(
    state: State<'_, AppState>,
) -> Result<Vec<SessionGroupCount>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.get_session_groups().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("get_session_groups join 失败: {}", e))?
}

/// 返回所有标签及关联卡片数量（按数量降序），用于知识库侧栏标签筛选。
#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagCount>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.list_all_tags().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("list_tags join 失败: {}", e))?
}

/// 返回 `cards.tech_stack` 聚合后的技术栈及卡片命中次数（知识库侧栏展示）。
#[tauri::command]
pub async fn list_tech_stack_counts(state: State<'_, AppState>) -> Result<Vec<TagCount>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.list_all_tech_stack_counts().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("list_tech_stack_counts join 失败: {}", e))?
}

/// 返回各知识类型的卡片数量统计，用于知识库侧栏类型筛选。
#[tauri::command]
pub async fn list_card_types(state: State<'_, AppState>) -> Result<Vec<TypeCount>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.list_card_type_counts().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("list_card_types join 失败: {}", e))?
}
