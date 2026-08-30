//! Cursor `workspace.json` 与历史库内路径的本地化展示。
//!
//! VS Code / Cursor 在 `folder` 字段中常写入 `file://` URL，非 ASCII 路径段会以 **百分号编码**
//! 存储。仅去掉 `file://` 会得到仍含 `%E5%90%91...` 的字符串，侧栏「项目名」不友好。
//!
//! ```text
//! workspace.json["folder"]     DB 中 project_path（旧）
//!        │                              │
//!        └──── decode_cursor_folder ────┘
//!                      │
//!               "/Users/x/向善数据"
//! ```

use url::Url;

/// 将 Cursor `workspace.json` 的 `folder` 字段或库内已存的本地路径字符串解码为绝对路径。
///
/// - 入参可为完整 `file://...` URL，或已去掉 scheme 但仍含 `%` 的路径。
/// - 无法解析时返回原字符串，避免破坏异常数据。
pub fn decode_cursor_folder_to_local_path(folder: &str) -> String {
    let trimmed = folder.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // 1) 标准：整段按 file URL 解析（含 % 解码）
    if let Ok(u) = Url::parse(trimmed) {
        if u.scheme() == "file" {
            if let Ok(pb) = u.to_file_path() {
                let s = pb.to_string_lossy().into_owned();
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }

    // 2) 兼容：仅路径、段内仍带 %（异常或旧逻辑只做了 strip_prefix）
    let raw = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    if raw.contains('%') {
        let synthetic = format!("file://{raw}");
        if let Ok(u) = Url::parse(&synthetic) {
            if u.scheme() == "file" {
                if let Ok(pb) = u.to_file_path() {
                    let s = pb.to_string_lossy().into_owned();
                    if !s.is_empty() {
                        return s;
                    }
                }
            }
        }
    }

    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_file_url_with_percent_encoded_segment() {
        let s = "file:///Users/steve/%E5%90%91%E5%96%84%E6%95%B0%E6%8D%AE";
        assert_eq!(
            decode_cursor_folder_to_local_path(s),
            "/Users/steve/向善数据"
        );
    }

    #[test]
    fn decodes_plain_path_with_percent_encoded_segment() {
        let s = "/Users/steve/%E5%90%91%E5%96%84%E6%95%B0%E6%8D%AE";
        assert_eq!(
            decode_cursor_folder_to_local_path(s),
            "/Users/steve/向善数据"
        );
    }

    #[test]
    fn passthrough_ascii_path_without_percent() {
        let s = "/Users/steve/myproject";
        assert_eq!(
            decode_cursor_folder_to_local_path(s),
            "/Users/steve/myproject"
        );
    }
}
