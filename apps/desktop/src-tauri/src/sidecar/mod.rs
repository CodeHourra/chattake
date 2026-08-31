//! TS Sidecar 进程管理 —— spawn / stop / RPC 调用。
//!
//! ```text
//! Rust (SidecarManager)
//!   ├── spawn() → 启动 chattake-sidecar 进程
//!   ├── call()  → 通过 RpcClient 发送 JSON-RPC 请求
//!   └── stop()  → 终止进程
//! ```
//!
//! 解析顺序（后者为兜底）：
//! 1. 开发：仓库 `packages/sidecar/dist/chattake-sidecar`（tauri dev / cargo run）
//! 2. 安装包：`resource_dir/chattake-sidecar`（与 `tauri.conf.json` 的 `bundle.resources` 一致）
//! 3. 用户全局：`~/.chattake/bin/chattake-sidecar`

pub mod rpc;

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    client: Mutex<Option<Arc<RpcClient>>>,
    /// sidecar 二进制路径
    binary_path: PathBuf,
    /// 任务取消后禁止 RPC 自动重启该 worker。
    cancelled: AtomicBool,
    /// 独立分析 worker 退出后必须让当前条目失败，不能无 init 自动重启。
    restartable: bool,
    started: AtomicBool,
}

impl SidecarManager {
    pub fn new(binary_path: PathBuf) -> Self {
        Self::with_restart(binary_path, true)
    }

    fn with_restart(binary_path: PathBuf, restartable: bool) -> Self {
        Self {
            process: Mutex::new(None),
            client: Mutex::new(None),
            binary_path,
            cancelled: AtomicBool::new(false),
            restartable,
            started: AtomicBool::new(false),
        }
    }

    /// 为一个分析条目创建独立进程管理器，隔离超时、取消与 Provider 状态。
    pub fn worker(&self) -> Self {
        Self::with_restart(self.binary_path.clone(), false)
    }

    /// 查找 sidecar 二进制路径。
    ///
    /// `package_info` 来自 [`tauri::generate_context!`]，用于与 Tauri CLI 相同的 `resource_dir` 解析（跨平台）。
    pub fn find_binary(package_info: &PackageInfo) -> Option<PathBuf> {
        find_companion_binary(package_info, "chattake-sidecar", "sidecar")
    }

    /// 查找开发目录、安装包资源或用户目录中的伴随二进制。
    pub fn find_companion_binary(
        package_info: &PackageInfo,
        binary_name: &str,
        package_dir: &str,
    ) -> Option<PathBuf> {
        find_companion_binary(package_info, binary_name, package_dir)
    }
}

fn find_companion_binary(
    package_info: &PackageInfo,
    binary_name: &str,
    package_dir: &str,
) -> Option<PathBuf> {
    let env = Env::default();

    // 1) 开发模式：从 packages/sidecar/dist/ 加载
    // CARGO_MANIFEST_DIR = .../apps/desktop/src-tauri
    // parent×3: src-tauri → desktop → apps → chattake root
    // Bun `--compile` 在 Windows 上产出 `chattake-sidecar.exe`，与 Unix 无后缀名不同
    let file_name = if cfg!(target_os = "windows") {
        format!("{binary_name}.exe")
    } else {
        binary_name.to_string()
    };
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| {
            p.join("packages")
                .join(package_dir)
                .join("dist")
                .join(&file_name)
        });

    if let Some(ref path) = dev_path {
        if path.exists() {
            log::info!("使用开发模式伴随程序: {}", path.display());
            return Some(path.clone());
        }
    }

    // 2) 安装包内：macOS/Linux 为 `chattake-sidecar`，Windows 为 `chattake-sidecar.exe`（见 tauri.windows.conf.json）
    if let Ok(dir) = resource_dir(package_info, &env) {
        let bundled = dir.join(&file_name);
        if bundled.exists() {
            log::info!("使用安装包内 sidecar: {}", bundled.display());
            return Some(bundled);
        }
    }

    // 3) 全局安装位置（可选覆盖）
    if let Some(home) = dirs::home_dir() {
        let global_path = home.join(".chattake/bin").join(&file_name);
        if global_path.exists() {
            log::info!("使用全局 sidecar: {}", global_path.display());
            return Some(global_path);
        }
    }

    log::warn!("未找到伴随程序: {binary_name}");
    None
}

impl SidecarManager {
    /// 启动 sidecar 进程
    pub fn start(&self) -> Result<(), RpcError> {
        let mut proc_guard = self
            .process
            .lock()
            .map_err(|_| RpcError::Internal("process lock poisoned".into()))?;

        if self.cancelled.load(Ordering::Acquire) {
            return Err(RpcError::Internal("Sidecar 任务已取消".into()));
        }
        if proc_guard.is_some() {
            log::debug!("Sidecar 已在运行中");
            return Ok(());
        }
        if self.started.load(Ordering::Acquire) && !self.restartable {
            return Err(RpcError::Internal("Sidecar worker 已退出".into()));
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
        *client_guard = Some(Arc::new(rpc_client));
        self.started.store(true, Ordering::Release);

        log::info!("Sidecar 启动成功");
        Ok(())
    }

    /// 取消任务并永久停止当前 worker，避免并发竞态触发自动重启。
    pub fn cancel(&self) -> Result<(), RpcError> {
        self.cancelled.store(true, Ordering::Release);
        self.stop()
    }

    /// 停止 sidecar 进程
    pub fn stop(&self) -> Result<(), RpcError> {
        let mut proc_guard = self
            .process
            .lock()
            .map_err(|_| RpcError::Internal("process lock poisoned".into()))?;

        if let Some(mut child) = proc_guard.take() {
            log::info!("停止 sidecar 进程...");
            #[cfg(unix)]
            let terminated = Command::new("kill")
                .arg("-TERM")
                .arg(child.id().to_string())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            #[cfg(not(unix))]
            let terminated = false;
            if !terminated {
                let _ = child.kill();
            }
            if terminated {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
                while std::time::Instant::now() < deadline {
                    if child.try_wait().ok().flatten().is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                }
            }
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

        let client = self
            .client
            .lock()
            .map_err(|_| RpcError::Internal("client lock poisoned".into()))?
            .as_ref()
            .cloned()
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

        let client = self
            .client
            .lock()
            .map_err(|_| RpcError::Internal("client lock poisoned".into()))?
            .as_ref()
            .cloned()
            .ok_or_else(|| RpcError::Internal("RPC 客户端未就绪".into()))?;
        let result = client.call_with_timeout(method, params, timeout);
        if matches!(result, Err(RpcError::Timeout(_))) {
            let _ = self.stop();
        }
        result
    }
}

impl Drop for SidecarManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_worker_cannot_restart() {
        let worker = SidecarManager::new(PathBuf::from("missing-sidecar"));
        worker.cancel().unwrap();
        assert!(
            matches!(worker.start(), Err(RpcError::Internal(message)) if message.contains("已取消"))
        );
    }

    #[test]
    fn analysis_worker_does_not_restart_without_init() {
        let root = SidecarManager::new(PathBuf::from("missing-sidecar"));
        let worker = root.worker();
        worker.started.store(true, Ordering::Release);
        assert!(
            matches!(worker.start(), Err(RpcError::Internal(message)) if message.contains("已退出"))
        );
    }
}
