# 踩坑：桌面会话提炼（distill）— JSON 解析、价值判断、标签与技术栈

本文汇总在 **Tauri 桌面 + sidecar + OpenAI 兼容 API** 上做会话提炼时遇到的问题：**JSON 解析失败**、**judge 与 distill 结果 seemingly 不一致**、**标签过多**、**技术栈侧栏无数据**等，并对应 **原因** 与 **已实现/建议的解决办法**。涉及 `packages/sidecar` 的 distiller、`apps/desktop/src-tauri` 的 `sessions`、`storage/cards` 与知识库侧栏。

---

## 一、`distill_full` 报 JSON 解析失败（Unterminated string / 策略 1～4 全失败）

### 现象

- Sidecar 日志：`策略1（直接解析）失败: Unterminated string`，或后续策略报 `Unrecognized token '/'` 等。
- 完整响应里外层常有 `` ```json `` 包裹，剥掉后仍无法 `JSON.parse`。

### 根因

- 要求模型在**单个 JSON 字符串字段** `note` 里写长 Markdown，且含 **fenced 代码块**（` ``` `）。模型若在块内输出**未转义的双引号**或破坏字符串边界，整段即**非法 JSON**。
- 策略 3 用正则修补 `note` 等字段时，若内容里已有未转义 `"`，非贪婪匹配可能**截断过早**，反而加重解析失败。

### 解决方法（已实现）

- **提示词**（`prompts.ts`）：强调 `note` 必须是合法 JSON 字符串，双引号、反斜杠、换行需按 JSON 转义；代码示例优先单引号。
- **`extractJson`**：多策略解析 + 失败时的**诊断日志**（位置、`"note"` 片段等）。
- **载荷日志**（`payload-log.ts` + `api-provider`）：记录与 HTTP 一致的 system/user，便于确认模型实际收到的正文是否被截断。

---

## 二、会话「明明有内容」却仍被标为 LOW，或 judge 与 distill 结论看起来矛盾

### 现象

- UI 里会话详情已有很长分析，**重新分析**仍判 **LOW**。
- 或 judge 为 medium/high，但用户感觉与「可见内容」不符。

### 根因

- **价值判断**与**全文提炼**共用同一套 **preprocess**（`clean` + `truncate`）。若正文被截断，**关键结论若只出现在被省略的中间段**，模型看不到，易判低价值。
- **展示内容**可能来自**历史卡片 / 折叠 UI**，而 **DB `messages` 转写**里若没有对应 assistant 文本，送给模型的 `content` 里**根本没有**那段话。
- 此前若把 **tool 角色**的整段输出拼进 transcript，会占满 token；后改为**只保留 user / assistant / model**，纯工具回传不再进入——若根因只在 tool 里且未被助手复述，判断也会偏。

### 解决方法（已实现）

- **Rust `build_transcript`**：仅拼接 **user、assistant、model**；**tool** 与空正文跳过（见 `commands/sessions.rs`）。
- **排查**：对比日志中的 **RPC 正文长度 / md5** 与 sidecar **preprocess 后**的 `user.md5`，确认是否被 **truncate** 截断。

---

## 三、开发环境如何看到「实际发给模型的 HTTP 正文」

### 现象

- 需要确认 judge / distill 时 **system、user** 各多长、内容头尾是什么。

### 根因

- 默认只打摘要，全文可能很长且含隐私。

### 解决方法（已实现）

- 环境变量 **`CHATTAKE_LOG_DISTILL_PAYLOAD=1`**：打印完整 system + user（sidecar stderr）。
- **开发构建**（`tauri dev` / `debug_assertions`）：若在进程环境中**未设置**该变量，桌面应用在启动时会**默认设为 `1`**；若需关闭，启动前设置 **`CHATTAKE_LOG_DISTILL_PAYLOAD=0`**（已存在则不覆盖）。

---

## 四、跨进程串联日志：`traceId`

### 现象

- Rust、sidecar、浏览器多处日志，难以对应**同一次**提炼。

### 根因

- 无统一 id。

### 解决方法（已实现）

- 每次 `distill_session` 在 Rust 生成 **UUID**，经 JSON-RPC 传入 **`traceId`**，sidecar 所有 `[distiller]` 日志带 **`[distiller][traceId=…]`**。
- 前端 **`DistillSessionResult.traceId`**；开发环境下队列成功时控制台会打印，便于与终端 grep。

---

## 五、标签「爆炸」、侧栏技术栈一直为空

### 现象

- 知识库侧栏 **标签**极多，且存在 **Rust / rust** 等近重复。
- **技术栈**区域长期无数据；或笔记头里也没有技术栈。

### 根因

1. **标签**：模型一次返回过多 tag；**tags 表**按**字符串精确**建唯一，大小写不同即两条；历史数据不会自动合并。
2. **技术栈**：
   - 早期侧栏仅为**占位文案**，**未读 `cards.tech_stack` 列**，因此「永远没有聚合数据」。
   - 若 LLM 未输出 **`tech_stack`** 或 JSON 解析丢字段，入库列为空，聚合也为空。
   - **旧卡片**在加列前产生，需**重新提炼**才会写入 `tech_stack`。

### 解决方法（已实现）

- **Sidecar**（`tech-tags.ts`）：对 `tags` **最多 5 条**、`tech_stack` **最多 8 条**，trim、**按小写去重**、保留首次出现的展示写法。
- **提示词**：限制 tag 数量与语义，避免与 `tech_stack` 重复堆砌。
- **Rust**：`insert_card` 日志中带 **`tech_stack列='…' (N段)`**；`distill_full` 解析后打 **tags / tech_stack**。
- **侧栏**：新增 **`list_tech_stack_counts`**，从 **`cards.tech_stack` 逗号列**拆分聚合（展示名按首次出现，统计按小写合并）；前端知识库 Tab 加载时与标签一并拉取。
- **UI**：标签区增加 **最大高度 + 滚动**，避免一屏占满。

### 排查关键词（终端 / sidecar stderr）

| 目的 | 建议 grep / 关注 |
|------|------------------|
| 模型是否返回 `tech_stack` | `[distiller][traceId=` 且含 **`normalize: tech 相关原始值`** |
| 归一化后是否仍为空 | 同 trace 下 **`normalize: tech_stack 归一化后`** |
| Rust 侧是否收到非空数组 | **`trace_id=`** + **`distill_full 解析入库`** |
| 是否写入 SQLite | **`创建卡片:`** + **`tech_stack列=`** |

更细的说明见 **`packages/sidecar/src/distiller/tech-tags.ts`** 文件头注释。

---

## 六、相关代码位置（便于二次开发）

| 模块 | 路径 |
|------|------|
| 转写与提炼管道 | `apps/desktop/src-tauri/src/commands/sessions.rs` |
| 卡片入库与标签 / tech_stack 列 | `apps/desktop/src-tauri/src/storage/cards.rs` |
| 侧栏技术栈聚合 | `list_all_tech_stack_counts`（同上） |
| 侧栏命令 | `apps/desktop/src-tauri/src/commands/sidebar.rs` |
| 提示词与 JSON 约定 | `packages/sidecar/src/distiller/prompts.ts` |
| 预处理与 JSON 提取 | `packages/sidecar/src/distiller/index.ts` |
| 标签/技术栈归一化 | `packages/sidecar/src/distiller/tech-tags.ts` |
| HTTP 载荷日志 | `packages/sidecar/src/distiller/payload-log.ts` |
| trace 与统一日志前缀 | `packages/sidecar/src/distiller/trace.ts` |
| 开发环境默认 `CHATTAKE_LOG_DISTILL_PAYLOAD` | `apps/desktop/src-tauri/src/lib.rs`（`run()` 开头） |

---

## 七、建议操作顺序（遇到问题时）

1. 记下本次提炼的 **`traceId`**（前端控制台或 Rust 日志 `distill 开始 trace_id=`）。
2. 在 **sidecar stderr** 用 `traceId` 过滤 **`normalize:`**、**`API 即将发送`**、**`Response:`**。
3. 在 **Rust 日志** 搜同一 **`trace_id=`** 的 **`distill_full 解析入库`** 与 **`创建卡片:`**。
4. 若 **`tech_stack` 列为空**，对目标会话 **重新执行提炼**；仍空则回到步骤 2 看模型原始 JSON 是否含 **`tech_stack`** 键。

---

*文档随实现迭代，若与代码不一致请以仓库为准。*
