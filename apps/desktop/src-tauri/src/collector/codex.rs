//! Codex JSONL 采集：只保留 response_item 中用户与助手可见文本。

use std::collections::HashMap;
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

pub struct CodexCollector {
    scan_dirs: Vec<PathBuf>,
}

impl CodexCollector {
    pub fn new(scan_dirs: Vec<PathBuf>) -> Self {
        Self { scan_dirs }
    }

    pub fn collect_changed<F>(&self, unchanged: F) -> CollectionBatch
    where
        F: Fn(&Path, i64, i64) -> bool,
    {
        // 同一 UUID 同时出现在 active/archive 时，先按文件 mtime 选较新者，再判断是否需要解析。
        let mut candidates: HashMap<String, (PathBuf, i64, i64)> = HashMap::new();
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
                let Some((mtime, size)) = file_fingerprint(path) else {
                    continue;
                };
                let key = session_id_from_filename(path)
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                match candidates.get(&key) {
                    Some((_, current_mtime, _)) if *current_mtime >= mtime => {}
                    _ => {
                        candidates.insert(key, (path.to_path_buf(), mtime, size));
                    }
                }
            }
        }

        let mut batch = CollectionBatch {
            found: candidates.len() as u32,
            ..Default::default()
        };
        for (_, (path, mtime, size)) in candidates {
            if unchanged(&path, mtime, size) {
                batch.skipped += 1;
                batch
                    .skipped_paths
                    .push(path.to_string_lossy().into_owned());
                continue;
            }
            match parse_codex_jsonl(&path, mtime, size) {
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
        batch
    }
}

fn session_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let candidate = stem.get(stem.len().checked_sub(36)?..)?;
    uuid::Uuid::parse_str(candidate)
        .ok()
        .map(|id| id.to_string())
}

fn parse_codex_jsonl(
    path: &Path,
    mtime: i64,
    size: i64,
) -> Result<Option<NormalizedSession>, Box<dyn std::error::Error>> {
    let mut external_id = None;
    let mut cwd = None;
    let mut messages = Vec::new();
    let mut first_timestamp = None;
    let mut last_timestamp = None;

    let mut lines = BufReader::new(File::open(path)?).lines().peekable();
    while let Some(line) = lines.next() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) if lines.peek().is_none() => {
                log::debug!("忽略活跃 Codex 会话末尾未完成行: {error}");
                continue;
            }
            Err(error) => return Err(format!("JSONL 中间行损坏: {error}").into()),
        };
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string);
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                let payload = &value["payload"];
                external_id = payload
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(external_id);
                cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(cwd);
                if first_timestamp.is_none() {
                    first_timestamp = timestamp.clone().or_else(|| {
                        payload
                            .get("timestamp")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                }
            }
            Some("response_item") => {
                let payload = &value["payload"];
                if payload.get("type").and_then(Value::as_str) != Some("message") {
                    continue;
                }
                let role = match payload.get("role").and_then(Value::as_str) {
                    Some("user") => "user",
                    Some("assistant") => "assistant",
                    _ => continue,
                };
                let allowed = if role == "user" {
                    "input_text"
                } else {
                    "output_text"
                };
                let content = payload
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some(allowed))
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n");
                if content.trim().is_empty() {
                    continue;
                }
                let content = if role == "user" {
                    match normalize_user_content(&content) {
                        Some(content) => content,
                        None => continue,
                    }
                } else {
                    content.trim().to_string()
                };
                first_timestamp.get_or_insert_with(|| timestamp.clone().unwrap_or_default());
                if timestamp.is_some() {
                    last_timestamp = timestamp.clone();
                }
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
    let Some(session_id) = external_id.or_else(|| session_id_from_filename(path)) else {
        return Ok(None);
    };
    if messages.is_empty() {
        return Ok(None);
    }
    let fallback = DateTime::<Utc>::from_timestamp_millis(mtime)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    let created_at = first_timestamp
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.clone());
    let updated_at = last_timestamp.unwrap_or_else(|| fallback.clone());
    let project_name = cwd
        .as_ref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string);
    Ok(Some(NormalizedSession {
        source_id: "codex".to_string(),
        session_id,
        project_path: cwd,
        project_name,
        analysis_title: None,
        messages,
        raw_path: path.to_string_lossy().into_owned(),
        raw_mtime_ms: Some(mtime),
        raw_size_bytes: Some(size),
        created_at,
        updated_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_visible_messages_and_excludes_internal_events() {
        let dir = tempfile::tempdir().unwrap();
        let id = "11111111-1111-4111-8111-111111111111";
        let path = dir
            .path()
            .join(format!("rollout-2026-08-30T00-00-00-{id}.jsonl"));
        let fixture = [
            serde_json::json!({"type":"session_meta","timestamp":"2026-08-30T00:00:00Z","payload":{"id":id,"cwd":"/tmp/demo"}}),
            serde_json::json!({"type":"response_item","timestamp":"2026-08-30T00:00:01Z","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"secret"}]}}),
            serde_json::json!({"type":"response_item","payload":{"type":"reasoning","summary":[]}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"duplicate"}}),
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions\n<INSTRUCTIONS>secret</INSTRUCTIONS>"}]}}),
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context><cwd>/private</cwd></environment_context>"}]}}),
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"The following is the Codex agent history whose request action you are assessing:\nprivate tool history"}]}}),
            serde_json::json!({"type":"response_item","timestamp":"2026-08-30T00:00:02Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"问题"},{"type":"image_url","url":"x"}]}}),
            serde_json::json!({"type":"response_item","timestamp":"2026-08-30T00:00:03Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"回答"}]}}),
            serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"tool"}}),
        ].into_iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n");
        fs::write(&path, fixture).unwrap();
        let (mtime, size) = file_fingerprint(&path).unwrap();
        let session = parse_codex_jsonl(&path, mtime, size).unwrap().unwrap();
        assert_eq!(session.session_id, id);
        assert_eq!(session.project_name.as_deref(), Some("demo"));
        assert_eq!(
            session
                .messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>(),
            vec!["问题", "回答"]
        );
    }

    #[test]
    fn keeps_newer_file_when_active_and_archive_share_session_id() {
        let dir = tempfile::tempdir().unwrap();
        let active = dir.path().join("sessions/2026/08/30");
        let archived = dir.path().join("archived_sessions");
        fs::create_dir_all(&active).unwrap();
        fs::create_dir_all(&archived).unwrap();
        let id = "22222222-2222-4222-8222-222222222222";
        let name = format!("rollout-2026-08-30T00-00-00-{id}.jsonl");
        let write_fixture = |path: &Path, answer: &str| {
            let rows = [
                serde_json::json!({"type":"session_meta","payload":{"id":id,"cwd":"/tmp/demo"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":answer}]}}),
            ];
            fs::write(
                path,
                rows.into_iter()
                    .map(|row| row.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .unwrap();
        };
        write_fixture(&archived.join(&name), "旧回答");
        std::thread::sleep(std::time::Duration::from_millis(2));
        write_fixture(&active.join(&name), "新回答");

        let batch = CodexCollector::new(vec![active, archived]).collect_changed(|_, _, _| false);
        assert_eq!(batch.sessions.len(), 1);
        assert_eq!(batch.sessions[0].messages[0].content, "新回答");
    }
}
