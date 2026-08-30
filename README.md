# 有得 · ChatTake

> 让每次 AI 对话，都有所得
> ChatTake — Turn AI chats into reusable knowledge

## 简介

有得是一款本地优先的 AI 对话知识库。它增量采集 Claude Code、Cursor、Codex、CodeBuddy、OMP、Pi 的本地对话，经用户确认后通过 API 或本地 CLI Provider 提取原子知识，并提供草稿治理、全文检索和只读 MCP 回溯。

v0.2.0 采用固定的五类知识（决策、排障、实现、解释、片段）与开放标签，避免 AI 无限生成分类。高价值知识自动发布，中价值进入草稿，重新分析不会覆盖旧知识。

## 技术栈

- **桌面框架**: Tauri 2.0
- **前端**: Vue 3 + TypeScript + UnoCSS + Naive UI + Pinia
- **后端**: Rust (rusqlite, serde, tokio)
- **LLM Sidecar**: TypeScript + Bun（OpenAI-compatible API / 本地 CLI）
- **MCP Server**: 官方 TypeScript SDK v2 + Bun（只读 stdio）

## 项目结构

```
chattake/
├── CHANGELOG.md           # 更新日志（唯一维护源，关于页经 sync:changelog 同步）
├── scripts/               # 根目录脚本（如 sync-changelog.mjs）
├── apps/desktop/          # Tauri 桌面应用
│   ├── src-tauri/         # Rust 后端
│   └── src/               # Vue 3 前端
├── packages/
│   ├── sidecar/           # TS Sidecar（LLM 调用 + 知识提炼）
│   ├── mcp-server/        # MCP Server（AI IDE 集成）
│   └── shared/            # 共享类型和工具
└── docs/                  # 文档
```

## 开发

```bash
# 安装依赖
bun install

# 更新日志：编辑根目录 CHANGELOG.md 后同步到关于页数据源（构建 tauri 前也会自动执行）
bun run sync:changelog

# 启动开发模式
bun --cwd apps/desktop run tauri dev

# 验证 Sidecar、MCP、Rust 与前端
bun run verify

# 统一 debug 构建（先编译伴随程序、运行 Rust 测试，再执行 Tauri）
bun run build:debug
```

## 配置

配置默认保存到 `~/.chattake/config.toml`。可同时保存 OpenAI-compatible API 与 Claude Code、Codex、Cursor、OMP、Pi、CodeBuddy CLI 配置；每个分析任务只绑定启动时选择的一套 Provider，不自动切换。完整示例见 [`docs/config.example.toml`](docs/config.example.toml)。

首次运行 v0.2.0 时，如果检测到旧 Schema，会先通过 SQLite `VACUUM INTO` 备份到 `~/.chattake/db/backups/`，备份成功后才重建数据库。

## MCP

先执行 `bun run build:companions`，然后在应用「设置 → MCP」复制配置片段。`chattake-mcp` 只读取已发布知识；草稿、任务、API Key 和供应商配置不会通过 MCP 返回。

## 许可证

MIT
