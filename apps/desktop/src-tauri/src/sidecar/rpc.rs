//! JSON-RPC 2.0 客户端 —— 通过 stdin/stdout 与 TS Sidecar 通信。
//!
//! 协议：每行一个 JSON 请求（写 stdin），每行一个 JSON 响应（读 stdout）。
//! 日志由 sidecar 输出到 stderr，不影响通信通道。
//!
//! 注意：call() 是同步阻塞的，Tauri command 层需用 spawn_blocking 包装。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

/// 默认 RPC 调用超时（秒），LLM 提炼可能需要较长时间
const DEFAULT_TIMEOUT_SECS: u64 = 120;
type PendingResponses = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<String, String>>>>>;

/// JSON-RPC 2.0 客户端，持有 sidecar 进程的 stdin/stdout
pub struct RpcClient {
    stdin: Mutex<ChildStdin>,
    pending: PendingResponses,
    request_id: AtomicU64,
    /// RPC 调用默认超时时间
    timeout: Duration,
}

impl RpcClient {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        let pending = PendingResponses::default();
        let reader_pending = pending.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        fail_pending(&reader_pending, "Sidecar stdout 已关闭");
                        break;
                    }
                    Ok(_) => {
                        if let Err(error) = dispatch_response(&reader_pending, line) {
                            log::warn!("Sidecar 响应分发失败: {error}");
                        }
                    }
                    Err(error) => {
                        fail_pending(&reader_pending, &error.to_string());
                        break;
                    }
                }
            }
        });
        Self {
            stdin: Mutex::new(stdin),
            pending,
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

        let (response_tx, response_rx) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| RpcError::Internal("pending response lock poisoned".into()))?
            .insert(id, response_tx);

        // 写 stdin
        let write_result = (|| -> Result<(), RpcError> {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|_| RpcError::Internal("stdin lock poisoned".into()))?;
            writeln!(stdin, "{}", request_str).map_err(|e| RpcError::Io(e.to_string()))?;
            stdin.flush().map_err(|e| RpcError::Io(e.to_string()))
        })();
        if let Err(error) = write_result {
            self.remove_pending(id);
            return Err(error);
        }

        let response_str = match response_rx.recv_timeout(timeout) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => return Err(RpcError::Io(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.remove_pending(id);
                return Err(RpcError::Timeout(timeout.as_secs()));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(RpcError::Io("读取线程异常退出".into()));
            }
        };
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

    fn remove_pending(&self, id: u64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
    }
}

fn dispatch_response(pending: &PendingResponses, line: String) -> Result<(), String> {
    let response: Value = serde_json::from_str(line.trim()).map_err(|error| error.to_string())?;
    let id = response
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "响应缺少数字 id".to_string())?;
    let sender = pending
        .lock()
        .map_err(|_| "pending response lock poisoned".to_string())?
        .remove(&id)
        .ok_or_else(|| format!("响应 id={id} 已超时或不存在"))?;
    sender
        .send(Ok(line))
        .map_err(|_| format!("响应 id={id} 的接收端已关闭"))
}

fn fail_pending(pending: &PendingResponses, error: &str) {
    let senders = pending
        .lock()
        .map(|mut pending| {
            pending
                .drain()
                .map(|(_, sender)| sender)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for sender in senders {
        let _ = sender.send(Err(error.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_out_of_order_responses_to_matching_calls() {
        let pending = PendingResponses::default();
        let (first_tx, first_rx) = mpsc::channel();
        let (second_tx, second_rx) = mpsc::channel();
        pending.lock().unwrap().insert(1, first_tx);
        pending.lock().unwrap().insert(2, second_tx);

        dispatch_response(&pending, r#"{"jsonrpc":"2.0","id":2,"result":"b"}"#.into()).unwrap();
        dispatch_response(&pending, r#"{"jsonrpc":"2.0","id":1,"result":"a"}"#.into()).unwrap();

        assert!(second_rx.recv().unwrap().unwrap().contains("\"b\""));
        assert!(first_rx.recv().unwrap().unwrap().contains("\"a\""));
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
