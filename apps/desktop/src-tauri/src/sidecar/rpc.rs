//! JSON-RPC 2.0 客户端 —— 通过 stdin/stdout 与 TS Sidecar 通信。
//!
//! 协议：每行一个 JSON 请求（写 stdin），每行一个 JSON 响应（读 stdout）。
//! 日志由 sidecar 输出到 stderr，不影响通信通道。
//!
//! 注意：call() 是同步阻塞的，Tauri command 层需用 spawn_blocking 包装。

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

/// 默认 RPC 调用超时（秒），LLM 提炼可能需要较长时间
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// JSON-RPC 2.0 客户端，持有 sidecar 进程的 stdin/stdout
pub struct RpcClient {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    request_id: AtomicU64,
    /// RPC 调用默认超时时间
    timeout: Duration,
}

impl RpcClient {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            request_id: AtomicU64::new(1),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    /// 发送 JSON-RPC 请求并等待响应（使用默认超时）。
    ///
    /// 注意：此方法是同步阻塞的（通过内部线程实现超时）。
    /// 在 Tauri async command 中调用时需用 `tokio::task::spawn_blocking` 包装。
    pub fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T, RpcError> {
        self.call_with_timeout(method, params, self.timeout)
    }

    /// 发送 JSON-RPC 请求并等待响应（使用自定义超时）。
    ///
    /// 不同的 RPC 方法可能需要不同的超时时间：
    /// - `init` → 30 秒
    /// - `judge_value` → 60 秒
    /// - `distill_full` → 300 秒（大对话可能需要更长时间）
    pub fn call_with_timeout<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<T, RpcError> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let request_str =
            serde_json::to_string(&request).map_err(|e| RpcError::Serialize(e.to_string()))?;

        log::debug!(
            "RPC 请求: method={}, id={}, timeout={}s",
            method,
            id,
            timeout.as_secs()
        );

        // 写 stdin
        {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|_| RpcError::Internal("stdin lock poisoned".into()))?;
            writeln!(stdin, "{}", request_str).map_err(|e| RpcError::Io(e.to_string()))?;
            stdin.flush().map_err(|e| RpcError::Io(e.to_string()))?;
        }

        // 读 stdout，通过独立线程 + channel 实现超时控制
        let response_str = self.read_response_with_timeout(timeout)?;

        if response_str.trim().is_empty() {
            return Err(RpcError::Io("Sidecar 返回空响应（进程可能已退出）".into()));
        }

        // 解析响应
        let response: Value = serde_json::from_str(response_str.trim()).map_err(|e| {
            RpcError::Deserialize(format!(
                "响应解析失败: {} - {}",
                e,
                &response_str[..response_str.len().min(200)]
            ))
        })?;

        // 检查 error 字段
        if let Some(err) = response.get("error") {
            let code = err["code"].as_i64().unwrap_or(-1);
            let message = err["message"].as_str().unwrap_or("unknown error");
            return Err(RpcError::Remote {
                code: code as i32,
                message: message.to_string(),
            });
        }

        // 提取 result 字段并反序列化为目标类型
        let result = response
            .get("result")
            .ok_or_else(|| RpcError::Deserialize("响应缺少 result 字段".into()))?;

        serde_json::from_value(result.clone())
            .map_err(|e| RpcError::Deserialize(format!("result 反序列化失败: {}", e)))
    }

    /// 带超时的 stdout 读取。
    ///
    /// 实现方式：scoped thread 内阻塞读取 + channel recv_timeout。
    /// `std::thread::scope` 保证子线程在作用域内结束，无需 unsafe。
    fn read_response_with_timeout(&self, timeout: Duration) -> Result<String, RpcError> {
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| RpcError::Internal("stdout lock poisoned".into()))?;

        let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();

        // scoped thread 可以安全借用栈上的 MutexGuard
        std::thread::scope(|s| {
            let reader = &mut *stdout;
            s.spawn(move || {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(_) => {
                        let _ = tx.send(Ok(line));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            });

            // 在 scope 内等待结果（带超时）
            // scope 结束时会自动 join 子线程
            match rx.recv_timeout(timeout) {
                Ok(Ok(line)) => Ok(line),
                Ok(Err(e)) => Err(RpcError::Io(e)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    Err(RpcError::Timeout(timeout.as_secs()))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    Err(RpcError::Io("读取线程异常退出".into()))
                }
            }
        })
    }
}

/// RPC 调用错误
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("IO 错误: {0}")]
    Io(String),
    #[error("RPC 调用超时（{0}秒），LLM 可能无响应")]
    Timeout(u64),
    #[error("序列化错误: {0}")]
    Serialize(String),
    #[error("反序列化错误: {0}")]
    Deserialize(String),
    #[error("远程错误 (code={code}): {message}")]
    Remote { code: i32, message: String },
    #[error("内部错误: {0}")]
    Internal(String),
}
