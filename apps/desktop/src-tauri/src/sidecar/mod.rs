//! TS Sidecar 进程管理 —— spawn / stop / RPC 调用。
//!
//! ```text
//! Rust (SidecarManager)
//!   ├── spawn() → 启动 xunji-sidecar 进程
//!   ├── call()  → 通过 RpcClient 发送 JSON-RPC 请求
//!   └── stop()  → 终止进程
//! ```
//!
//! 解析顺序（后者为兜底）：
//! 1. 开发：仓库 `packages/sidecar/dist/xunji-sidecar`（tauri dev / cargo run）
//! 2. 安装包：`resource_dir/xunji-sidecar`（与 `tauri.conf.json` 的 `bundle.resources` 一致）
//! 3. 用户全局：`~/.xunji/bin/xunji-sidecar`

pub mod rpc;

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use rpc::{RpcClient, RpcError};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::utils::platform::resource_dir;
use tauri::utils::{Env, PackageInfo};

/// Sidecar 进程管理器
pub struct SidecarManager {
    /// 运行中的子进程
    process: Mutex<Option<Child>>,
    /// RPC 客户端（进程启动后创建）
    client: Mutex<Option<RpcClient>>,
    /// sidecar 二进制路径
    binary_path: PathBuf,
}

impl SidecarManager {
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            process: Mutex::new(None),
            client: Mutex::new(None),
            binary_path,
        }
    }

    /// 查找 sidecar 二进制路径。
    ///
    /// `package_info` 来自 [`tauri::generate_context!`]，用于与 Tauri CLI 相同的 `resource_dir` 解析（跨平台）。
    pub fn find_binary(package_info: &PackageInfo) -> Option<PathBuf> {
        let env = Env::default();

        // 1) 开发模式：从 packages/sidecar/dist/ 加载
        // CARGO_MANIFEST_DIR = .../apps/desktop/src-tauri
        // parent×3: src-tauri → desktop → apps → xunji root
        // Bun `--compile` 在 Windows 上产出 `xunji-sidecar.exe`，与 Unix 无后缀名不同
        let dev_sidecar_name = if cfg!(target_os = "windows") {
            "xunji-sidecar.exe"
        } else {
            "xunji-sidecar"
        };
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("packages/sidecar/dist").join(dev_sidecar_name));

        if let Some(ref path) = dev_path {
            if path.exists() {
                log::info!("使用开发模式 sidecar: {}", path.display());
                return Some(path.clone());
            }
        }

        // 2) 安装包内：macOS/Linux 为 `xunji-sidecar`，Windows 为 `xunji-sidecar.exe`（见 tauri.windows.conf.json）
        if let Ok(dir) = resource_dir(package_info, &env) {
            let bundled = if cfg!(target_os = "windows") {
                dir.join("xunji-sidecar.exe")
            } else {
                dir.join("xunji-sidecar")
            };
            if bundled.exists() {
                log::info!("使用安装包内 sidecar: {}", bundled.display());
                return Some(bundled);
            }
        }

        // 3) 全局安装位置（可选覆盖）
        if let Some(home) = dirs::home_dir() {
            let global_path = if cfg!(target_os = "windows") {
                home.join(".xunji/bin/xunji-sidecar.exe")
            } else {
                home.join(".xunji/bin/xunji-sidecar")
            };
            if global_path.exists() {
                log::info!("使用全局 sidecar: {}", global_path.display());
                return Some(global_path);
            }
        }

        log::warn!("未找到 sidecar 二进制文件");
        None
    }

    /// 启动 sidecar 进程
    pub fn start(&self) -> Result<(), RpcError> {
        let mut proc_guard = self
            .process
            .lock()
            .map_err(|_| RpcError::Internal("process lock poisoned".into()))?;

        if proc_guard.is_some() {
            log::debug!("Sidecar 已在运行中");
            return Ok(());
        }

        log::info!("启动 sidecar: {}", self.binary_path.display());
        let mut child = Command::new(&self.binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // sidecar 日志直接输出到父进程 stderr
            .spawn()
            .map_err(|e| RpcError::Io(format!("启动 sidecar 失败: {}", e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RpcError::Internal("无法获取 sidecar stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RpcError::Internal("无法获取 sidecar stdout".into()))?;

        let rpc_client = RpcClient::new(stdin, stdout);

        // 先存进程再存 client
        *proc_guard = Some(child);
        let mut client_guard = self
            .client
            .lock()
            .map_err(|_| RpcError::Internal("client lock poisoned".into()))?;
        *client_guard = Some(rpc_client);

        log::info!("Sidecar 启动成功");
        Ok(())
    }

    /// 停止 sidecar 进程
    pub fn stop(&self) -> Result<(), RpcError> {
        let mut proc_guard = self
            .process
            .lock()
            .map_err(|_| RpcError::Internal("process lock poisoned".into()))?;

        if let Some(mut child) = proc_guard.take() {
            log::info!("停止 sidecar 进程...");
            let _ = child.kill();
            let _ = child.wait();
        }

        let mut client_guard = self
            .client
            .lock()
            .map_err(|_| RpcError::Internal("client lock poisoned".into()))?;
        *client_guard = None;

        Ok(())
    }

    /// 检查 sidecar 进程是否存活（通过 try_wait 探测真实状态）。
    /// 如果进程已退出，自动清理内部状态。
    pub fn is_running(&self) -> bool {
        let mut guard = match self.process.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };

        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                // 进程仍在运行
                Ok(None) => return true,
                // 进程已退出
                Ok(Some(status)) => {
                    log::warn!("Sidecar 进程已退出: {:?}", status);
                    // 清理状态，允许后续自动重启
                    *guard = None;
                    if let Ok(mut client) = self.client.lock() {
                        *client = None;
                    }
                    return false;
                }
                Err(e) => {
                    log::error!("检测 sidecar 状态失败: {}", e);
                    return false;
                }
            }
        }

        false
    }

    /// 发送 JSON-RPC 请求到 sidecar（使用默认超时）。如果进程未启动或已崩溃则自动重启。
    ///
    /// 注意：call() 内部是同步阻塞的，在 Tauri async command 中调用时
    /// 需用 `tokio::task::spawn_blocking` 包装，避免阻塞 tokio 线程池。
    pub fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T, RpcError> {
        if !self.is_running() {
            self.start()?;
        }

        let client_guard = self
            .client
            .lock()
            .map_err(|_| RpcError::Internal("client lock poisoned".into()))?;

        let client = client_guard
            .as_ref()
            .ok_or_else(|| RpcError::Internal("RPC 客户端未就绪".into()))?;

        client.call(method, params)
    }

    /// 发送 JSON-RPC 请求到 sidecar（使用自定义超时）。如果进程未启动或已崩溃则自动重启。
    pub fn call_with_timeout<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<T, RpcError> {
        if !self.is_running() {
            self.start()?;
        }

        let client_guard = self
            .client
            .lock()
            .map_err(|_| RpcError::Internal("client lock poisoned".into()))?;

        let client = client_guard
            .as_ref()
            .ok_or_else(|| RpcError::Internal("RPC 客户端未就绪".into()))?;

        client.call_with_timeout(method, params, timeout)
    }
}

impl Drop for SidecarManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
