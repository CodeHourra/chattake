# 有得（ChatTake）产品需求文档 PRD

> **历史愿景文档**：本文包含尚未实现和已取消的早期设想，不可作为当前能力清单。v0.2.0 的实施边界以 [`../plans/2026-08-30-v0.2.0-架构与功能重构.md`](../plans/2026-08-30-v0.2.0-架构与功能重构.md) 为准。

## v0.2.0 实现状态（2026-08-30）

| 能力 | 状态 |
|---|---|
| Claude Code / Cursor / Codex / CodeBuddy 本地增量采集 | 已实现 |
| 多 API 配置、单任务配置快照、硅基流动 | 已实现 |
| 五类原子知识、主题/技术标签、草稿治理 | 已实现 |
| 持久任务、取消、失败重试、异常中断恢复 | 已实现 |
| trigram FTS、短词回退、标签合并 | 已实现 |
| 只读 MCP 四工具 | 已实现 |
| CLI LLM、`*-internal`、向量库、REST、远程同步、模型并发 | 已取消或不在 v0.2.0 范围 |

当前功能清单见 [`../specs/v0.2.0-功能清单.md`](../specs/v0.2.0-功能清单.md)。下文保留为历史产品愿景。

> 版本：v0.7 草稿  
> 日期：2026-03-20  
> 状态：补充去重策略、批量并发分析、UI 交互设计

---

## 一、产品概述

### 1.1 产品定位

**有得**是一款面向个人开发者的 AI 编程知识平台，将散落在 Cursor、Claude Code、CodeBuddy 等多个 AI IDE 中的对话记录，自动同步、提炼、分类，沉淀为个人可搜索、可复用的技术知识库。

> 追寻每一次和 AI 对话的思维轨迹

### 1.2 核心价值主张

| 痛点 | 解法 |
|------|------|
| AI 对话记录散落在各 IDE，找不回来 | 统一采集，多源汇聚 |
| 优质对话一次性消费，无法复用 | 大模型自动提炼成知识笔记 |
| 知识积累靠人工整理，成本高 | 自动分类打标，生成知识卡片 |
| 本地知识无法喂给 AI 使用 | 开放 MCP/API 接口，作为 AI 本地知识库 |

### 1.3 目标用户

**主要用户：** 重度使用 AI 编程工具的个人开发者  
**用户画像：**
- 每天使用 1~3 个 AI IDE（Cursor / Claude Code / CodeBuddy 等）
- 有知识沉淀意识，但苦于没有顺手工具
- 对数据隐私敏感，倾向本地存储
- 有一定技术背景，接受命令行/配置文件操作

---

## 二、产品目标（北极星）

**MVP 目标（v1.0）：** 让开发者在不改变现有工作流的前提下，自动积累个人技术知识库，并能在需要时快速检索复用。

**衡量指标：**
- 日活知识条目增长数（知识生产效率）
- 知识检索次数（知识复用频率）
- 数据源接入数量（采集覆盖度）

---

## 三、功能模块

### 3.1 数据采集层（Collector）

**核心能力：** 自动监听并同步多个 AI IDE 的对话记录

#### 支持数据源（按优先级）

> 以下存储路径和格式均经过本地实际读取验证。路径扫描规则参考问渠（wenqu）开源实现。

| 优先级 | 数据源 | 存储路径 | 格式 | 解析难度 |
|--------|--------|---------|------|---------|
| P0 | **Claude Code（内网版）** | `~/.claude-internal/projects/{路径hash}/{session-uuid}.jsonl`<br>全局历史：`~/.claude-internal/history.jsonl`<br>子代理：`sessions/{id}/subagents/agent-*.jsonl` | JSONL | ✅ 低 |
| P0 | **Cursor** | 全局：`~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`（~208MB）<br>工作区：`workspaceStorage/<id>/state.vscdb`（约 196 个）<br>Plan 文件：`~/.cursor/plans/*.plan.md` | SQLite + Lexical 富文本 | ⚠️ 高 |
| P1 | **CodeBuddy（扩展 / IDE，与问渠扫描一致）** | 根目录：macOS `~/Library/Application Support/CodeBuddyExtension/Data/`，Linux `~/.local/share/CodeBuddyExtension/`（见问渠 `scanner/codebuddy.py`）。会话落盘在目录树中名为 `history` 的路径之下：`.../history/{workspace_id}/{conversation_id}/`，含 `index.json`（消息索引）与 `messages/*.json`（单条消息）。**不作为**主会话来源：`~/.codebuddy/projects/*.jsonl`（CLI JSONL，与扩展存储不同） | JSON | ✅ 中等 |
| P1 | **CodeBuddy 补充数据** | 索引库：`~/Library/Application Support/com.tencent.copilot/copilot_index.db`<br>问题历史：`~/Library/Application Support/JetBrains/{IDE版本}/options/tencent-copilot-question-history.xml` | SQLite + XML | ✅ 中等 |
| P2 | **Windsurf** | `~/Library/Application Support/Windsurf/User/workspaceStorage/` | SQLite | 中 |
| P2 | **GitHub Copilot** | VSCode workspaceStorage | SQLite | 中 |

#### 各数据源格式详解

**Claude Code（内网版）**

```json
{
  "type": "user|assistant|progress|system",
  "message": {
    "role": "user|assistant",
    "content": "文本或 content 数组",
    "model": "claude-4.6-opus|deepseek-v3.1-terminus|glm-4.7"
  },
  "uuid": "消息ID",
  "timestamp": "ISO-8601",
  "sessionId": "会话ID",
  "version": "2.1.78",
  "gitBranch": "main",
  "usage": { "input_tokens": 100, "output_tokens": 200 }
}
```

特点：✅ 包含完整 token 统计 ✅ 工具调用完整记录（Bash/Read/Write 等）✅ 支持多代理会话追踪

**Cursor**

- 消息以 `bubbles` 数组存储在 `tabs` 中，每个 bubble 有 `type(user/assistant)`、`id`、`mentions`、`fileSelections`、`selectedCommits` 等
- 消息文本嵌在 **Lexical Editor 富文本格式**（React 富文本）中，需要额外解析
- Plan 文件结构清晰：YAML frontmatter 含 `name`、`overview`、`todos`（含 status/dependencies）
- ❌ 无 token 统计，时间戳为毫秒时间戳

**CodeBuddy CLI（独立 JSONL 流，与扩展会话目录不同）**

> 有得当前版本对 CodeBuddy 的采集实现为 **CodeBuddyExtension** 根目录下的会话目录（`index.json` + `messages/`，见上表）；下表为 CLI 侧 JSONL 行格式，供对照，**非**当前采集主路径。

```json
{
  "type": "message|function_call|function_call_result",
  "role": "user|assistant",
  "content": [{ "type": "input_text|output_text", "text": "..." }],
  "providerData": {
    "usage": { "inputTokens": 100, "outputTokens": 200, "totalTokens": 300 }
  }
}
```

特点：✅ 包含函数调用完整链路 ✅ token 消耗统计 ✅ 结构与 Claude Code 类似，解析可复用

**CodeBuddy JetBrains 插件**

目录结构：

```
~/Library/Application Support/CodeBuddyExtension/Data/{USER_ID}/CodeBuddyIDE/{USER_ID}/history/
└── {conversation_id}/
    ├── index.json                ← 会话索引
    └── {session_id}/
        ├── index.json            ← 消息列表索引
        └── messages/
            ├── {message_id}.json ← 单条消息（每条一个文件）
            └── ...
```

单条消息结构：

```json
{
  "role": "user|assistant",
  "message": "完整对话内容",
  "references": ["引用的文件列表"],
  "extra": {
    "os": "macOS",
    "shell": "zsh",
    "workspace": "/path/to/project",
    "gitStatus": "...",
    "usage": { "inputTokens": 100, "outputTokens": 200 }
  }
}
```

附加数据：
- **copilot_index.db**（SQLite）：包含 conversations / query_log / projects / studios / studio_snapshots 表，提供会话元数据和项目信息
- **tencent-copilot-question-history.xml**（XML）：简略提问历史，6 个 JetBrains IDE 版本（IntelliJIdea / PhpStorm / GoLand 各 2025.2 & 2025.3）均有此文件

特点：✅ 消息文本直接可读 ✅ token 统计在 extra 字段 ✅ 包含文件引用和工程上下文信息

#### 三者对比

| 特征 | Cursor | Claude Code | CodeBuddy CLI | CodeBuddy JetBrains |
|------|--------|-------------|---------------|---------------------|
| 格式 | SQLite + Lexical | JSONL | JSONL | JSON + SQLite + XML |
| 解析难度 | ⚠️ 高 | ✅ 简单 | ✅ 简单 | ✅ 中等 |
| 消息文本 | 嵌在富文本中 | 直接可读 | 直接可读 | 直接可读 |
| Token 统计 | ❌ 无 | ✅ 有 | ✅ 有 | ✅ 有（extra 字段） |
| 工具调用 | 部分 | ✅ 完整 | ✅ 完整 | ✅ 有（references） |
| 时间戳 | 毫秒时间戳 | ISO-8601 | 无显式时间戳 | 需解析 |
| 模型信息 | 需解析 | ✅ 有 | 需解析 | 需解析 |

#### 远端采集（SSH 模式）

> 参考问渠的 SSH + rsync 方案，有得需支持同样的远端采集能力（Claude Code / CodeBuddy 经常运行在远端 Linux 服务器上）。

**Linux 路径扫描规则（问渠实测验证）：**

| IDE | 扫描命令片段 | 说明 |
|-----|-------------|------|
| **Claude Code** | `find /home /data /root /opt /srv /workspace /projects -maxdepth 7 -type d -name projects \( -path '*/.claude-internal/*' -o -path '*/.claude/projects' \)` | 先广扫，失败回退到 `find ~ -maxdepth 7 ...` |
| **CodeBuddy** | `find ... -maxdepth 12 -type d -name history -path '*/.local/share/CodeBuddyExtension/*'` | **需要 maxdepth 12**，DevCloud 环境 home 目录实际层级可达 10 层 |
| **Cursor（SpecStory）** | `find ... -maxdepth 6 -type d -name .specstory` | 需在 Cursor 中安装 SpecStory 插件 |

**关键实现细节（来自问渠）：**
- SSH 连接必须支持 `BatchMode=yes`（无密码交互），否则扫描失败
- rsync 同步原始文件后，在本机完成解析（Claude Code JSONL → Markdown）
- 数据目录按 `<host>/<project>/sessions/*.md` 结构归档
- `isSidechain: true` 的消息是子代理内部工具调用循环，**应跳过不纳入知识提炼**

#### 采集机制
- **文件 Watch 模式**：使用系统 FSEvent（macOS）/inotify（Linux）监听文件变化，增量同步
- **定时扫描模式**：兜底策略，每 N 分钟全量扫描（可配置，默认 30min）
- **手动触发**：用户可一键全量同步
- **不依赖 SpecStory 插件**：Cursor 直接读取原生 `state.vscdb`，无需安装任何 IDE 插件

#### 去重策略

**去重粒度：Session 级（单条会话是最小单位）**

| 场景 | 去重策略 |
|------|---------|
| 同一 session 增量同步重叠 | `session_id` 精确匹配，已存在则跳过 |
| Claude Code 本地 + 远端 SSH 同一份文件 | `session_id` + `source_host` 组合唯一键 |
| Cursor vscdb 多次读取同一 workspace | `(project_path, timestamp_minute)` 模糊匹配 |
| 用户重新导入历史数据 | `content_hash`（消息内容 MD5）兜底去重 |

**重新分析检测：**
- 每次同步记录 session 的 `message_count`
- 下次同步发现 `message_count` 变化 → 标记为「会话有更新，可重新分析」（黄色角标提示）
- **不自动重跑**，由用户手动触发重新分析

#### 数据规范化

所有来源的对话统一转换为内部标准格式：

```json
{
  "id": "uuid",
  "source": "cursor|claude-code|codebuddy|...",
  "session_id": "原始 session id",
  "project_path": "/Users/xxx/myproject",
  "project_name": "myproject",
  "created_at": "ISO8601",
  "updated_at": "ISO8601",
  "messages": [
    {
      "role": "user|assistant",
      "content": "消息内容",
      "timestamp": "ISO8601",
      "usage": { "input_tokens": 0, "output_tokens": 0 }
    }
  ],
  "raw_path": "原始文件路径"
}
```

> **Cursor 解析注意**：需要实现 Lexical 富文本解析器，从 `bubbles[].message` 中提取纯文本，这是三个数据源中唯一的高难度解析任务。建议参考开源的 `lexical` 解析库。

---

### 3.2 知识提炼层（Distiller）

**核心能力：** 用大模型将原始对话提炼成结构化知识

#### 3.2.1 Distiller 提供商配置

支持两种提炼模式，用户在 Settings 中配置，可随时切换：

**模式一：API Provider（直接调用大模型 API）**

采用 OpenAI 兼容协议优先的设计，内置主流提供商快速配置，同时支持自定义端点兜底。

| 优先级 | 提供商 | 代表模型 | 说明 |
|--------|--------|---------|------|
| P0 | **OpenAI** | GPT-4o / GPT-4.1 | 最广泛，开发者首选 |
| P0 | **Anthropic** | Claude 3.5 Sonnet / 3.7 | 代码理解强，与 Claude Code 用户高度重合 |
| P0 | **DeepSeek** | DeepSeek-V3 / R1 | 性价比极高，国内开发者大量使用 |
| P0 | **Ollama** | Llama3 / Qwen / Mistral... | 本地模型，零 token 费用，隐私友好 |
| P1 | **腾讯混元** | Hunyuan-pro | 腾讯系/工蜂 AI 用户 |
| P1 | **阿里百炼** | Qwen-Max / Qwen-Plus | 国内最大开发者生态 |
| P1 | **Google Gemini** | Gemini 1.5 Pro / 2.0 | 长上下文强 |
| P1 | **硅基流动** | 聚合多模型 | 国内聚合平台，覆盖面广 |
| P2 | 字节豆包 / 智谱 GLM / MiniMax | — | 后续迭代扩展 |

配置示例（`config.toml`）：

```toml
[distiller.api]
# 内置 provider（自动填充 base_url）
provider = "anthropic"        # openai / anthropic / deepseek / ollama
                              # hunyuan / qwen / gemini / siliconflow

api_key  = "sk-..."
model    = "claude-3-5-sonnet-20241022"

# 自定义端点（openai-compatible 万能兼容模式）
# provider  = "openai-compatible"
# base_url  = "https://api.siliconflow.cn/v1"
# api_key   = "sk-xxx"
# model     = "deepseek-ai/DeepSeek-V3"
```

**Token 使用统计（API 模式专属功能）：**

- 每次提炼记录 prompt_tokens / completion_tokens / 模型单价
- 自动换算人民币成本（¥），给用户直观感知
- 统计维度：本次同步 / 今日 / 本月 / 累计总消耗
- 展示位置：
  - 同步完成状态栏提示：`"提炼 5 条，消耗 ~2,400 tokens，约 ¥0.02"`
  - Settings → 用量统计页（折线图 + 明细表）
  - 每张知识卡片元数据中记录生成成本

---

**模式二：CLI Provider（调用本地 AI CLI 工具）**

通过 stdin/stdout 与本地 CLI 交互，无需 API Key，复用用户已有的订阅额度。

| 支持 CLI | 非交互参数 | 说明 |
|----------|-----------|------|
| `claude` | `--print --yes` | Claude Code CLI |
| `codex` | `--approval-policy auto` | OpenAI Codex CLI |
| `codebuddy` | `--print` | CodeBuddy CLI |
| 自定义命令 | 用户配置 | 扩展支持其他 CLI |

配置示例：

```toml
[distiller.cli]
command      = "claude"
timeout_secs = 120
```

**统一 Distiller 抽象接口：**

```
┌─────────────────────────────────┐
│     Distiller Interface         │
│  distill(session) → cards[]     │
└────────────┬────────────────────┘
             │
    ┌────────┴──────────┐
    ▼                   ▼
API Provider        CLI Provider
(HTTP API 调用)    (本地 CLI stdin/stdout)
```

- CLI 解析失败时，自动 fallback 到 API 模式（如已配置）
- 提供「原始输出调试模式」，方便排查 CLI 解析问题
- 各 CLI 适配器模块化，独立维护，应对 CLI 工具格式变化

---

#### 3.2.2 自动提炼触发机制

有得是客户端应用，不常驻后台，触发策略围绕"启动时"和"用户主动"设计：

**触发优先级：**

```
P0  应用启动时自动同步（按用户配置模式执行）
P1  用户手动触发（工具栏一键同步按钮）
P1  勾选会话 → 批量分析
P2  macOS Launch Agent 后台采集（可选开启）
P3  兜底：距上次同步超过 X 小时，下次启动时提示
```

**启动同步三种模式（Settings 可配置）：**

| 模式 | 行为 | 默认 |
|------|------|------|
| **静默模式** | 启动即后台同步+提炼，完成后状态栏提示"同步了 N 条" | ✅ 默认 |
| **询问模式** | 启动弹提示"发现 N 条新对话，是否提炼？"，用户确认后执行 | 可选 |
| **手动模式** | 完全不自动，用户主动点同步按钮 | 可选 |

**macOS Launch Agent（可选开启）：**

解决应用不常驻时数据不丢失的问题。通过 `launchd` 注册轻量后台 agent，职责分离：

```
Launch Agent（轻量，每 2 小时，可配置间隔）
    └── 只做：扫描文件变化 → 原始数据写入 SQLite（不调用 LLM）

应用启动时
    └── 发现未提炼的 session → 按配置模式触发提炼（LLM 调用）
```

优势：
- 数据采集不依赖应用是否打开 ✅
- LLM 提炼（消耗 token）仍在用户感知下触发 ✅
- 应用无需常驻内存 ✅
- Launch Agent 可在设置中一键开启/关闭 ✅

**批量分析逻辑：**

```
批量分析流程
│
├── 1. 用户勾选 sessions（支持全选/按项目选）
├── 2. 先跑轻量 Prompt B 做价值判断（value: none/low/medium/high）
│     └── value=none → 标记跳过，不消耗后续 token
├── 3. 对 medium/high 的 session 并发跑完整提炼
│     ├── 并发数：默认 3，可在 AI 设置中配置（1-10）
│     ├── 单条失败：跳过，继续后续，失败的可单独重试
│     ├── 失败重试：最多 2 次，指数退避
│     └── 结果实时落库，不等全部完成
└── 4. 完成后状态栏汇报：
      "分析 48 条：32 有价值 / 16 跳过，消耗约 ¥0.08"
```

**批量分析进度展示（双轨）：**
- **底部全局进度条**：显示"正在分析 12 / 48"，支持取消
- **列表内每条实时更新**：转圈（分析中）→ ✨ 完成 / ❌ 失败
- 中途关闭应用：已完成的保留，下次启动可续跑未完成的

#### 3.2.3 提炼产出物

**①知识摘要（Summary）**
- 用 2-5 句话概括这次对话解决了什么问题
- 包含关键技术点、解决方案要点

**② 知识标签（Tags）**

自动生成多维度标签：
- 技术栈标签：`Python` / `TypeScript` / `React` / `Rust`...
- 问题类型标签：`Bug修复` / `架构设计` / `性能优化` / `工具配置`...
- 难度标签：`入门` / `进阶` / `深度`
- 自定义标签（用户可手动添加）

**③ 知识卡片（Knowledge Card）**

核心产出。大模型自动判断一个 session 包含几个独立知识点，每个知识点生成一张卡片（一对多关系）。用户可手动合并/拆分卡片。

**卡片格式示例：**

```
## [卡片标题]

**问题**：xxx

**解决方案**：
- 步骤1
- 步骤2

**关键代码/命令**：
（代码块）

**适用场景**：xxx
**注意事项**：xxx

---
来源：Cursor | 项目：myproject | 时间：2026-03-19
```

**④ 自动分类（Category）**

知识卡片自动归入分类树：

```
知识库/
├── 语言与框架/
│   ├── Python/
│   ├── TypeScript/
│   └── ...
├── 工程实践/
│   ├── 架构设计/
│   ├── 性能优化/
│   └── ...
├── 工具与配置/
├── Bug & 排错/
└── 未分类/
```

---

#### 3.2.4 提示词设计（来自问渠借鉴）

> **参考来源**：问渠（https://git.woa.com/ti-ai/wenqu）是腾讯内部同类产品，已实测验证以下提示词在实际工程中的效果。以下两套提示词可作为有得 Distiller 的起点，可在此基础上迭代优化。

---

**Prompt A：Memory + Skill 双轨制提炼**

适用场景：快速提炼，判断知识价值，同时识别可复用操作流程。

产出：Memory 条目（给 AI 直接复用）+ Skill 模板（标准操作流程）

```
你是一个专业的技术经验提炼助手。请从下面这段 AI 编程对话中提炼出 Memory 条目，并判断是否适合生成 Skill 模板。

## 一、Memory

Memory 的目标：让 AI 助手在未来对话中直接利用这段经验，少走弯路、不重复踩坑。

**判断是否值得写入 Memory：**
- 有价值：踩到了坑并解决、确定了技术决策/约定、发现了工具/框架的关键行为
- 无价值：纯问答型学习、没有落地结论的讨论、泛泛的概念解释
- 无价值时 `has_memory: false`，`content` 为空字符串

**Memory 格式（has_memory 为 true 时）：**
- 用简洁 Markdown bullet，每条一句话直接说结论
- 开头用 `## <技术领域>` 分类（如 `## Python / FastAPI`、`## Git`）
- 每条不超过两行，如有代码用 inline code
- 总长度 200 字以内

## 二、Skill

Skill 的目标：把**可复用的排查/操作流程**做成可触发的指令模板，让 AI 按步骤带用户执行。

**判断是否值得生成 Skill（skill_worthy）：**
- 适合：对话中包含可复用的排查步骤、诊断流程、标准操作序列（如"排查内存泄漏"、"定位慢查询"）
- 不适合：一次性的根因结论（如"这个 bug 是因为版本 X 的问题"）、纯概念学习

**Skill 模板格式（skill_worthy 为 true 时）：**
---
description: 一句话描述这个 skill 的用途
---

# <Skill 名称>

## 使用场景
什么情况下触发这个 skill

## 执行步骤
1. 第一步：...
2. 第二步：...
3. ...

## 注意事项
- 关键注意点

请按以下 JSON 格式返回（只返回纯 JSON，不含其他内容）：
{
  "has_memory": true,
  "reason": "一句话说明为什么值得/不值得写入 Memory",
  "content": "Markdown 格式的 Memory 内容，has_memory 为 false 时为空字符串",
  "category": "技术领域分类（如 Python、Git、Docker、SQL 等）",
  "skill_worthy": false,
  "skill_reason": "一句话说明为什么值得/不值得生成 Skill",
  "skill_template": "Skill 的完整 Markdown 模板，skill_worthy 为 false 时为空字符串"
}

对话内容：
---
{content}
---
```

---

**Prompt B：深度技术笔记提炼**

适用场景：对高价值对话做完整知识笔记，内容详细、可独立阅读。

产出：结构化技术笔记（按 debug / research / implementation / optimization / learning 五种类型分别输出对应结构）

```
你是一个专业的技术知识库整理助手。请深度分析下面这段 AI 编程对话，提炼出一篇完整的技术笔记。

## 第一步：价值判断

以下情况直接判定为无价值（value: "none"）：
- 对话中只有问题/指令，没有实质性的回答或解决过程
- 纯闲聊、问天气、问时事、问非技术性内容
- 只是让 AI 执行简单的格式转换/翻译等，无技术含量
- 对话极短（少于3轮）且没有实质内容

低价值（value: "low"）：
- 问题过于简单，答案是常识或一句话能说清楚
- 内容高度重复，没有新知识点

## 第二步：识别对话类型

- **debug**：遇到报错/bug，经过排查最终解决
- **research**：调研某个技术、工具、方案的对比选型
- **implementation**：实现某个功能、模块、系统
- **optimization**：性能优化、重构、代码改进
- **learning**：学习理解某个概念、原理、框架用法
- **other**：其他有价值的技术对话

## 第三步：生成详细技术笔记

**这是最重要的步骤。** note 字段必须是一篇完整、独立、有深度的技术笔记，读者无需看原始对话即可完全理解并参考使用。

笔记要求：
- **详细**：充分展开技术细节，不要省略关键步骤
- **具体**：完整保留对话中的真实代码片段、命令、报错信息、配置内容、文件路径等
- **可操作**：让读者能根据笔记独立复现或参考实施
- **篇幅**：中等复杂度问题 400-600 字，复杂实现/debug 800 字以上，不设上限

根据类型使用对应结构：

**debug 类**：
- `## 问题描述`：完整描述问题现象、具体报错信息（原文）、复现步骤
- `## 根本原因`：深入分析为什么会出现这个问题，原理层面解释
- `## 解决方案`：完整的修复步骤，包含具体代码、命令或配置变更
- `## 关键收获`：从这个 bug 学到的经验、易踩的坑、如何预防

**research 类**：
- `## 调研背景`：为什么需要做这个调研，当前的痛点是什么
- `## 方案对比`：每个方案的详细分析，各自优缺点，适用场景
- `## 结论与选型`：最终选什么，选择理由，潜在风险
- `## 关键细节`：实施要点、配置示例、注意事项

**implementation 类**：
- `## 实现目标`：要实现什么功能，输入输出，约束条件
- `## 核心思路`：整体设计思路、技术选型、架构决策及其理由
- `## 关键实现`：核心代码逻辑，附完整的关键代码片段，解释每段代码的作用
- `## 注意事项`：踩过的坑、边界情况、已知限制、后续优化方向

**optimization 类**：
- `## 优化背景`：原来的问题是什么，性能数据，瓶颈表现
- `## 分析过程`：如何定位瓶颈，用了什么工具，分析思路
- `## 优化方案`：具体的优化手段，附优化前后代码对比
- `## 效果对比`：优化前后的数据，提升幅度

**learning 类**：
- `## 核心概念`：要理解的是什么，存在的背景和问题
- `## 关键原理`：深入解释原理，配合示例说明
- `## 使用方式`：具体用法，附代码示例，常见参数说明
- `## 实践总结`：实际使用中的注意点、最佳实践、与其他方案的区别

请按以下 JSON 格式返回（只返回纯 JSON，不含其他内容）：

{
  "title": "简洁准确的标题（20字以内）",
  "type": "debug|research|implementation|optimization|learning|other",
  "value": "none|low|medium|high",
  "value_reason": "如果 value 为 none 或 low，说明原因（1句话）",
  "summary": "一句话概括这段对话解决了什么问题（40字以内）",
  "note": "完整的 Markdown 笔记（value 为 none 时为空字符串）",
  "tags": ["标签1", "标签2", "标签3"],
  "tech_stack": ["技术1", "技术2"]
}

注意：
- note 必须详细充实，像一篇真正的技术博客文章，而不是摘要
- 代码片段用 markdown 代码块包裹并注明语言
- tags 5个以内，tech_stack 列出涉及的技术/框架/工具/语言
- value 为 none 时 note 为空字符串，其余字段仍正常填写

对话内容：
---
{content}
---
```

---

**两套 Prompt 的差异与选用建议：**

| 维度 | Prompt A（Memory + Skill） | Prompt B（深度笔记） |
|------|---------------------------|---------------------|
| 产出形式 | 简洁 bullet 条目 + Skill 模板 | 完整技术笔记文章 |
| 适合场景 | 快速积累、AI 直接复用 | 知识沉淀、人工查阅 |
| 篇幅 | 紧凑（200 字以内） | 详细（400-800+ 字） |
| token 消耗 | 较少 | 较多 |
| **推荐策略** | 默认全量运行 | 仅对 value=high 的对话运行 |

**实现建议：** 两套 Prompt 串行执行——先跑 Prompt B 做价值判断，仅对 `value=medium/high` 的对话再跑 Prompt A 生成 Memory/Skill 条目，避免低价值对话浪费 token。

---

**前处理：清理 AI 内部数据**

提炼前需要清理对话中的 AI 内部数据，这些内容对知识提炼无价值且浪费 token：

```python
# 需要从对话内容中剥离的模式
patterns = [
    r"<thinking>[\s\S]*?</thinking>",           # Extended thinking 推理过程
    r"<antml_function_calls>[\s\S]*?</antml_function_calls>",  # Anthropic 工具调用 XML
    r"<function_calls>[\s\S]*?</function_calls>",              # 通用工具调用 XML
    r"<tool_use>[\s\S]*?</tool_use>",
    r"<tool_call>[\s\S]*?</tool_call>",
    r"<tool_result>[\s\S]*?</tool_result>",
]
```

**超长对话截断策略（避免超出上下文限制）：**

```python
def truncate_for_ai(content: str, max_chars: int = 12000) -> str:
    if len(content) <= max_chars:
        return content
    # 保留开头 8000 + 结尾 4000，保留问题背景和最终结论
    return content[:8000] + "\n\n[... 中间内容已省略 ...]\n\n" + content[-4000:]
```

---

#### 3.2.5 大模型配置
- 提炼 Prompt 可用户自定义（在以上两套模板基础上修改）
- 提炼质量支持用户反馈（👍/👎），用于后续 Prompt 优化
- 模型提供商详见 3.2.1 Distiller 提供商配置

---

### 3.3 知识存储层（Storage）

**存储方案：** 本地 SQLite + 文件系统

```
~/.chattake/
├── db/
│   └── chattake.db          # 主数据库（SQLite）
├── cards/
│   └── YYYY-MM/
│       └── <card-id>.md  # 知识卡片 Markdown 文件
├── sessions/
│   └── <source>/
│       └── <session-id>.json  # 原始对话归档
└── config.toml           # 用户配置
```

**数据库核心表设计：**

- `sessions`：原始对话记录元数据
- `knowledge_cards`：知识卡片
- `tags`：标签
- `card_tags`：卡片-标签关联
- `categories`：分类树
- `sync_state`：各数据源同步状态

---

### 3.4 检索与展示层（Explorer）

#### 3.4.1 搜索能力
- **全文搜索**：基于 SQLite FTS5，支持中英文分词
- **语义搜索**：本地向量嵌入（可选，需配置 embedding 模型）
- **标签筛选**：多标签组合过滤
- **分类浏览**：树形分类导航
- **时间线视图**：按时间倒序浏览知识积累

#### 3.4.2 知识卡片展示
- Markdown 渲染
- 代码高亮
- 一键复制代码块
- 显示来源对话（可跳转查看原始对话）
- 相关卡片推荐（基于标签/语义相似度）

#### 3.4.3 统计 Dashboard
- 知识积累趋势图（按天/周）
- 各数据源贡献占比
- 热门标签词云
- 技术栈分布

---

### 3.5 导出与开放能力层（Export & API）

#### 3.5.1 导出格式
- **Markdown**：单卡/批量导出 `.md` 文件
- **Obsidian Vault**：直接生成兼容 Obsidian 的目录结构（含双向链接）
- **JSON**：结构化数据导出，方便程序化处理
- **PDF**：知识报告导出

#### 3.5.2 开放能力（给 AI 使用）

**MCP Server 模式（核心！）**

有得作为本地 MCP Server，提供以下工具给 AI IDE 调用：

```
tools:
  - chattake_search(query, tags?, limit?)     # 语义检索知识库
  - chattake_get_card(card_id)                # 获取完整知识卡片
  - chattake_list_tags()                      # 列出所有标签
  - chattake_add_note(title, content, tags?)  # 快速添加笔记到知识库
```

这样 Cursor/Claude Code 在写代码时，可以直接检索有得的本地知识库，实现**"AI 用你自己积累的知识帮你写代码"**的闭环。

**REST API 模式（可选）**

本地 HTTP 服务（默认 `localhost:7399`），供其他工具/脚本调用。

---

## 四、用户界面（UI/UX）

### 4.1 整体布局

三栏结构：左侧导航树 + 中间会话/卡片列表 + 右侧详情面板。

```
┌──────────────────────────────────────────────────────────────┐
│  🔍 搜索框                                    同步🔄  ⚙️设置  │
├────────────┬─────────────────────────┬────────────────────────┤
│            │                         │                        │
│ 📱 Cursor  │  会话列表 / 知识库       │  详情面板              │
│  ├ chattake   │                         │  （原始对话 or 卡片）   │
│  └ myproj  │                         │                        │
│            │                         │                        │
│ 🤖 Claude  │                         │                        │
│  └ chattake   │                         │                        │
│            │                         │                        │
│ 🛠 CodeBuddy│                         │                        │
│  └ myproj  │                         │                        │
│            │                         │                        │
│ ──────────  │                         │                        │
│ 📚 知识库   │                         │                        │
│            ├─────────────────────────┤                        │
│            │ [全局进度条 12/48 ████░] │                        │
└────────────┴─────────────────────────┴────────────────────────┘
```

### 4.2 左侧导航树

**三层结构：应用 → 项目 → （展开后显示会话列表）**

```
├── Cursor (20)
│   ├── chattake (12)
│   └── my-project (8)
├── Claude Code (34)
│   ├── chattake (34)
│   └── openclaw (6)
└── CodeBuddy (15)
    └── my-project (15)

────────────────
📚 知识库
```

- 括号内显示该节点下会话总数
- 点击项目名 → 中间列表过滤到该项目
- 点击「知识库」→ 切换到卡片视图

### 4.3 会话列表视图（首页默认）

**每条会话卡片展示：**

```
┌─────────────────────────────────────────────────────┐
│ ☑  📁 chattake  │  修复 WebSocket 断连重连问题           │
│              │  🏷 debug  🔥 high   03-19  Claude    │
│              │  💬 42条消息                           │
├─────────────────────────────────────────────────────┤
│ ☑  📁 chattake  │  ⚠️ 会话有更新，可重新分析             │  ← 黄色角标
│              │  🏷 implementation  ✅ 已分析  03-18   │
├─────────────────────────────────────────────────────┤
│ ☑  📁 myproj │  实现批量并发分析逻辑                  │
│              │  （未分析，灰色显示）         03-17     │
└─────────────────────────────────────────────────────┘
```

**状态视觉规范：**

| 状态 | 视觉表现 |
|------|---------|
| 未分析 | 灰色文字，无标签 |
| 分析中 | 转圈动画，`正在分析…` |
| 已分析（high/medium） | 正常色，显示 type 标签 + value 徽章 |
| 已分析（low/none） | 略灰，显示 `跳过` 标签 |
| 会话有更新 | ⚠️ 黄色角标，`可重新分析` 提示 |
| 分析失败 | ❌ 红色角标，可单条重试 |

**批量操作工具栏（勾选后出现）：**

```
✅ 已选 12 条  [✨ 批量分析]  [🔄 重新分析]  [取消]
```

### 4.4 知识库卡片视图

默认列表，可切换网格。

**列表模式：**

```
┌────────────────────────────────────────────────────────┐
│ [全部] [debug] [implementation] [research] ...  🔍搜索  │
│                              [≡列表] [⊞网格]           │
├────────────────────────────────────────────────────────┤
│ WebSocket 断连重连机制          debug · Python · 03-19  │
│ 一句话摘要：使用指数退避策略重连，避免服务端雪崩…         │
├────────────────────────────────────────────────────────┤
│ SQLite FTS5 中文全文搜索        implementation · 03-18  │
│ 一句话摘要：jieba 分词 + FTS5 虚拟表实现中文检索…        │
└────────────────────────────────────────────────────────┘
```

**网格模式：**

```
┌───────────────────┐  ┌───────────────────┐
│ WebSocket 断连重连 │  │ SQLite FTS5 搜索   │
│ debug · Python    │  │ implementation    │
│ 一句话摘要…        │  │ 一句话摘要…        │
│ 03-19             │  │ 03-18             │
└───────────────────┘  └───────────────────┘
```

### 4.5 卡片详情 / 原始对话面板（右侧）

点击会话或卡片后，右侧面板展开，支持两个 Tab 切换：

**Tab 1：知识笔记**

```
┌──────────────────────────────────────────────────────┐
│ [知识笔记] [原始对话]          👍 👎  📋复制  ⬇导出  │
├──────────────────────────────────────────────────────┤
│ WebSocket 断连重连机制                                 │
│ 🏷 debug  Python  WebSocket  ⭐ high                  │
│                                                      │
│ ## 问题描述                                           │
│ ...                                                  │
│ ## 解决方案                                           │
│ ```python                                            │
│ ...                                                  │
│ ```                                                  │
│                                                      │
│ ──────────────────────────────────────               │
│ 📅 2026-03-19  💬 来自 Claude Code / chattake            │
│ [✨ 重新分析]                                         │
└──────────────────────────────────────────────────────┘
```

**Tab 2：原始对话（气泡回放）**

```
┌──────────────────────────────────────────────────────┐
│ [知识笔记] [原始对话]                                  │
├──────────────────────────────────────────────────────┤
│                                                      │
│  👤 User  14:23                                      │
│  ┌─────────────────────────────────────┐             │
│  │ WebSocket 连接经常断掉，重连逻辑怎么写？│             │
│  └─────────────────────────────────────┘             │
│                                                      │
│       🤖 Assistant  14:23                            │
│       ┌────────────────────────────────────────┐    │
│       │ 推荐使用指数退避策略...                   │    │
│       │ ```python                               │    │
│       │ ...                                     │    │
│       │ ```                                     │    │
│       └────────────────────────────────────────┘    │
│  [工具调用 ▶ 折叠]                                    │
└──────────────────────────────────────────────────────┘
```

- `<thinking>` 块和工具调用默认折叠，可展开查看
- 代码块支持一键复制

---

## 五、技术架构

### 5.1 技术选型

| 层级 | 技术 | 理由 |
|------|------|------|
| 客户端框架 | **Tauri 2.0** | 轻量跨平台，macOS/Windows/Linux，包体小 |
| 前端 UI | **Vue 3 + TypeScript** | Composition API 简洁，对中文开发者友好，vibe coding 友好 |
| UI 组件库 | **Naive UI + UnoCSS** | Vue 生态下开箱即用的高质量组件库，内置主题系统，组件丰富 |
| 后端逻辑 | **Rust（Tauri 内核）** | 文件操作、SQLite、性能 |
| AI 调用 | **Python sidecar** | 大模型 API 调用、向量嵌入，Python 生态更成熟 |
| 数据库 | **SQLite（rusqlite）** | 本地轻量，无需部署 |
| 全文搜索 | **SQLite FTS5** | 内置，无额外依赖 |
| 向量搜索 | **sqlite-vss / hnswlib** | 可选，语义搜索 |
| MCP Server | **Python（FastMCP）** | MCP 协议实现 |

### 5.2 系统架构图

```
┌─────────────────────────────────────┐
│         Tauri App（桌面客户端）       │
│  ┌──────────┐  ┌──────────────────┐ │
│  │ Vue UI  │  │   Rust 后端       │ │
│  │          │←→│  - 文件 Watch     │ │
│  │          │  │  - SQLite 操作    │ │
│  │          │  │  - 配置管理       │ │
│  └──────────┘  └────────┬─────────┘ │
└───────────────────────── │ ──────────┘
                           │ IPC
              ┌────────────▼──────────┐
              │   Python Sidecar      │
              │  - LLM API 调用       │
              │  - 知识提炼 Pipeline  │
              │  - 向量嵌入计算       │
              │  - MCP Server         │
              └───────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
   ~/.claude/        Cursor SQLite      CodeBuddy SQLite
   (JSONL)           (workspaceStorage) (workspaceStorage)
```

---

## 六、版本规划

### v0.1 - MVP（目标：4周）

**核心功能：**
- [ ] Claude Code 数据采集（JSONL 解析）
- [ ] Cursor 数据采集（SQLite 解析）
- [ ] 基础知识提炼（摘要 + 标签）
- [ ] 知识卡片生成与存储
- [ ] 基础全文搜索
- [ ] 简单卡片列表 UI

**交付标准：** 能跑起来，自用不报错

---

### v0.2 - 体验完善（目标：+3周）

- [ ] CodeBuddy 数据采集
- [ ] 自动分类树
- [ ] 标签系统完善
- [ ] 统计 Dashboard
- [ ] Obsidian 导出
- [ ] 系统托盘 + 后台静默同步

---

### v0.3 - 开放能力（目标：+3周）

- [ ] MCP Server 实现
- [ ] 语义搜索（向量嵌入）
- [ ] 相关卡片推荐
- [ ] 知识卡片编辑器
- [ ] 多 LLM 提供商配置

---

### v1.0 - 正式版

- [ ] Windows 支持
- [ ] 更多数据源（Windsurf、Copilot 等）
- [ ] 知识卡片版本历史
- [ ] 知识图谱可视化
- [ ] 插件/扩展机制

---

## 七、关键风险与对策

| 风险 | 概率 | 影响 | 对策 |
|------|------|------|------|
| IDE 更新导致存储格式变化 | 高 | 中 | 各数据源解析器模块化，独立维护 |
| 本地大模型质量差，提炼效果不佳 | 中 | 高 | 支持多 LLM 配置，提供 Prompt 调优界面 |
| SQLite 并发读写冲突 | 低 | 中 | WAL 模式 + 队列化写入 |
| 用户隐私顾虑（对话内容上传云端） | 中 | 高 | 默认本地模型，远程 API 需明确授权，数据不离本地 |
| Tauri 跨平台兼容问题 | 低 | 中 | 优先 macOS，Windows 在 v1.0 跟进 |

---

## 八、待讨论 / 开放问题

- [x] **知识卡片的粒度**：~~一个 session = 一张卡，还是一个 session 可拆分成多张卡？~~ **已决策：一个 session 可拆分成多张卡，大模型自动判断知识点数量，用户可手动合并/拆分。**
- [x] **提炼时机**：~~实时提炼（对话结束即触发）vs 批量提炼（每晚定时）？~~ **已决策：启动时触发（静默/询问/手动三种模式可配置）+ 可选 macOS Launch Agent 后台采集。**
- [ ] **知识老化处理**：某个技术方案过时了，卡片如何标记/更新？
- [ ] **多设备同步**：未来是否考虑通过 iCloud / Git 同步知识库？
- [ ] **社区功能**：未来是否支持匿名分享优质知识卡片？

---

## 九、参考项目

| 项目 | 链接 | 说明 |
|------|------|------|
| **问渠（wenqu）** | https://git.woa.com/ti-ai/wenqu | 腾讯内部同类产品。Python + FastAPI + 纯前端架构，支持 Cursor / Claude Code / CodeBuddy，已验证路径扫描规则、提示词设计、AI 内容前处理等核心逻辑 |

---

*文档持续迭代中。*
