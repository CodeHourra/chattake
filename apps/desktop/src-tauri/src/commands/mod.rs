//! Tauri `invoke` 命令入口 —— 按领域拆分子模块。
//!
//! `lib.rs` 的 `generate_handler!` 须使用完整路径（如 `commands::sync::sync_all`），
//! 以便 `#[tauri::command]` 生成的 `__cmd__*` 符号解析正确。

pub mod cards;
pub mod config;
pub mod export;
pub mod jobs;
pub mod sessions;
pub mod sidebar;
