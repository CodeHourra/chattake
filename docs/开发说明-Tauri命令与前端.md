# Tauri 命令与桌面前端（v0.1）

## Rust：`invoke` 命令

已在 `apps/desktop/src-tauri/src/lib.rs` 注册：

| 命令 | 模块 | 说明 |
|------|------|------|
| `sync_all` | `commands/sync` | 全量采集，`spawn_blocking` 内执行 |
| `list_sessions` | `commands/sessions` | 分页列表，参数 camelCase |
| `get_session` | `commands/sessions` | `{ id }` |
| `get_session_messages` | `commands/sessions` | `{ sessionId }` |
| `distill_session` | `commands/sessions` | `{ sessionId }`，阻塞线程内跑 Sidecar + DB |
| `search_cards` | `commands/cards` | FTS 搜索 |
| `list_cards` | `commands/cards` | 分页卡片 |
| `get_card` | `commands/cards` | `{ id }` |

`generate_handler!` 必须使用 **`commands::子模块::函数` 全路径**，否则找不到 `__cmd__*` 宏展开符号。

## IPC 序列化

`storage/models.rs` 中与前端交互的结构体已加 `#[serde(rename_all = "camelCase")]`，与 `apps/desktop/src/types/index.ts` 对齐。

## `AppState`

- `Arc<Database>`、`Arc<AppConfig>`：供异步 command 中 `spawn_blocking` 克隆使用。
- `Option<Arc<SidecarManager>>`：找不到 `chattake-sidecar` 时为 `None`，提炼命令会返回明确错误。

## Capabilities

`src-tauri/capabilities/default.json` 当前仅声明 `core:default`。若运行时报「未授权调用某 command」，需在构建产物生成的 ACL 中查找对应 `allow-*` 标识符并追加到该文件的 `permissions` 数组（具体名称以本地 `cargo tauri build` / 文档为准）。

## 前端入口

- 路由：`src/router/index.ts`
- API 封装：`src/lib/tauri.ts`
- 布局：`AppLayout` + `TopBar` + `Sidebar`
- 搜索状态：`stores/search.ts`（Pinia，300ms debounce）

### 知识库页 `KnowledgeView.vue`

- **布局**：`flex flex-col h-full min-h-0`，中间内容区 `flex-1 min-h-0 overflow-y-auto`（列表/卡片在带边框的固定区域内滚动），**分页**在底部 `footer` 使用 `shrink-0`，不随列表滚动。
- **视图**：`n-radio-group` 切换「列表 / 卡片」；偏好持久化 `localStorage` 键 `chattake:knowledgeViewMode`（`list` | `card`）。
- **视觉**：内容区使用**薄荷/青绿渐变底**；每条笔记为**统一静态悬浮**（`shadow` + 半透明白底/暗色底 + `backdrop-blur`），**不用 hover 抬升或加重阴影**，hover 仅微调描边色。
- **列表**：紧凑行距（`space-y-2`、较小字号），标题单行截断 + 摘要两行。
- **卡片网格**：`sm:2 列` / `lg:3 列`，`min-h` 约 132px，摘要两行，提高信息密度。

### 知识类型（英文枚举 → 中文展示）

- **单一数据源**：`packages/shared/src/cardTypes.ts` 中 `CARD_TYPE_LABELS` + `getCardTypeLabel()`；与 sidecar `PROMPT_*` 中的 `type` 枚举一致，并含历史兼容键（如 `architecture`）。
- **使用处**：知识库列表/卡片、`Sidebar` 类型筛选、`SessionCard`、笔记详情 `NoteHeader`（摘要下、技术栈上的绿色类型标签）。

### `@chattake/shared` 与浏览器

- **主入口**（`import '@chattake/shared'`）**不得**聚合导出 `constants.ts`（内部使用 Node 的 `path` / `os`），否则 Vite 打浏览器包时会执行 Node 模块导致**白屏**。
- 路径类常量仅在 Node 侧使用：`import { CHATTAKE_HOME } from '@chattake/shared/constants'`（见 `package.json` 的 `exports`）。

### 会话列表 `SessionsView.vue` + `SessionCard.vue`

- **列表容器**：与会话列表区域外包一层与知识库**同系**的渐变底 + 圆角边框；浅/深均用**外阴影**表达层次（暗色不取消阴影，与知识库条目同一套逻辑）。
- **单条卡片**：默认态与 `KnowledgeView` 条目对齐（半透底、`backdrop-blur`、固定轻阴影；hover 仅描边）；**批量选中**时使用加粗边框 + `ring` + 渐变底 + 更强阴影，选中态比旧版「细边框 + 淡绿底」更明显。

### 提炼结果 `tech_stack`（技术栈）

- **展示位置**：单条会话进入详情后，笔记标题下的蓝色标签（`NoteHeader`）；侧栏「技术栈」区块目前仅为说明文案，**不是**从数据库聚合筛选。
- **落库链路**：`distill_full` → `cards.tech_stack`（逗号拼接）→ 前端 `card.techStack`。
- **易踩坑**：部分模型 JSON 使用 camelCase `techStack`，而 Rust 约定 `tech_stack`。Sidecar 已对返回值做 **tech_stack / techStack 归一化**，Rust `DistillFullResult` 亦增加 `alias = "techStack"`；修复前已分析过的会话需**重新提炼**才会写入技术栈。

依赖安装请在仓库根目录执行 **`bun install`**（workspace 使用 `workspace:` 协议，部分 npm 版本不兼容）。

## 全局分析队列（Pinia）

- **Store**：`stores/analysisQueue.ts` — `enqueue` / `cancel` / `clear`，内部**串行**调用 `distill_session`（与 Sidecar 单通道一致）。
- **入口统一**：会话列表单条「分析」、批量「开始分析」、详情页「提炼笔记 / 重新分析」均通过 `enqueue` 入队。
- **UI**：`components/AnalysisQueuePanel.vue` 挂在 `AppLayout`，任意页面可见进度、耗时、「停止排队」（未执行任务取消并恢复为 `pending`）；标题栏可**收起**为右下角圆角窄条（显示进度与当前标题摘要），点击窄条可重新展开。
- **批量完成 Toast**：`SessionsView` 对批量入队的任务在 callbacks 中计数，全部结束后提示成功/失败条数。
