//! Grok 会话采集：读取 `~/.grok/sessions/<encoded-cwd>/<session-id>/` 下的摘要与聊天记录。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use super::agent_jsonl::visible_text;
use super::normalizer::{
    file_fingerprint, normalize_user_content, CollectionBatch, CollectionFailure,
    NormalizedMessage, NormalizedSession,
};

pub struct GrokCollector {
    scan_dirs: Vec<PathBuf>,
}

impl GrokCollector {
    pub fn new(scan_dirs: Vec<PathBuf>) -> Self {
        Self { scan_dirs }
    }

    pub fn collect_changed<F>(&self, unchanged: F) -> CollectionBatch
    where
        F: Fn(&Path, i64, i64) -> bool,
    {
        let mut batch = CollectionBatch::default();
        for root in &self.scan_dirs {
            if !root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if !path.is_file()
                    || path.file_name().and_then(|v| v.to_str()) != Some("chat_history.jsonl")
                {
                    continue;
                }
                batch.found += 1;
                let Some((mtime, size)) = session_fingerprint(path) else {
                    continue;
                };
                if unchanged(path, mtime, size) {
                    batch.skipped += 1;
                    batch
                        .skipped_paths
                        .push(path.to_string_lossy().into_owned());
                    continue;
                }
                match parse_grok_session(path, mtime, size) {
                    Ok(Some(session)) => batch.sessions.push(session),
                    Ok(None) => {
                        batch.skipped += 1;
                        batch
                            .skipped_paths
                            .push(path.to_string_lossy().into_owned());
                    }
                    Err(error) => batch.failures.push(CollectionFailure {
                        raw_path: path.to_string_lossy().into_owned(),
                        error: error.to_string(),
                    }),
                }
            }
        }
        batch
    }
}

fn session_fingerprint(history_path: &Path) -> Option<(i64, i64)> {
    let (mut mtime, mut size) = file_fingerprint(history_path)?;
    if let Some((summary_mtime, summary_size)) = history_path
        .parent()
        .and_then(|dir| file_fingerprint(&dir.join("summary.json")))
    {
        mtime = mtime.max(summary_mtime);
        size = size.saturating_add(summary_size);
    }
    Some((mtime, size))
}

fn parse_grok_session(
    history_path: &Path,
    mtime: i64,
    size: i64,
) -> Result<Option<NormalizedSession>, Box<dyn std::error::Error>> {
    let session_dir = history_path.parent().ok_or("Grok 会话路径缺少父目录")?;
    let summary: Value = serde_json::from_reader(File::open(session_dir.join("summary.json"))?)?;
    if summary.get("session_kind").and_then(Value::as_str) == Some("subagent") {
        return Ok(None);
    }
    let info = &summary["info"];
    let session_id = info
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| session_dir.file_name().and_then(|v| v.to_str()))
        .ok_or("Grok 会话缺少 id")?
        .to_string();
    let cwd = info.get("cwd").and_then(Value::as_str).map(str::to_string);

    // ponytail: Grok 尚无无持久化参数；CLI Provider 固定在 chattake-cli-* 临时目录运行，采集时跳过这些自生成会话。
    if cwd
        .as_deref()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with("chattake-cli-"))
    {
        return Ok(None);
    }

    let mut messages = Vec::new();
    let mut has_user_message = false;
    let mut lines = BufReader::new(File::open(history_path)?).lines().peekable();
    while let Some(line) = lines.next() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) if lines.peek().is_none() => {
                log::debug!("忽略活跃 Grok 会话末尾未完成行: {error}");
                continue;
            }
            Err(error) => return Err(format!("Grok JSONL 中间行损坏: {error}").into()),
        };
        let role = match value.get("type").and_then(Value::as_str) {
            Some("user") if value.get("synthetic_reason").is_none() => "user",
            Some("assistant") => "assistant",
            _ => continue,
        };
        let content = visible_text(value.get("content"));
        if content.is_empty() {
            continue;
        }
        let content = if role == "user" {
            let Some(content) = normalize_user_content(&content) else {
                continue;
            };
            has_user_message = true;
            content
        } else {
            content
        };
        let timestamp = value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .and_then(Value::as_str)
            .map(str::to_string);
        messages.push(NormalizedMessage {
            role: role.to_string(),
            content,
            timestamp,
            tokens_in: 0,
            tokens_out: 0,
        });
    }

    if !has_user_message {
        return Ok(None);
    }
    let fallback = DateTime::<Utc>::from_timestamp_millis(mtime)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    let project_name = cwd
        .as_deref()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .map(str::to_string);
    Ok(Some(NormalizedSession {
        source_id: "grok".to_string(),
        session_id,
        project_path: cwd,
        project_name,
        analysis_title: summary
            .get("generated_title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        messages,
        raw_path: history_path.to_string_lossy().into_owned(),
        raw_mtime_ms: Some(mtime),
        raw_size_bytes: Some(size),
        created_at: summary
            .get("created_at")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| fallback.clone()),
        updated_at: summary
            .get("updated_at")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(fallback),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collects_real_grok_messages_and_project_metadata() {
        let root = tempfile::tempdir().unwrap();
        let session_dir = root.path().join("%2Ftmp%2Fdemo").join("session-1");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("summary.json"),
            serde_json::json!({
                "info": { "id": "session-1", "cwd": "/tmp/demo" },
                "created_at": "2026-08-31T00:00:00Z",
                "updated_at": "2026-08-31T00:01:00Z",
                "generated_title": "修复构建"
            })
            .to_string(),
        )
        .unwrap();
        let history = [
            serde_json::json!({"type":"system","content":"secret"}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"project rules"}],"synthetic_reason":"project_instructions"}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"为什么失败？"}],"prompt_index":0}),
            serde_json::json!({"type":"assistant","content":"因为配置错误。","tool_calls":[]}),
            serde_json::json!({"type":"tool_result","content":"private output"}),
        ]
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        fs::write(session_dir.join("chat_history.jsonl"), history).unwrap();

        let batch =
            GrokCollector::new(vec![root.path().to_path_buf()]).collect_changed(|_, _, _| false);

        assert!(batch.failures.is_empty());
        assert_eq!(batch.found, 1);
        let session = &batch.sessions[0];
        assert_eq!(session.source_id, "grok");
        assert_eq!(session.session_id, "session-1");
        assert_eq!(session.project_path.as_deref(), Some("/tmp/demo"));
        assert_eq!(session.project_name.as_deref(), Some("demo"));
        assert_eq!(session.analysis_title.as_deref(), Some("修复构建"));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].content, "为什么失败？");
        assert_eq!(session.messages[1].content, "因为配置错误。");
    }

    #[test]
    fn skips_sessions_created_by_the_grok_provider() {
        let root = tempfile::tempdir().unwrap();
        let session_dir = root.path().join("project").join("session-1");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("summary.json"),
            serde_json::json!({
                "info": { "id": "session-1", "cwd": "/tmp/chattake-cli-abc123" }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            session_dir.join("chat_history.jsonl"),
            serde_json::json!({"type":"user","content":"提炼这段对话"}).to_string(),
        )
        .unwrap();

        let batch =
            GrokCollector::new(vec![root.path().to_path_buf()]).collect_changed(|_, _, _| false);

        assert!(batch.sessions.is_empty());
        assert_eq!(batch.skipped, 1);
    }

    #[test]
    fn skips_subagent_sessions() {
        let root = tempfile::tempdir().unwrap();
        let session_dir = root.path().join("project").join("subagent-1");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("summary.json"),
            serde_json::json!({
                "session_kind": "subagent",
                "info": { "id": "subagent-1", "cwd": "/tmp/demo" }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            session_dir.join("chat_history.jsonl"),
            serde_json::json!({"type":"user","content":"内部探索任务"}).to_string(),
        )
        .unwrap();

        let batch =
            GrokCollector::new(vec![root.path().to_path_buf()]).collect_changed(|_, _, _| false);

        assert!(batch.sessions.is_empty());
        assert_eq!(batch.skipped, 1);
    }

    #[test]
    #[ignore]
    fn collects_local_grok_data() {
        let root = dirs::home_dir().unwrap().join(".grok/sessions");
        let batch = GrokCollector::new(vec![root]).collect_changed(|_, _, _| false);
        assert!(batch.failures.is_empty(), "解析失败: {:?}", batch.failures);
        assert!(!batch.sessions.is_empty(), "本地应有 Grok 对话数据");
        println!(
            "Grok: found={}, sessions={}, skipped={}",
            batch.found,
            batch.sessions.len(),
            batch.skipped
        );
    }
}
