//! 配置读取、保存与供应商连通性检查。

use std::time::Duration;

use tauri::State;

use crate::config::{
    AppConfig, CollectorConfig, DistillerConfig, ProviderProfile, SourceConfig, SyncConfig,
};
use crate::AppState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigDto {
    pub distiller: DistillerConfigDto,
    pub collector: CollectorConfigDto,
    pub sync: SyncConfigDto,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistillerConfigDto {
    pub active_profile_id: String,
    pub max_concurrent_analyses: usize,
    pub profiles: Vec<ProviderProfileDto>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub command: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorConfigDto {
    pub sources: Vec<SourceConfigDto>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfigDto {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub scan_dirs: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfigDto {
    pub scan_on_startup: bool,
}

impl From<&AppConfig> for AppConfigDto {
    fn from(c: &AppConfig) -> Self {
        Self {
            distiller: DistillerConfigDto {
                active_profile_id: c.distiller.active_profile_id.clone(),
                max_concurrent_analyses: c.distiller.max_concurrent_analyses,
                profiles: c
                    .distiller
                    .profiles
                    .iter()
                    .map(ProviderProfileDto::from)
                    .collect(),
            },
            collector: CollectorConfigDto {
                sources: c
                    .collector
                    .sources
                    .iter()
                    .map(|s| SourceConfigDto {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        enabled: s.enabled,
                        scan_dirs: s.scan_dirs.clone(),
                    })
                    .collect(),
            },
            sync: SyncConfigDto {
                scan_on_startup: c.sync.scan_on_startup,
            },
        }
    }
}

impl From<&ProviderProfile> for ProviderProfileDto {
    fn from(p: &ProviderProfile) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            kind: p.kind.clone(),
            provider: p.provider.clone(),
            base_url: p.base_url.clone(),
            api_key: p.api_key.clone(),
            model: p.model.clone(),
            command: p.command.clone(),
            timeout_secs: p.timeout_secs,
        }
    }
}

impl From<ProviderProfileDto> for ProviderProfile {
    fn from(p: ProviderProfileDto) -> Self {
        Self {
            id: p.id,
            name: p.name,
            kind: p.kind,
            provider: p.provider,
            base_url: p.base_url,
            api_key: p.api_key,
            model: p.model,
            command: p.command,
            timeout_secs: p.timeout_secs,
        }
    }
}

impl From<AppConfigDto> for AppConfig {
    fn from(dto: AppConfigDto) -> Self {
        Self {
            distiller: DistillerConfig {
                active_profile_id: dto.distiller.active_profile_id,
                max_concurrent_analyses: dto.distiller.max_concurrent_analyses,
                profiles: dto
                    .distiller
                    .profiles
                    .into_iter()
                    .map(ProviderProfile::from)
                    .collect(),
                legacy_api: None,
            },
            collector: CollectorConfig {
                sources: dto
                    .collector
                    .sources
                    .into_iter()
                    .map(|s| SourceConfig {
                        id: s.id,
                        name: s.name,
                        enabled: s.enabled,
                        scan_dirs: s.scan_dirs,
                    })
                    .collect(),
            },
            sync: SyncConfig {
                scan_on_startup: dto.sync.scan_on_startup,
            },
        }
    }
}

fn profile_params(profile: ProviderProfileDto) -> Result<serde_json::Value, String> {
    let profile = ProviderProfile::from(profile);
    if profile.kind == "cli" {
        if profile.command.trim().is_empty() {
            return Err("请先填写 CLI 命令".to_string());
        }
        return Ok(serde_json::json!({
            "kind": "cli", "provider": profile.provider, "command": profile.command,
            "model": profile.model, "timeout_secs": profile.timeout_secs,
        }));
    }
    if profile.api_key.trim().is_empty() || profile.base_url.trim().is_empty() {
        return Err("请先填写 API Key 和 Base URL".to_string());
    }
    Ok(serde_json::json!({
        "kind": "api", "provider": profile.provider, "base_url": profile.base_url,
        "api_key": profile.api_key, "model": profile.model, "timeout_secs": profile.timeout_secs,
    }))
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfigDto, String> {
    Ok(AppConfigDto::from(&state.config_snapshot()))
}

#[tauri::command]
pub async fn get_database_backup_path(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    Ok(state
        .db
        .last_backup_path()
        .map(|path| path.display().to_string()))
}

#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: AppConfigDto) -> Result<(), String> {
    let new_config = AppConfig::from(config);
    new_config.validate().map_err(|e| e.to_string())?;
    new_config
        .save(&AppConfig::default_path())
        .map_err(|e| format!("配置保存失败: {e}"))?;
    state
        .analysis_runtime
        .set_limit(new_config.distiller.max_concurrent_analyses);
    *state
        .config
        .write()
        .map_err(|_| "配置锁异常，请重启应用".to_string())? = new_config;
    Ok(())
}

#[tauri::command]
pub async fn list_provider_models(
    state: State<'_, AppState>,
    profile: ProviderProfileDto,
) -> Result<Vec<String>, String> {
    let sidecar = state.sidecar.as_ref().ok_or("Sidecar 未就绪")?.clone();
    let timeout = Duration::from_secs(profile.timeout_secs.saturating_add(5));
    let params = profile_params(profile)?;
    tokio::task::spawn_blocking(move || sidecar.call_with_timeout("list_models", params, timeout))
        .await
        .map_err(|e| format!("模型列表任务失败：{e}"))?
        .map_err(|e| format!("模型列表加载失败：{e}"))
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    profile: ProviderProfileDto,
) -> Result<String, String> {
    let sidecar = state.sidecar.as_ref().ok_or("Sidecar 未就绪")?.clone();
    let timeout = Duration::from_secs(profile.timeout_secs.saturating_add(5));
    let params = profile_params(profile)?;
    tokio::task::spawn_blocking(move || sidecar.call_with_timeout("test_provider", params, timeout))
        .await
        .map_err(|e| format!("连接测试任务失败：{e}"))?
        .map_err(|e| format!("连接测试失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_dto_uses_camel_case() {
        let json = serde_json::to_value(ProviderProfileDto {
            id: "sf".into(),
            name: "硅基流动".into(),
            kind: "api".into(),
            provider: "siliconflow".into(),
            base_url: "https://api.siliconflow.cn/v1".into(),
            api_key: "secret".into(),
            model: "deepseek-ai/DeepSeek-V3".into(),
            command: String::new(),
            timeout_secs: 120,
        })
        .unwrap();
        assert_eq!(json["baseUrl"], "https://api.siliconflow.cn/v1");
        assert!(json.get("base_url").is_none());
    }
}
