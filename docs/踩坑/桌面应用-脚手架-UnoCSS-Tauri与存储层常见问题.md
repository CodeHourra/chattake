# 踩坑：脚手架阶段 — UnoCSS、Tauri 布局、存储层与 CSP

本文汇总在 **有得 v0.1 脚手架与 Phase 1～2（存储层）** 开发中遇到的典型问题：**现象 → 原因 → 解决办法**。与「图标在构建后全部丢失」相关的 Node/Bun 版本与 `node:util.styleText` 问题，见同目录 [构建-UnoCSS-生产构建图标丢失.md](./构建-UnoCSS-生产构建图标丢失.md)。

---

## 一、Monorepo 里 Tauri 前端 `package.json` 应放在哪一层？

### 现象

- 按计划把 `package.json` 放在 `apps/desktop/src/` 下时，执行 `npx tauri dev` 报错：找不到 `tauri.conf.json`（当前目录下无 `src-tauri` 子目录）。
- 或 `beforeDevCommand` 的工作目录与 `vite` 实际根目录不一致。

### 原因

- **Tauri CLI** 会在**当前工作目录**下递归查找 `src-tauri/tauri.conf.json`。标准布局是：

  ```text
  apps/desktop/
    package.json      ← 与 src-tauri 同级
    src/              ← 仅放 Vue 源码
    src-tauri/
  ```

- 若把 `package.json` 嵌在 `src/` 里，CLI 从 `apps/desktop` 运行时能工作，但从 `apps/desktop/src` 运行则找不到 `../src-tauri`。

### 解决办法

- **推荐**：`package.json`、`vite.config.ts`、`index.html` 放在 `apps/desktop/`，Vue 源码仍在 `apps/desktop/src/`。
- 根目录 `package.json` 的 `workspaces` 指向 `"apps/desktop"`（不要写成 `apps/desktop/src`）。
- `tauri.conf.json` 里 `frontendDist`、`beforeDevCommand` 使用相对 `src-tauri` 的路径（如 `../dist`、`npm run dev` 在 `apps/desktop` 执行）。

---

## 二、开发时提示「Port 1420 is already in use」

### 现象

- `npm run tauri dev` 或 `vite` 启动失败：`Error: Port 1420 is already in use`。

### 原因

- 上一次 **Tauri dev** 或 **Vite** 未正常退出，进程仍占用 1420（`tauri.conf.json` / `vite.config.ts` 里 `strictPort: true` 时无法自动换端口）。

### 解决办法

```bash
# 查占用并结束（macOS/Linux）
lsof -ti:1420 | xargs kill -9

# 或结束残留桌面进程
pkill -f chattake-desktop
```

重新执行 `npm run tauri dev`（或项目 Makefile/脚本里等价命令）。

---

## 三、UnoCSS 构建报错：`oxc-parser` Cannot find native binding

### 现象

- 执行 `vite build` 或 `tauri dev` 时失败，堆栈指向 `oxc-parser`、`@unocss/transformer-attributify-jsx`、`unocss` 加载配置。
- 错误提示建议删除 `node_modules` 与 lock 后重装，仍可能复现。

### 原因

- 安装完整包 **`unocss`** 会拉取 **`@unocss/transformer-attributify-jsx`** → 依赖 **`oxc-parser`** 原生绑定；对 **Node 版本**、**可选依赖安装**、**npm workspace 提升** 较敏感。
- 本地 Node 若低于部分依赖声明的 `engines`（如需 ≥20.19），可能出现绑定缺失或安装不完整。

### 解决办法（本项目采用）

- **不要**安装元包 `unocss`，只装实际用到的子包：

  - `@unocss/vite`
  - `@unocss/preset-uno`
  - `@unocss/preset-icons`
  - `@unocss/core`（类型 / 配置）

- `vite.config.ts` 使用 `import UnoCSS from '@unocss/vite'`。
- `uno.config.ts` 从 `@unocss/preset-uno` / `@unocss/preset-icons` 引入 preset，用 `UserConfig`（`@unocss/core`）导出配置，避免依赖会拉 `oxc-parser` 的 transformer 包。

若仍失败，可再核对：清掉 `node_modules` 与 lock 后重装、或升级 Node 到 LTS 满足子包 `engines`。

---

## 四、`npx tauri dev` 报「could not determine executable to run」

### 现象

- 在部分环境执行 `npx tauri dev` 失败，npm 无法解析要运行的可执行文件。

### 原因

- `npx` 解析与 **当前目录**、**本地 `node_modules/.bin`**、**npm 版本** 有关；在 monorepo 子包或沙箱环境下偶发异常。

### 解决办法

- 在 **`apps/desktop`** 下使用 **`npm run tauri -- dev`**（依赖写在 `devDependencies` 的 `@tauri-apps/cli`）。
- 确保工作目录为 `apps/desktop`，且已 `npm install`。

---

## 五、Git 初始化在沙箱中失败：`Operation not permitted`

### 现象

- `git init` 报错：`.git/hooks/: Operation not permitted`。

### 原因

- 部分自动化环境对 **`.git/hooks`** 写入有限制。

### 解决办法

- 在**非沙箱**或允许全部权限的终端执行 `git init`，或在本地终端直接操作。

---

## 六、`insert_card` 的 INSERT 列数与占位符不一致（严重）

### 现象

- 运行时执行 `insert_card` 可能报 SQLite 参数个数错误，或写入错误列（如 `cost_yuan` 未绑定）。

### 原因

- `cards` 表 **18 列**，但 `VALUES` 中 `?` 与 `NULL` 数量与 **14 个绑定参数** 未一一对应；其中 **`cost_yuan` 曾错误写成 `NULL`**，导致列与值错位。

### 解决办法

- 按列顺序逐列核对：`id` … `note` → `category_id/memory/skill` 为 `NULL` → `source_name` … → `cost_yuan` → `feedback` 为 `NULL` → `created_at`、`updated_at`。
- 用 **`NewCard` 结构体** 单入参替代十几个散落参数，降低再错概率；代码中加简短 ASCII 注释标出「几列 ?、几列 NULL」。

---

## 七、`insert_messages` 使用五元组可读性差

### 现象

- 调用方必须记住 `(String, String, Option<String>, i32, i32)` 各位置含义，易传错顺序。

### 原因

- 类型信息无法自解释，不符合 clean-code。

### 解决办法

- 定义 **`NewMessage`** 结构体（`role`、`content`、`timestamp`、`tokens_in`、`tokens_out`），`insert_messages` 接收 `&[NewMessage]`。

---

## 八、CSP 设为 `null` 的安全风险；图标与「其他 CSS」是否受影响？

### 现象

- 希望收紧 CSP，又担心 **UnoCSS 生成的样式 / 图标** 被拦截。

### 原因与结论

- **`style-src 'self' 'unsafe-inline'`** 通常已覆盖：
  - Vite/UnoCSS 注入的 **内联 `<style>`**（需 `'unsafe-inline'`）；
  - 同源的 **`/assets/*.css`**（需 `'self'`）。
- **图标**：若使用 **`presetIcons` + 本地 `@iconify-json/*`**（**不要**配置 `cdn: 'https://…'`），图标在构建时内联进 CSS（如 `data:` URL），**不依赖外网**，也**不必**为图标单独放宽 `connect-src`。
- **若仍用 CDN 拉图标**：才需要在 CSP 中增加 `connect-src`/`img-src` 等对应域名；桌面应用更推荐本地 bundle。

### 解决办法（本项目）

- `tauri.conf.json` 设置基础 CSP（示例）：`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' asset: https://asset.localhost`（以实际 Tauri 版本文档为准）。
- **UnoCSS**：去掉 `presetIcons` 的 **`cdn`**，依赖已安装的 iconify JSON，离线可用。

---

## 九、`env_logger` 默认几乎看不到 `info!` / `debug!`

### 现象

- Rust 里写了 `log::info!`，终端无输出。

### 原因

- `env_logger::init()` 默认 filter 偏保守，未设 `RUST_LOG` 时可能只看到 error。

### 解决办法

```rust
env_logger::Builder::from_env(
    env_logger::Env::default().default_filter_or("info")
).init();
```

需要更细日志时：`RUST_LOG=debug cargo run` 或启动前导出环境变量。

---

## 十、`AppState` 里对 `Database` 再包一层 `Arc` 是否必要？

### 现象

- 代码审查质疑双重 `Arc`。

### 原因

- Tauri 的 **`.manage(state)`** 内部会对 state 做 **`Arc` 包装**；若 `Database` 已是共享语义，外层再 `Arc<Database>` 多一层间接引用（非错误，但冗余）。

### 解决办法

- 使用 **`AppState { db: Database }`**（具体以当前 Tauri 版本与 `Send` 要求为准；`Database` 内含 `Mutex<Connection>` 时通常可行）。

---

## 十一、其他存储层细节（审查结论摘要）

| 问题 | 处理 |
|------|------|
| `get_card` 用 `filter_map(\|r\| r.ok())` 吞掉标签查询错误 | 改为 `collect::<Result<Vec<_>, _>>()?` 传播错误 |
| `delete_card` 删除 0 行仍返回 `Ok` | 判断 `DELETE` 影响行数，0 行返回 `NotFound` |
| 迁移大段 DDL 失败可能导致半迁移 | 后续可考虑整段包在事务中（需单独验证 SQLite `execute_batch` 行为） |

---

## 相关文档

- [桌面应用-Tailwind-ring与overflow-hidden裁切.md](./桌面应用-Tailwind-ring与overflow-hidden裁切.md) — `ring` 画在盒外时被 `overflow-hidden` 裁掉，选中描边不完整
- [桌面应用-分段控件-WebView原生button默认样式.md](./桌面应用-分段控件-WebView原生button默认样式.md) — 分段条原生 `<button>` 在 WebView 下的灰底与暗色层次问题
- [构建-UnoCSS-生产构建图标丢失.md](./构建-UnoCSS-生产构建图标丢失.md) — 构建后图标全丢、Node `styleText`、Bun `--bun`
- [桌面应用-提炼-CLI路径与环境变量.md](./桌面应用-提炼-CLI路径与环境变量.md) — 桌面进程 PATH、CLI 绝对路径
- [Monorepo-混用包管理器.md](./Monorepo-混用包管理器.md) — 锁文件与依赖一致性

---

*文档整理自有得 v0.1 脚手架与存储层开发过程中的问题记录，随实现迭代可继续补充。*
