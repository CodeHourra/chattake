mod collector;
mod commands;
pub mod config;
/// Cursor workspace 路径百分号解码（采集与 DB 迁移共用）
mod path_local;
mod sidecar;
mod storage;

use std::sync::{Arc, RwLock};
use tauri::Manager;

use config::AppConfig;
use sidecar::SidecarManager;
use storage::Database;

/// 应用全局状态，由 Tauri manage() 注入，各 command 通过 State<AppState> 访问
///
/// ```text
/// Arc<Database>              —— 供 spawn_blocking 中克隆使用
/// Arc<RwLock<AppConfig>>     —— 支持运行时热更新（设置页保存配置后无需重启）
/// Option<Arc<SidecarManager>> —— 未找到 sidecar 二进制时为 None
/// ```
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<AppConfig>>,
    pub sidecar: Option<Arc<SidecarManager>>,
    pub job_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    /// 获取当前配置快照（读锁，Clone 出一份用于 spawn_blocking）
    pub fn config_snapshot(&self) -> AppConfig {
        self.config.read().expect("config RwLock poisoned").clone()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 开发构建（`bun run tauri dev` / cargo dev）：默认开启提炼载荷全文日志（与 sidecar 的 CHATTAKE_LOG_DISTILL_PAYLOAD=1 一致）。
    // 发布构建不设置；若需在本机关闭，启动前可 export CHATTAKE_LOG_DISTILL_PAYLOAD=0。
    #[cfg(debug_assertions)]
    if std::env::var_os("CHATTAKE_LOG_DISTILL_PAYLOAD").is_none() {
        std::env::set_var("CHATTAKE_LOG_DISTILL_PAYLOAD", "1");
    }

    // 默认 info 级别，可通过 RUST_LOG 环境变量覆盖（如 RUST_LOG=debug）
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = AppConfig::load(None).expect("配置加载失败");
    log::info!(
        "已加载 {} 个数据源，{} 个已启用，API 配置: {} 套",
        config.collector.sources.len(),
        config.enabled_sources().len(),
        config.distiller.profiles.len(),
    );

    let db = Database::open_default().expect("数据库初始化失败");

    // 启动时清理上次运行中残留的 analyzing 状态（中途退出导致的脏数据）
    if let Err(e) = db.reset_stale_analyzing() {
        log::error!("清理残留 analyzing 状态失败: {}", e);
    }
    if let Err(e) = db.interrupt_running_jobs() {
        log::error!("恢复中断任务状态失败: {}", e);
    }

    let context = tauri::generate_context!();

    let sidecar = SidecarManager::find_binary(context.package_info())
        .map(|path| Arc::new(SidecarManager::new(path)));

    if sidecar.is_none() {
        log::warn!("Sidecar 未就绪：提炼功能将不可用，直至构建或安装 chattake-sidecar");
    }

    let state = AppState {
        db: Arc::new(db),
        config: Arc::new(RwLock::new(config)),
        sidecar,
        job_lock: Arc::new(tokio::sync::Mutex::new(())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .setup(|app| {
            let state = app.state::<AppState>();
            if state.config_snapshot().sync.scan_on_startup {
                if let Err(error) = commands::jobs::start_sync_internal(
                    app.handle().clone(),
                    state.db.clone(),
                    state.config_snapshot(),
                    state.sidecar.clone(),
                    state.job_lock.clone(),
                    None,
                ) {
                    log::error!("启动扫描任务创建失败: {}", error);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::jobs::start_sync,
            commands::jobs::start_analysis,
            commands::jobs::list_jobs,
            commands::jobs::get_job,
            commands::jobs::cancel_job,
            commands::jobs::retry_job_item,
            commands::sessions::list_sessions,
            commands::sessions::count_sessions_by_filter_groups,
            commands::sessions::delete_sessions_by_filter_groups,
            commands::sessions::get_session,
            commands::sessions::get_session_messages,
            commands::cards::search_cards,
            commands::cards::list_cards,
            commands::cards::get_card,
            commands::cards::update_card,
            commands::cards::publish_card,
            commands::cards::list_tag_records,
            commands::cards::merge_tags,
            commands::export::export_card_markdown,
            commands::export::export_cards_markdown_dir,
            commands::export::export_all_cards_markdown_dir,
            commands::export::count_all_cards,
            commands::sidebar::get_session_groups,
            commands::sidebar::list_tags,
            commands::sidebar::list_tech_stack_counts,
            commands::sidebar::list_card_types,
            commands::config::get_config,
            commands::config::get_database_backup_path,
            commands::config::save_config,
            commands::config::list_provider_models,
            commands::config::test_provider,
            commands::mcp::get_mcp_info,
        ])
        .run(context)
        .expect("Tauri 应用启动失败");
}
