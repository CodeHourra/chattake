# macOS 打包说明（寻迹 / Tauri）

本文说明如何在 **Apple Silicon（aarch64）** 或 **Intel（x86_64）** 的 macOS 上从源码打出可安装的 `.app` / `.dmg`，且 **安装 DMG 后即可使用提炼等 Sidecar 能力**（无需再手动拷贝 `xunji-sidecar`）。

## 环境要求

| 工具 | 说明 |
|------|------|
| **Xcode Command Line Tools** | `xcode-select --install` |
| **Rust** | 建议 [rustup](https://rustup.rs/)，`rustc` 能正常编译即可 |
| **Bun** | 安装 workspace 依赖、执行 `beforeBuildCommand`、编译 Sidecar |
| **GNU Make**（可选） | 用于 `make macos` 一键构建 |

版本号不锁死，与仓库当前 `package.json` / `Cargo.toml` 兼容即可。

## 一键构建（推荐）

在仓库根目录 `xunji/`：

```bash
make macos
```

等价于：`bun install` + 在 `apps/desktop` 执行 `env -u CI bun run tauri:build`（会按 `tauri.conf.json` 先编 **Sidecar**、再编前端，并把 `xunji-sidecar` 打进应用包资源目录）。

> 部分环境存在 `CI=1` 导致 Tauri CLI 报错，Makefile / `tauri:build` 已用 `env -u CI` 规避。

## 手动步骤（与 Make 一致）

```bash
# 1. 安装依赖
bun install

# 2. 打包（内部会：编译 sidecar → 编译前端 → 打 .app/.dmg）
cd apps/desktop && bun run tauri:build
```

无需再单独执行 `cd packages/sidecar && bun run build`，除非你想只更新 sidecar 二进制做调试。

## 产物位置

构建成功后：

`apps/desktop/src-tauri/target/release/bundle/`

| 产物 | 说明 |
|------|------|
| `macos/<产品名>.app` | 应用包（`productName` 为 `XunJi` 时目录名为 `XunJi.app`；`bundle.macOS.bundleName` 仅影响展示名），内含 `Contents/Resources/xunji-sidecar` |
| `macos/*_<版本>_<arch>.dmg` | **最终**磁盘映像在同一目录 `macos/` 下，文件名含版本与架构（如 `XunJi_0.1.6_aarch64.dmg`；部分环境会先出现带 `rw.` 前缀的临时名） |
| `dmg/` | 仅 **create-dmg 中间资源**（如 `bundle_dmg.sh`、`.icns`），**不是**最终 `.dmg` 输出位置（Tauri 2 与旧文档路径习惯不同，易误以为「没有 dmg」） |

安装 DMG 后，运行时会按顺序查找 Sidecar：

1. 开发机仓库下的 `packages/sidecar/dist/xunji-sidecar`（仅本地仍存在源码树时）
2. **安装包内** `…/Contents/Resources/xunji-sidecar`（正常用户仅此即可）
3. 可选全局覆盖：`~/.xunji/bin/xunji-sidecar`

## 仅构建命令行（不打包安装包）

```bash
cd apps/desktop/src-tauri
cargo build --release
# 二进制：target/release/xunji-desktop
```

## 常见问题

- **Tauri 提示 `invalid value '1' for '--ci'`**  
  使用 `make macos` 或 `cd apps/desktop && bun run tauri:build`，不要在外层 CI 把 `CI=1` 传给 CLI。

- **Bundle identifier 以 `.app` 结尾的警告**  
  来自 `identifier: com.xunji.app`，若以后要上架或严格签名，可改为不含 `.app` 后缀的 ID（需同步改 `tauri.conf.json`）。

- **代码签名 / 公证**  
  本地试用可直接打开；若需分发给他人并减少「无法验证开发者」提示，需在 Apple 开发者账号下配置证书并对 `.app` 签名/notarize，此处不展开。

- **提炼报 API 鉴权或模型错误**
  在「设置 → AI 配置」检查当前配置的 Base URL、API Key 和完整模型 ID；寻迹不会自动切换到其他供应商。
