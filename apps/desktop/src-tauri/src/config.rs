//! 应用配置管理：四个内置采集源 + 多 API 配置、单配置激活。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub collector: CollectorConfig,
    #[serde(default)]
    pub distiller: DistillerConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorConfig {
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub scan_dirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillerConfig {
    #[serde(default)]
    pub active_profile_id: String,
    #[serde(default)]
    pub profiles: Vec<ApiProfile>,
    /// 仅用于把 v0.1 的单 API 配置迁移成 profile；落盘时移除。
    #[serde(default, rename = "api", skip_serializing)]
    pub legacy_api: Option<LegacyApiConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyApiConfig {
    pub provider: String,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

impl Default for DistillerConfig {
    fn default() -> Self {
        Self {
            active_profile_id: "default".to_string(),
            profiles: vec![ApiProfile {
                id: "default".to_string(),
                name: "OpenAI".to_string(),
                provider: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: String::new(),
                model: String::new(),
                timeout_secs: default_timeout(),
            }],
            legacy_api: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProfile {
    /// 稳定本机标识；重命名配置不会改变该值。
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default = "default_true")]
    pub scan_on_startup: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            scan_on_startup: true,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_timeout() -> u64 {
    120
}

fn default_codebuddy_scan_dirs() -> Vec<String> {
    vec![
        "~/Library/Application Support/CodeBuddyExtension/Data".to_string(),
        "~/.local/share/CodeBuddyExtension".to_string(),
    ]
}

fn builtin_sources() -> Vec<SourceConfig> {
    vec![
        SourceConfig {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            enabled: true,
            scan_dirs: vec!["~/.claude".to_string()],
        },
        SourceConfig {
            id: "cursor".to_string(),
            name: "Cursor".to_string(),
            enabled: true,
            scan_dirs: vec!["~/Library/Application Support/Cursor".to_string()],
        },
        SourceConfig {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            enabled: true,
            scan_dirs: vec![
                "~/.codex/sessions".to_string(),
                "~/.codex/archived_sessions".to_string(),
            ],
        },
        SourceConfig {
            id: "codebuddy".to_string(),
            name: "CodeBuddy".to_string(),
            enabled: false,
            scan_dirs: default_codebuddy_scan_dirs(),
        },
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            collector: CollectorConfig {
                sources: builtin_sources(),
            },
            distiller: DistillerConfig::default(),
            sync: SyncConfig::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML 解析错误: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("TOML 序列化错误: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("配置无效: {0}")]
    Invalid(String),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

impl AppConfig {
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .expect("无法获取用户主目录")
            .join(".xunji/config.toml")
    }

    pub fn load(path: Option<&Path>) -> ConfigResult<Self> {
        let config_path = path
            .map(Path::to_path_buf)
            .or_else(|| std::env::var("XUNJI_CONFIG").ok().map(PathBuf::from))
            .unwrap_or_else(Self::default_path);

        if !config_path.exists() {
            let config = Self::default();
            config.save(&config_path)?;
            return Ok(config);
        }

        let content = fs::read_to_string(&config_path)?;
        let mut config: AppConfig = toml::from_str(&content)?;
        let changed = config.normalize();
        config.validate()?;
        if changed {
            config.save(&config_path)?;
        }
        Ok(config)
    }

    /// 只保留 v0.2 支持的四个内置来源；不保留 CLI/internal 兼容入口。
    fn normalize(&mut self) -> bool {
        let mut changed = false;
        let old = std::mem::take(&mut self.collector.sources);
        let mut normalized = Vec::new();

        for default in builtin_sources() {
            if let Some(mut existing) = old.iter().find(|s| s.id == default.id).cloned() {
                let before = existing.scan_dirs.len();
                existing
                    .scan_dirs
                    .retain(|p| !p.to_ascii_lowercase().contains("-internal"));
                if existing.scan_dirs.is_empty() {
                    existing.scan_dirs = default.scan_dirs.clone();
                }
                changed |= before != existing.scan_dirs.len();
                existing.name = default.name;
                normalized.push(existing);
            } else {
                normalized.push(default);
                changed = true;
            }
        }
        changed |= normalized.len() != old.len();
        self.collector.sources = normalized;

        if self.distiller.profiles.is_empty() {
            self.distiller = if let Some(api) = self.distiller.legacy_api.take() {
                DistillerConfig {
                    active_profile_id: "migrated-api".to_string(),
                    profiles: vec![ApiProfile {
                        id: "migrated-api".to_string(),
                        name: "已迁移 API".to_string(),
                        provider: api.provider,
                        base_url: api
                            .base_url
                            .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                        api_key: api.api_key,
                        model: api.model,
                        timeout_secs: api.timeout_secs,
                    }],
                    legacy_api: None,
                }
            } else {
                DistillerConfig::default()
            };
            changed = true;
        } else if !self
            .distiller
            .profiles
            .iter()
            .any(|p| p.id == self.distiller.active_profile_id)
        {
            self.distiller.active_profile_id = self.distiller.profiles[0].id.clone();
            changed = true;
        }
        changed
    }

    pub fn validate(&self) -> ConfigResult<()> {
        if self.distiller.profiles.is_empty() {
            return Err(ConfigError::Invalid("至少需要一套 API 配置".to_string()));
        }
        let mut ids = HashSet::new();
        for profile in &self.distiller.profiles {
            if profile.id.trim().is_empty() || !ids.insert(profile.id.as_str()) {
                return Err(ConfigError::Invalid(
                    "API 配置 id 不能为空或重复".to_string(),
                ));
            }
            if profile.name.trim().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "API 配置 {} 缺少显示名称",
                    profile.id
                )));
            }
            if !(10..=600).contains(&profile.timeout_secs) {
                return Err(ConfigError::Invalid(format!(
                    "API 配置 {} 的超时必须在 10–600 秒之间",
                    profile.name
                )));
            }
        }
        if !ids.contains(self.distiller.active_profile_id.as_str()) {
            return Err(ConfigError::Invalid(
                "当前激活的 API 配置不存在".to_string(),
            ));
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> ConfigResult<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn enabled_sources(&self) -> Vec<&SourceConfig> {
        self.collector
            .sources
            .iter()
            .filter(|s| s.enabled)
            .collect()
    }

    pub fn source_display_name(&self, source_id: &str) -> Option<String> {
        self.collector
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .map(|s| s.name.clone())
    }

    pub fn api_profile(&self, profile_id: Option<&str>) -> Result<&ApiProfile, String> {
        let id = profile_id.unwrap_or(&self.distiller.active_profile_id);
        self.distiller
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("未找到 API 配置：{}", id))
    }

    pub fn sidecar_init_params(&self, profile_id: Option<&str>) -> Result<Value, String> {
        let profile = self.api_profile(profile_id)?;
        if profile.api_key.trim().is_empty() {
            return Err(format!("API 配置「{}」尚未填写 API Key", profile.name));
        }
        if profile.base_url.trim().is_empty() || profile.model.trim().is_empty() {
            return Err(format!("API 配置「{}」缺少 Base URL 或模型", profile.name));
        }
        Ok(serde_json::json!({
            "provider": profile.provider,
            "base_url": profile.base_url,
            "api_key": profile.api_key,
            "model": profile.model,
            "timeout_secs": profile.timeout_secs,
        }))
    }
}

impl SourceConfig {
    pub fn resolved_scan_dirs(&self) -> Vec<PathBuf> {
        let home = dirs::home_dir().expect("无法获取用户主目录");
        self.scan_dirs
            .iter()
            .map(|dir| {
                if let Some(rest) = dir.strip_prefix("~/") {
                    home.join(rest)
                } else if dir == "~" {
                    home.clone()
                } else {
                    PathBuf::from(dir)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_four_sources_and_one_profile() {
        let config = AppConfig::default();
        let ids: Vec<&str> = config
            .collector
            .sources
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, vec!["claude-code", "cursor", "codex", "codebuddy"]);
        assert_eq!(config.distiller.profiles.len(), 1);
        assert_eq!(config.distiller.active_profile_id, "default");
        assert!(config.sync.scan_on_startup);
    }

    #[test]
    fn profile_roundtrip_and_selection() {
        let mut config = AppConfig::default();
        config.distiller.profiles.push(ApiProfile {
            id: "siliconflow-main".to_string(),
            name: "硅基流动".to_string(),
            provider: "siliconflow".to_string(),
            base_url: "https://api.siliconflow.cn/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "deepseek-ai/DeepSeek-V3".to_string(),
            timeout_secs: 90,
        });
        config.distiller.active_profile_id = "siliconflow-main".to_string();
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&text).unwrap();
        let selected = parsed.api_profile(None).unwrap();
        assert_eq!(selected.provider, "siliconflow");
        assert_eq!(selected.timeout_secs, 90);
    }

    #[test]
    fn rejects_duplicate_profile_ids() {
        let mut config = AppConfig::default();
        config
            .distiller
            .profiles
            .push(config.distiller.profiles[0].clone());
        assert!(config.validate().is_err());
    }

    #[test]
    fn migrates_v01_single_api_without_cli_compatibility() {
        let text = r#"
[collector]
sources = []
[distiller]
mode = "api"
[distiller.api]
provider = "deepseek"
base_url = "https://api.deepseek.com/v1"
api_key = "sk-old"
model = "deepseek-chat"
timeout_secs = 60
[sync]
mode = "manual"
interval_secs = 300
"#;
        let mut config: AppConfig = toml::from_str(text).unwrap();
        assert!(config.normalize());
        assert_eq!(config.distiller.active_profile_id, "migrated-api");
        assert_eq!(config.api_profile(None).unwrap().model, "deepseek-chat");
        assert!(!toml::to_string(&config)
            .unwrap()
            .contains("[distiller.api]"));
    }

    #[test]
    fn expands_home_directory() {
        let source = SourceConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            enabled: true,
            scan_dirs: vec!["~/.codex/sessions".to_string(), "/tmp/x".to_string()],
        };
        assert_eq!(
            source.resolved_scan_dirs()[0],
            dirs::home_dir().unwrap().join(".codex/sessions")
        );
        assert_eq!(source.resolved_scan_dirs()[1], PathBuf::from("/tmp/x"));
    }
}
