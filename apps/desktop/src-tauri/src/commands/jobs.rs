use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter, State};

use crate::collector::scheduler::CollectorScheduler;
use crate::config::AppConfig;
use crate::sidecar::SidecarManager;
use crate::storage::models::{Job, NewJobItem};
use crate::storage::Database;
use crate::AppState;

fn emit_job(app: &AppHandle, db: &Database, job_id: &str) {
    if let Ok(job) = db.get_job_snapshot(job_id) {
        let _ = app.emit("job://updated", job);
    }
}

fn spawn_job(
    app: AppHandle,
    db: Arc<Database>,
    config: AppConfig,
    sidecar: Option<Arc<SidecarManager>>,
    job_lock: Arc<tokio::sync::Mutex<()>>,
    job_id: String,
) {
    tauri::async_runtime::spawn(async move {
        let _guard = job_lock.lock().await;
        let Ok(job) = db.get_job(&job_id) else { return };
        if job.cancel_requested {
            let _ = db.cancel_queued_items(&job_id);
            let _ = db.finish_job(&job_id, "cancelled", None);
            emit_job(&app, &db, &job_id);
            return;
        }
        let phase = if job.kind == "sync" {
            "scanning"
        } else {
            "judging"
        };
        if db.mark_job_running(&job_id, phase).is_err() {
            return;
        }
        emit_job(&app, &db, &job_id);

        // 一个分析任务只初始化一次供应商；任务内所有条目共享同一 Sidecar 与配置快照。
        if job.kind == "analysis" {
            let init_result = sidecar
                .as_ref()
                .ok_or_else(|| "Sidecar 未就绪".to_string())
                .and_then(|sidecar| {
                    let params = config.sidecar_init_params(job.provider_profile_id.as_deref())?;
                    sidecar
                        .call_with_timeout::<serde_json::Value>(
                            "init",
                            params,
                            std::time::Duration::from_secs(30),
                        )
                        .map_err(|error| format!("Sidecar init 失败：{error}"))
                });
            if let Err(error) = init_result {
                for item in &job.items {
                    let _ = db.finish_item(&job_id, &item.id, "failed", "failed", 0, Some(&error));
                }
                let _ = db.finish_job(&job_id, "failed", Some(&error));
                emit_job(&app, &db, &job_id);
                return;
            }
        }

        let mut failed = false;
        for item in job.items {
            if db.job_cancel_requested(&job_id).unwrap_or(false) {
                break;
            }
            let item_phase = if job.kind == "sync" {
                "scanning"
            } else {
                "judging"
            };
            if db.mark_item_running(&item.id, item_phase).is_err() {
                failed = true;
                continue;
            }
            emit_job(&app, &db, &job_id);
            let started = Instant::now();

            let result: Result<(), String> = if job.kind == "sync" {
                let db = db.clone();
                let config = config.clone();
                let source_id = item.source_id.clone().unwrap_or_default();
                let progress_db = db.clone();
                let progress_app = app.clone();
                let progress_job_id = job_id.clone();
                let progress_source_id = source_id.clone();
                tokio::task::spawn_blocking(move || {
                    let result = CollectorScheduler::new(&config, &db)
                        .collect_source_with_progress(&source_id, |raw_path, phase, error| {
                            if let Ok(id) = progress_db.append_job_item(
                                &progress_job_id,
                                &NewJobItem {
                                    session_id: None,
                                    source_id: Some(&progress_source_id),
                                    raw_path: Some(raw_path),
                                },
                            ) {
                                let status = if error.is_some() {
                                    "failed"
                                } else {
                                    "succeeded"
                                };
                                let _ = progress_db.finish_item(
                                    &progress_job_id,
                                    &id,
                                    status,
                                    phase,
                                    0,
                                    error,
                                );
                                emit_job(&progress_app, &progress_db, &progress_job_id);
                            }
                        });
                    if result.failed > 0 {
                        Err(format!("{} 个文件同步失败", result.failed))
                    } else {
                        Ok(())
                    }
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|value| value)
            } else {
                let db = db.clone();
                let config = config.clone();
                let sidecar = sidecar.clone().ok_or_else(|| "Sidecar 未就绪".to_string());
                let session_id = item
                    .session_id
                    .clone()
                    .ok_or_else(|| "任务条目缺少会话".to_string());
                let profile_id = job.provider_profile_id.clone();
                match (sidecar, session_id) {
                    (Ok(sidecar), Ok(session_id)) => {
                        let phase_db = db.clone();
                        let phase_app = app.clone();
                        let phase_job_id = job_id.clone();
                        let phase_item_id = item.id.clone();
                        tokio::task::spawn_blocking(move || {
                            let on_phase = |phase: &str| {
                                let _ = phase_db.update_job_item_phase(
                                    &phase_job_id,
                                    &phase_item_id,
                                    phase,
                                );
                                emit_job(&phase_app, &phase_db, &phase_job_id);
                            };
                            super::sessions::run_distill_pipeline(
                                &db,
                                &config,
                                &sidecar,
                                &session_id,
                                &phase_job_id,
                                profile_id.as_deref(),
                                Some(&on_phase),
                            )
                            .map(|_| ())
                        })
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|value| value)
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            };

            let duration = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
            let cancelled = db.job_cancel_requested(&job_id).unwrap_or(false);
            match (result, cancelled) {
                (_, true) => {
                    let _ = db.finish_item(
                        &job_id,
                        &item.id,
                        "cancelled",
                        "cancelled",
                        duration,
                        Some("用户取消"),
                    );
                }
                (Ok(()), false) => {
                    let _ =
                        db.finish_item(&job_id, &item.id, "succeeded", "completed", duration, None);
                }
                (Err(error), false) => {
                    failed = true;
                    let _ = db.finish_item(
                        &job_id,
                        &item.id,
                        "failed",
                        "failed",
                        duration,
                        Some(&error),
                    );
                }
            }
            emit_job(&app, &db, &job_id);
        }

        if db.job_cancel_requested(&job_id).unwrap_or(false) {
            let _ = db.cancel_queued_items(&job_id);
            let _ = db.finish_job(&job_id, "cancelled", None);
        } else if failed {
            let _ = db.finish_job(&job_id, "failed", Some("部分条目执行失败"));
        } else {
            let _ = db.finish_job(&job_id, "succeeded", None);
        }
        emit_job(&app, &db, &job_id);
    });
}

pub(crate) fn start_sync_internal(
    app: AppHandle,
    db: Arc<Database>,
    config: AppConfig,
    sidecar: Option<Arc<SidecarManager>>,
    job_lock: Arc<tokio::sync::Mutex<()>>,
    scope: Option<&str>,
) -> Result<Job, String> {
    let sources = config
        .enabled_sources()
        .into_iter()
        .filter(|source| scope.map(|wanted| wanted == source.id).unwrap_or(true))
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Err("没有匹配的已启用数据源".into());
    }
    let items = sources
        .iter()
        .map(|source| NewJobItem {
            session_id: None,
            source_id: Some(source.id.as_str()),
            raw_path: None,
        })
        .collect::<Vec<_>>();
    let job = db
        .create_job("sync", "queued", None, &items)
        .map_err(|error| error.to_string())?;
    spawn_job(app, db, config, sidecar, job_lock, job.id.clone());
    Ok(job)
}

fn start_analysis_internal(
    app: AppHandle,
    db: Arc<Database>,
    config: AppConfig,
    sidecar: Option<Arc<SidecarManager>>,
    job_lock: Arc<tokio::sync::Mutex<()>>,
    session_ids: &[String],
    provider_profile_id: Option<&str>,
) -> Result<Job, String> {
    if sidecar.is_none() {
        return Err("Sidecar 未就绪".into());
    }
    if session_ids.is_empty() {
        return Err("请至少选择一个会话".into());
    }
    for id in session_ids {
        db.get_session(id).map_err(|error| error.to_string())?;
    }
    let profile = config.provider_profile(provider_profile_id)?;
    let target = if profile.kind == "cli" {
        profile.command.as_str()
    } else {
        profile.base_url.as_str()
    };
    let items = session_ids
        .iter()
        .map(|id| NewJobItem {
            session_id: Some(id),
            source_id: None,
            raw_path: None,
        })
        .collect::<Vec<_>>();
    let job = db
        .create_job(
            "analysis",
            "queued",
            Some((&profile.id, &profile.provider, target, &profile.model)),
            &items,
        )
        .map_err(|error| error.to_string())?;
    spawn_job(app, db, config, sidecar, job_lock, job.id.clone());
    Ok(job)
}

#[tauri::command]
pub async fn start_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<Job, String> {
    start_sync_internal(
        app,
        state.db.clone(),
        state.config_snapshot(),
        state.sidecar.clone(),
        state.job_lock.clone(),
        scope.as_deref(),
    )
}

#[tauri::command(rename_all = "camelCase")]
pub async fn start_analysis(
    app: AppHandle,
    state: State<'_, AppState>,
    session_ids: Vec<String>,
    provider_profile_id: Option<String>,
) -> Result<Job, String> {
    start_analysis_internal(
        app,
        state.db.clone(),
        state.config_snapshot(),
        state.sidecar.clone(),
        state.job_lock.clone(),
        &session_ids,
        provider_profile_id.as_deref(),
    )
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_jobs(state: State<'_, AppState>, active_only: bool) -> Result<Vec<Job>, String> {
    state
        .db
        .list_jobs(active_only)
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_job(state: State<'_, AppState>, job_id: String) -> Result<Job, String> {
    state.db.get_job(&job_id).map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn cancel_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Job, String> {
    state
        .db
        .request_job_cancel(&job_id)
        .map_err(|error| error.to_string())?;
    let job = state
        .db
        .get_job(&job_id)
        .map_err(|error| error.to_string())?;
    if job.kind == "analysis" && job.items.iter().any(|item| item.status == "running") {
        if let Some(sidecar) = state.sidecar.clone() {
            tokio::task::spawn_blocking(move || sidecar.stop())
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
        }
    }
    emit_job(&app, &state.db, &job_id);
    state.db.get_job(&job_id).map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn retry_job_item(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    item_id: String,
    provider_profile_id: Option<String>,
) -> Result<Job, String> {
    let old = state
        .db
        .get_job(&job_id)
        .map_err(|error| error.to_string())?;
    let item = old
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or("任务条目不存在")?;
    if !matches!(item.status.as_str(), "failed" | "cancelled" | "interrupted") {
        return Err("只能重试失败、取消或中断的条目".into());
    }
    if old.kind == "sync" {
        start_sync_internal(
            app,
            state.db.clone(),
            state.config_snapshot(),
            state.sidecar.clone(),
            state.job_lock.clone(),
            item.source_id.as_deref(),
        )
    } else {
        let session_id = item.session_id.clone().ok_or("任务条目缺少会话")?;
        let profile_id = provider_profile_id
            .as_deref()
            .or(old.provider_profile_id.as_deref());
        start_analysis_internal(
            app,
            state.db.clone(),
            state.config_snapshot(),
            state.sidecar.clone(),
            state.job_lock.clone(),
            &[session_id],
            profile_id,
        )
    }
}
