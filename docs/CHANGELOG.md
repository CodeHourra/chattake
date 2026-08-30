# 寻迹（XunJi）更新日志 — 技术说明

> 本文档与仓库根目录 `CHANGELOG.md` 的版本一一对应，保留**实现向**表述（字段名、路径、数据结构等），供开发、排查与文档对照使用。  
> **面向最终用户的说明请以根目录 `CHANGELOG.md` 为准。**

---

## 0.2.0（2026-08-30）

### 架构与数据

- Schema v7：`sessions/messages/cards/tags/card_tags/jobs/job_items/analysis_runs/cards_fts`；升级前强制 `VACUUM INTO` 备份。
- 四采集器统一标准会话输出；文件 `mtime + size` 预判，变化后以内容哈希确认；Codex 去除内部事件和重复事件。
- `jobs/job_items` 持久化同步与分析任务；任务保存供应商配置快照，不保存 API Key；异常退出标记 `interrupted`。

### 分析与检索

- Sidecar 使用 24,000 字符首尾保留/中段轮次采样；`judge_value` 与 `extract_knowledge` 两阶段协议。
- 固定五类知识；高价值发布、中价值草稿、低价值只记录判断；重新分析保护旧卡片。
- FTS5 trigram + 短词参数化 `LIKE`；标签重命名/合并会事务化重建受影响索引。

### 桌面与 MCP

- Light/Dark Token、结构性液态玻璃、四来源品牌资产、真实任务中心、消息游标分页和懒加载；单生产 Chunk 低于 500 KB。
- `@modelcontextprotocol/server` v2 + `serveStdio`；四个工具只读访问已发布知识，stdout 仅承载 JSON-RPC。
- 构建资源同时包含 `xunji-sidecar` 与 `xunji-mcp`。

---

## 0.1.7（2026-03-26）

### 构建与文档

- **Makefile**：`macos` 目标增加 `TAURI_SIGNING_PRIVATE_KEY` / `xunji.updater.key` 检查与导出
- **docs/macOS 打包说明.md**：产物路径与命名说明

---

## 0.1.6（2026-03-25）

### 体验与构建

- **桌面**：`AppUpdateModal.vue`、`useAppUpdater.ts`；`TopBar` 顶栏入口；`SettingsModal` 关于页移除重复入口
- **更新流程**：`downloadAndInstall` 回调驱动 `n-progress`；无 `contentLength` 时降级为转圈 + 已接收数据量

---

## 0.1.5（2026-03-25）

### 体验与构建

- **Tauri**：`productName` 为 `XunJi`；`bundle.macOS.bundleName` 与 `Info.plist` 的 `CFBundleDisplayName` 为「寻迹」；关于页「应用名称」固定展示品牌名
- **Release**：恢复/保持 `release.yml` 全矩阵（macOS aarch64 + x86_64、Windows）

---

## 0.1.4（2026-03-25）

### 新增与优化

- **桌面更新**：`tauri-plugin-updater`；`SettingsModal` 增加检查更新；`tauri.conf.json` 配置 `updater`（公钥、`endpoints`）；Windows 侧 `tauri.windows.conf.json` 等
- **CI / Release**：`.github/workflows/release.yml` 多平台 `tauri build`、上传安装包与 `latest.json`；文档 `docs/桌面应用-Release与自动更新.md`

---

## 0.1.3（2026-03-25）

### 新增与优化

- **知识库**：`CardFilters` / `list_cards` / `search_cards` 增加 `tech_stack` 筛选（AND；`cards.tech_stack` 逗号字段，`instr` 边界匹配、大小写不敏感）；前端 `filters.selectedTechStacks`、侧栏技术栈 chip 可切换、KnowledgeView 拉列表时传参
- **侧栏**：知识库筛选区使用 `NCollapse` + `display-directive="if"`；去除标签/技术栈独立 `max-height` 滚动，统一由侧栏外层滚动；`:deep` 避免折叠内容区内部滚动条

### 体验与修复

- **分段控件**：`App.vue` 中 `button.segment-pill-btn` 去除 WebView 默认按钮外观；`TopBar`、`SessionToolbar`、`KnowledgeView` 统一分段样式与暗色变体
- **SessionCard**：紧凑布局；选中态 `ring-inset`；底部元数据容器 `m-0 p-0`
- **踩坑文档**：重命名为中文主文件名；新增 `docs/踩坑/README.md`；`ring` + `overflow-hidden` 裁切说明

---

## 0.1.2（2026-03-22 – 2026-03-23）

### 新增与优化

- **采集**：支持 **CodeBuddy** 对话记录导入，扫描路径与问渠 `scanner/codebuddy.py` 一致：macOS `~/Library/Application Support/CodeBuddyExtension/Data`、Linux `~/.local/share/CodeBuddyExtension`，递归识别其下含 `index.json` 与 `messages/` 的会话目录；需在设置 → 数据源中开启「CodeBuddy」，默认关闭以免未安装时无效扫盘
- **配置**：旧版配置文件在启动时会自动补充 CodeBuddy 数据源项（若尚无）；若仍仅为旧默认 `~/.codebuddy`，启动时会迁移为上述 CodeBuddyExtension 根目录
- **CodeBuddy 采集与展示**
  - 从工作区目录 `index.json` 的 `conversations[].name` 写入会话展示标题（`analysis_title`），与 IDE 会话列表一致；列表/详情在未提炼笔记前即可显示可读标题
  - 侧栏「项目」分组名：由 `Workspace Folder` / `Workspace Path` 推导；若路径末级为纯数字或长 hex 等无辨识度目录名，则尝试使用上一级目录名
  - 助手消息中的工具调用与思考过程统一为 `[Tool: …]`、`<thinking>…</thinking>`，便于对话回放与 Markdown 渲染
- **旧版 CodeBuddy 消息**：兼容 `content` 为**单字符字符串数组**（拼接后与新版等价）、以及旧版字段名 **`Workspace Path:`**（与新版 `Workspace Folder:` 并存，优先匹配 Folder）
- **修复**：首条用户消息中 `content` 为**整段字符串**（非 text 块数组）时，可正确解析工作区路径，避免侧栏项目名退化为 `history` 下哈希目录名

### 修正

- **CodeBuddy 路径**：此前按 `~/.codebuddy/projects/*.jsonl` 采集与产品实际会话存储不符，已改为扩展数据目录（见上）

### 其他

- 会话列表未分析时优先展示上述采集标题；会话详情顶栏支持 `analysisTitle`；对话回放中工具标签匹配更宽松（含下划线等工具名）

---

## 0.1.1（2026-03-22）

### 新增与优化

- **会话整理**：在对话记录侧栏可进入「会话整理」，在目录树中多选数据源、主机或项目后，一次性清理所选范围内的会话记录；清理的是应用内数据，**不会删除**本机上的原始对话文件；若一次选了多个范围，同一对话只会被删一次
- **关于**：在设置中可查看应用版本、运行环境与本次更新说明
- **导出**：笔记可导出为 Markdown，支持单条、多选或整库导出；整库导出不受当前列表筛选影响
- **会话与笔记**：列表与详情可查看会话路径与会话标识；列表标题优先展示首条用户消息摘要
- **分析**：分析失败时可看到原因；分析队列中可查看任务详情
- **阅读**：笔记正文 Markdown 显示效果优化
- **体验**：搜索与同步等相关提示与文案优化

### 修复与调整

- 侧栏点选「未关联项目」时，列表能正确显示该分组下的会话
- 批量删除会话仅通过「会话整理」勾选分组后进行，避免误操作

---

## 0.1.0

**发布说明**：首个正式版本，可在本机完成从采集对话、整理列表、提炼笔记到知识库浏览与搜索的完整流程。

### 新增

- **应用**：桌面端应用，支持浅色与深色外观
- **采集**：支持从 Claude Code、Cursor 等来源导入对话到本地
- **整理**：对话列表、会话详情（含笔记与对话回放）、按数据源分组的侧栏目录
- **提炼**：对会话做价值判断并生成结构化笔记，接入本地模型或命令行工具
- **知识库**：笔记列表、全文搜索、标签与类型筛选
- **其他**：同步与设置、分析任务队列

### 已知限制

- 部分导出入口仍为占位或能力受限；个别数据源仅可配置名称与图标，尚不能采集
- 同步需手动触发；与其他工具的深度联动仍在完善中
