# 踩坑：Cursor 采集 — `workspace.json` 路径百分号编码导致侧栏项目名不可读

本文记录 **有得桌面端** 从 Cursor `workspaceStorage/.../workspace.json` 解析项目路径时，若仅去掉 `file://` 而不做 **URL 解码**，侧栏「按项目分组」会显示 `%E5%90%91...` 而非中文目录名的问题：**根因**、**修复要点**与 **数据迁移**。

---

## 现象

- **UI**：对话侧栏目录树中，Cursor 来源下的「项目」节点显示为 **`%E5%90%91%E5%96%84...`** 等编码串，Tooltip 里也是整段百分号编码，可读性差。
- **数据**：`sessions.project_path` / `sessions.project_name` 中末段路径仍含 `%`，与资源管理器里真实文件夹名（如「向善数据」）不一致。
- **复现条件**：本机 Cursor 工作区路径含 **非 ASCII 字符**（中文等）；VS Code / Cursor 常将 `folder` 写成带百分号编码的 `file://` URL。

---

## 根因

- `workspace.json` 中 **`folder` 字段**典型值为 `file:///Users/.../%E5%90%91...`（UTF-8 字节经 **百分号编码** 写入 URL）。
- 旧逻辑等价于只做 **`strip_prefix("file://")`**，得到仍含 **`%XX`** 的字符串；再用 `Path::file_name()` 取末段作为 **`project_name`**，得到的是**编码后的假文件名**，而非 UTF-8 展示名。

```text
workspace.json["folder"]
       │
       ├─ 错误: strip_prefix("file://") only
       │        → "/Users/x/%E5%90%91..."  → project_name = "%E5%90%91..."
       │
       └─ 正确: Url::parse(file URL) → to_file_path()
                → "/Users/x/向善数据"     → project_name = "向善数据"
```

---

## 解决方法（已实现）

1. **统一解码**  
   新增 `apps/desktop/src-tauri/src/path_local.rs`：用 **`url` crate** 将 `folder` 按 **file URL** 解析，`to_file_path()` 得到本地绝对路径（自动完成百分号 → UTF-8）。  
   - 对外函数：`decode_cursor_folder_to_local_path`、`decode_session_paths`（迁移用）。  
   - Cursor 采集：`collector/cursor.rs` 中 `read_workspace_project_path` 改为调用上述逻辑。

2. **历史数据**  
   数据库迁移 **v6**（`storage/migrations.rs` → `migrate_v6`）：扫描 `sessions` 中 `project_path` / `project_name` 含 **`%`** 的行，按 `decode_session_paths` 写回，避免仅依赖「重新同步」才能纠正旧库。

3. **依赖**  
   `apps/desktop/src-tauri/Cargo.toml` 增加 **`url`**。

4. **测试**  
   `path_local` 模块内单测覆盖：`file://...` 与纯路径 `%` 末段两种输入。

---

## 后续如何避免

- **新路径解析**：凡来自 **URI / `file://`** 的字段，应先按 URL 解析再当地路径化，不要只去前缀。  
- **回归**：改动 `read_workspace_project_path` 时跑 `cargo test path_local collector::cursor::tests`。  
- **日志**：迁移 v6 会对修正条数打 `info`，便于确认老库是否被修复。

---

## 相关位置

| 路径 | 说明 |
|------|------|
| `apps/desktop/src-tauri/src/path_local.rs` | 解码实现与单元测试 |
| `apps/desktop/src-tauri/src/collector/cursor.rs` | `read_workspace_project_path` 调用解码 |
| `apps/desktop/src-tauri/src/storage/migrations.rs` | `migrate_v6` 修正已入库的 `sessions` |
| `apps/desktop/src-tauri/Cargo.toml` | `url` 依赖 |

---

## 备注（与 Cursor 编辑器本身的区别）

若 **Cursor IDE 自带界面**（非有得）仍显示编码串，属于 **客户端展示问题**，需在 Cursor 侧反馈；有得侧仅保证 **同一数据源** 在应用内展示为解码后的项目名。
