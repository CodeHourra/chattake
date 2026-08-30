use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{sidecar::SidecarManager, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInfo {
    available: bool,
    binary_path: Option<String>,
    database_path: String,
    config_snippet: Option<String>,
}

#[tauri::command]
pub fn get_mcp_info(app: AppHandle, state: State<'_, AppState>) -> Result<McpInfo, String> {
    let database_path = state.db.path().to_string_lossy().into_owned();
    let binary =
        SidecarManager::find_companion_binary(app.package_info(), "chattake-mcp", "mcp-server");
    let config_snippet = binary.as_ref().map(|path| {
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                "chattake": {
                    "command": path.to_string_lossy(),
                    "env": { "CHATTAKE_DB": database_path }
                }
            }
        }))
        .expect("MCP 配置序列化失败")
    });
    Ok(McpInfo {
        available: binary.is_some(),
        binary_path: binary.map(|path| path.to_string_lossy().into_owned()),
        database_path,
        config_snippet,
    })
}
