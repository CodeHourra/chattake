# 导出 Markdown 格式约定（v0.1.1）

有得将知识卡片导出为 **单个 `.md` 文件**：文件开头为 **YAML frontmatter**，随后空行接 **正文**（与库内 `note` 字段一致，已是 Markdown）。

## 文件命名

- **单条另存为**：由系统「另存为」对话框决定路径与文件名。
- **批量 / 全库**：由 `{标题摘要}-{card_id 前 8 位}.md` 生成文件名；非法路径字符替换为 `-`，标题过长截断。

## Frontmatter 字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `title` | string | 卡片标题 |
| `source` | string? | 来源数据源名称（冗余展示字段） |
| `project` | string? | 来源项目名 |
| `created_at` | string | 卡片创建时间（RFC 3339，与 DB 一致） |
| `tags` | string[] | 标签列表（可空数组） |
| `type` | string? | 知识类型（与库内 `type` 一致） |
| `session_id` | string? | **数据源侧**会话标识（`sessions.session_id`） |
| `card_id` | string | 卡片主键 UUID |
| `session_internal_id` | string | **应用内**会话表主键（`sessions.id`），便于与界面/DB 对照 |

正文前分隔线为：

```markdown
---
（YAML）
---

```

## 语义说明

- **「导出全部笔记」** 仅与数据库 `cards` 全表有关，**与当前知识库筛选、分页无关**。
- **「导出所选」** 仅导出勾选的 `card_id` 列表对应文件。

## 实现位置

- 拼接与写盘：`apps/desktop/src-tauri/src/commands/export.rs`
- 前端交互与对话框：`apps/desktop/src/lib/cardExport.ts`、`KnowledgeView.vue`、`TopBar.vue`
