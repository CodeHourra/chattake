# 桌面应用：打 tag 发版与应用内更新

## 流程概览

1. 同步修改桌面端、Sidecar、MCP、Shared、Tauri 和 Cargo 的版本，并在 `CHANGELOG.md` 增加同版本标题。
2. 本地执行 `bun run check:release-version -- v0.2.0`、`bun run sync:changelog` 与 `bun run verify`。
3. 推送 tag：`git tag v0.2.0 && git push origin v0.2.0`。
4. GitHub Actions 先执行全仓库检查；通过后创建草稿 Release，并分别构建 macOS aarch64 与 x86_64。
5. 两个架构与 `latest.json` 全部上传成功后才公开 Release；任一阶段失败时保留草稿，不向用户暴露半成品版本。

应用内「检查更新」使用 `tauri-plugin-updater`，从以下地址拉取静态清单：

`https://github.com/CodeHourra/chattake/releases/latest/download/latest.json`

工作流当前只发布 macOS 双架构；Windows 资源配置仍可用于本地构建，但尚未进入自动发布矩阵。

## 更新签名与 macOS 分发模式

Tauri 2 的更新通道使用 minisign 密钥对，防止自动更新包被篡改；这与 Apple Developer 签名是两套独立机制。v0.2.0 的 macOS DMG 采用 **Ad Hoc 签名、未公证分发**，不需要 Apple Developer 证书或公证密钥。

> **注意**：仓库内 `tauri.conf.json` 的 `plugins.updater.pubkey` 必须与 GitHub Secret `TAURI_SIGNING_PRIVATE_KEY` 中的私钥成对。若你尚未在 Actions 中配置私钥，请按下方步骤生成新密钥对，并用 **公钥全文** 替换配置里的 `pubkey` 字段（勿提交私钥文件）。

### 1. 生成本地密钥

在仓库根目录：

```bash
cd apps/desktop
env -u CI bun x tauri signer generate --ci -p '' -w chattake.updater.key -f
```

- 私钥文件：`chattake.updater.key`（已加入根目录 `.gitignore` 模式 `*.updater.key`，勿提交）
- 公钥文件：`chattake.updater.key.pub`

### 2. 将公钥写入应用配置

把 `chattake.updater.key.pub` 的**完整一行内容**粘贴到 `apps/desktop/src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey` 字段。

### 3. 将私钥写入 GitHub Actions Secrets

在 GitHub 仓库 **Settings → Secrets and variables → Actions** 中新增：

| Name | 说明 |
|------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥文件**全文**（与 `pubkey` 成对） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 若生成时使用了 `-p` 密码则填写，否则可留空或不建此项 |

未配置 `TAURI_SIGNING_PRIVATE_KEY` 时，Release 工作流会在构建前失败并提示。

### 4. Ad Hoc 签名与首次打开

`tauri.conf.json` 将 `bundle.macOS.signingIdentity` 固定为 `"-"`。这会生成带 Ad Hoc 签名的 `.app` / `.dmg`，适合不使用 Apple Developer 账号的直接下载分发，但不会绕过 Gatekeeper。

用户首次启动若被拦截：

1. 先尝试打开一次应用。
2. 打开「系统设置 → 隐私与安全性」。
3. 在安全性区域点击「仍要打开」，输入登录密码并确认。

macOS 会把该应用保存为例外，后续可正常双击启动。详见 [Apple 官方说明](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac)。若未来需要消除该提示，再切换为 Developer ID 签名与 Apple 公证。

### 5. 密钥轮换

若私钥泄露或遗失：重新生成密钥对 → 更新 `pubkey` → 更新 Secret → **发一版新安装包**；已安装旧公钥的用户需能先收到一次带新公钥的更新（或需手动重装，视情况而定）。

## 本地验证构建

```bash
export CHATTAKE_BUN_TARGET=bun-darwin-arm64 # Intel 改为 bun-darwin-x64
export TAURI_SIGNING_PRIVATE_KEY="$(cat chattake.updater.key)"
# 若有密码：export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='...'
bun run --cwd apps/desktop tauri -- build --target aarch64-apple-darwin
```

`CHATTAKE_BUN_TARGET` 必须与 Rust `--target` 一致，否则主程序与 Sidecar/MCP 会出现架构不匹配。

## 常见问题

### 检查更新失败：`darwin-x86_64` 不在 `platforms` 里

仅发布 **aarch64** 包时，`latest.json` 往往只有 `darwin-aarch64`。**Intel Mac** 或 **Rosetta 下的 x86 应用** 会查找 `darwin-x86_64`，若 Release 未构建该目标，会报 *None of the fallback platforms … were found in the response*。请在 `release.yml` 的 `publish-tauri` 矩阵中保留 `x86_64-apple-darwin` 并成功上传；发新 tag 后再试「检查更新」。

## 端到端验证（两连续 tag）

1. 安装旧版本打包产物（或本地 `tauri build` 的上一版）。
2. 合并新版本号并推送新 tag，等待 Release 工作流完成。
3. 在旧版应用中打开 **设置 → 关于有得 → 检查更新**，应能发现新版本；下载安装并重启后版本号应更新。

若 `latest.json` 中某平台条目不完整，Tauri 在校验清单时可能直接失败，请确保 CI 各 matrix 任务均成功上传。
