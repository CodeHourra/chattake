# 寻迹 —— 常用构建入口（macOS 为主，需已安装 Bun / Rust / Xcode CLT）
# 使用：在仓库根目录执行 `make macos` 或 `make help`

REPO := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))

.PHONY: help deps sidecar mcp companions desktop-frontend macos

help:
	@echo "目标说明："
	@echo "  make deps     - bun install（workspace）"
	@echo "  make sidecar  - 仅编译 packages/sidecar → dist/xunji-sidecar"
	@echo "  make mcp      - 仅编译 packages/mcp-server → dist/xunji-mcp"
	@echo "  make companions - 编译 Sidecar 与 MCP"
	@echo "  make macos    - 依赖 + 打 macOS 安装包（.app + .dmg，内含 Sidecar/MCP）"
	@echo "产物：apps/desktop/src-tauri/target/release/bundle/dmg/寻迹_<版本>_<arch>.dmg"

deps:
	@cd "$(REPO)" && bun install

sidecar:
	cd "$(REPO)/packages/sidecar" && bun run build

mcp:
	cd "$(REPO)/packages/mcp-server" && bun run build

companions: sidecar mcp

# 一键：安装依赖并执行 Tauri 发布构建（beforeBuild 会编译 Sidecar/MCP + 前端并打入资源）
# createUpdaterArtifacts 需要 minisign 私钥：优先用环境变量 TAURI_SIGNING_PRIVATE_KEY，否则从 apps/desktop/xunji.updater.key 读取（勿提交该文件）
macos: deps
	@cd "$(REPO)/apps/desktop" && \
		if [ -z "$$TAURI_SIGNING_PRIVATE_KEY" ] && [ -f xunji.updater.key ]; then \
			export TAURI_SIGNING_PRIVATE_KEY="$$(cat xunji.updater.key)"; \
		elif [ -z "$$TAURI_SIGNING_PRIVATE_KEY" ]; then \
			echo "错误: 未设置 TAURI_SIGNING_PRIVATE_KEY，且当前目录无 xunji.updater.key。请生成密钥或 export，见 docs/桌面应用-Release与自动更新.md" >&2; \
			exit 1; \
		fi; \
		env -u CI bun run tauri:build

# 一键：安装依赖并执行 Tauri 开发构建（beforeDev 会编译 sidecar + 前端）
dev:
	cd "$(REPO)/apps/desktop" && bun run tauri dev

# 清除构建产物
clean:
	cd "$(REPO)/apps/desktop" && rm -rf src-tauri/target
	cd "$(REPO)/packages/sidecar" && rm -rf dist
	cd "$(REPO)/packages/mcp-server" && rm -rf dist
	cd "$(REPO)/packages/shared" && rm -rf dist
