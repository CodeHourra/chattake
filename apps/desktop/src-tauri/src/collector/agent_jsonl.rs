//! OMP / Pi 的 JSONL 会话解析器。两者使用相同的 pi-agent 会话格式，但作为独立来源入库。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use super::normalizer::{
    file_fingerprint, normalize_user_content, CollectionBatch, CollectionFailure,
    NormalizedMessage, NormalizedSession,
};

pub struct AgentJsonlCollector {
    source_id: &'static str,
    scan_dirs: Vec<PathBuf>,
}

impl AgentJsonlCollector {
    pub fn new(source_id: &'static str, scan_dirs: Vec<PathBuf>) -> Self {
        Self {
            source_id,
            scan_dirs,
        }
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
                if !path.is_file() || path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                    continue;
                }
                batch.found += 1;
                let Some((mtime, size)) = file_fingerprint(path) else {
                    continue;
                };
                if unchanged(path, mtime, size) {
                    batch.skipped += 1;
                    batch
                        .skipped_paths
                        .push(path.to_string_lossy().into_owned());
                    continue;
                }
                match parse_agent_jsonl(path, self.source_id, mtime, size) {
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

fn parse_agent_jsonl(
    path: &Path,
    source_id: &str,
    mtime: i64,
    size: i64,
) -> Result<Option<NormalizedSession>, Box<dyn std::error::Error>> {
    let mut session_id = None;
    let mut cwd = None;
    let mut title = None;
    let mut messages = Vec::new();
    let mut created_at = None;
    let mut updated_at = None;

    let mut lines = BufReader::new(File::open(path)?).lines().peekable();
    while let Some(line) = lines.next() {
        let value: Value = match serde_json::from_str(&line?) {
            Ok(value) => value,
            Err(error) if lines.peek().is_none() => {
                log::debug!("忽略活跃 {source_id} 会话末尾未完成行: {error}");
                continue;
            }
            Err(error) => return Err(format!("JSONL 中间行损坏: {error}").into()),
        };
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match value.get("type").and_then(Value::as_str) {
            Some("session") => {
                session_id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(session_id);
                cwd = value
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(cwd);
                created_at = timestamp.clone().or(created_at);
            }
            Some("title") | Some("title_change") => {
                title = value
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(title);
            }
            Some("message") => {
                let message = &value["message"];
                let role = match message.get("role").and_then(Value::as_str) {
                    Some("user") => "user",
                    Some("assistant") => "assistant",
                    _ => continue,
                };
                let content = visible_text(message.get("content"));
                if content.is_empty() {
                    continue;
                }
                let content = if role == "user" {
                    match normalize_user_content(&content) {
                        Some(content) => content,
                        None => continue,
                    }
                } else {
                    content
                };
                let timestamp = message
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(timestamp);
                if created_at.is_none() {
                    created_at = timestamp.clone();
                }
                updated_at = timestamp.clone().or(updated_at);
                messages.push(NormalizedMessage {
                    role: role.to_string(),
                    content,
                    timestamp,
                    tokens_in: 0,
                    tokens_out: 0,
                });
            }
            _ => {}
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }
    let session_id = session_id
        .or_else(|| path.file_stem().and_then(|v| v.to_str()).map(str::to_owned))
        .ok_or("会话文件缺少 id")?;
    let fallback = DateTime::<Utc>::from_timestamp_millis(mtime)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    let project_name = cwd
        .as_deref()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .map(str::to_owned);
    Ok(Some(NormalizedSession {
        source_id: source_id.to_string(),
        session_id,
        project_path: cwd,
        project_name,
        analysis_title: title.filter(|value| !value.trim().is_empty()),
        messages,
        raw_path: path.to_string_lossy().into_owned(),
        raw_mtime_ms: Some(mtime),
        raw_size_bytes: Some(size),
        created_at: created_at.unwrap_or_else(|| fallback.clone()),
        updated_at: updated_at.unwrap_or(fallback),
    }))
}

fn visible_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_visible_pi_agent_messages_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let fixture = [
            serde_json::json!({"type":"session","id":"session-1","cwd":"/tmp/demo","timestamp":"2026-08-30T00:00:00Z"}),
            serde_json::json!({"type":"title_change","title":"修复构建","timestamp":"2026-08-30T00:00:01Z"}),
            serde_json::json!({"type":"message","timestamp":"2026-08-30T00:00:02Z","message":{"role":"user","content":[{"type":"text","text":"为什么失败？"}]}}),
            serde_json::json!({"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"secret"},{"type":"toolCall","name":"bash"},{"type":"text","text":"因为配置错误。"}]}}),
            serde_json::json!({"type":"message","message":{"role":"toolResult","content":[{"type":"text","text":"private output"}]}}),
        ].into_iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n");
        fs::write(&path, fixture).unwrap();
        let (mtime, size) = file_fingerprint(&path).unwrap();
        let session = parse_agent_jsonl(&path, "pi", mtime, size)
            .unwrap()
            .unwrap();
        assert_eq!(session.source_id, "pi");
        assert_eq!(session.analysis_title.as_deref(), Some("修复构建"));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[1].content, "因为配置错误。");
    }

    #[test]
    fn rejects_broken_middle_line_but_tolerates_incomplete_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let session = serde_json::json!({"type":"session","id":"session-1"}).to_string();
        let message =
            serde_json::json!({"type":"message","message":{"role":"user","content":"问题"}})
                .to_string();

        fs::write(&path, format!("{session}\n{{broken\n{message}")).unwrap();
        let (mtime, size) = file_fingerprint(&path).unwrap();
        assert!(parse_agent_jsonl(&path, "pi", mtime, size).is_err());

        fs::write(&path, format!("{session}\n{message}\n{{broken")).unwrap();
        let (mtime, size) = file_fingerprint(&path).unwrap();
        let parsed = parse_agent_jsonl(&path, "pi", mtime, size)
            .unwrap()
            .unwrap();
        assert_eq!(parsed.messages.len(), 1);
    }
}
