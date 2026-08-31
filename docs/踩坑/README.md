# 踩坑文档说明

本目录存放**已定位根因**且**有可验证修复**的问题记录（现象、根因、解决、预防）。编写规范见项目技能 `pitfall-documentation`。

## 文件命名

- **以中文描述主题**，用 `**-`** 连接层次（领域 / 子主题 / 关键词）。
- **技术专名**可保留英文，便于检索与对齐代码：`UnoCSS`、`Tauri`、`CLI`、`JSON`、`WebView`、`Tailwind`、`FTS5`、`MATCH`、`role` 等。
- 避免整文件名纯英文蛇形（如 ~~`desktop-foo-bar.md`~~），除非尚无稳定中文概括。

## 条目索引（按文件名）


| 文件                                                                                   | 概要                          |
| ------------------------------------------------------------------------------------ | --------------------------- |
| [Bun-bun-run命令写法差异.md](./Bun-bun-run命令写法差异.md)                                       | `bun run` 与嵌套 `bun bun run` |
| [Monorepo-混用包管理器.md](./Monorepo-混用包管理器.md)                                           | 锁文件与依赖一致性                   |
| [Naive-UI-Vue重复实例白屏.md](./Naive-UI-Vue重复实例白屏.md)                                     | 多份 Vue 运行时                  |
| [工具结果-role识别.md](./工具结果-role识别.md)                                                   | tool result 角色识别            |
| [工具结果-正则匹配失败.md](./工具结果-正则匹配失败.md)                                                   | tool result 正则              |
| [构建-UnoCSS-生产构建图标丢失.md](./构建-UnoCSS-生产构建图标丢失.md)                                     | 生产构建图标空、Node `styleText`    |
| [桌面应用-Cursor-workspace路径百分号编码.md](./桌面应用-Cursor-workspace路径百分号编码.md)                 | 采集路径编码                      |
| [桌面应用-SQLite-FTS5-MATCH虚拟表名.md](./桌面应用-SQLite-FTS5-MATCH虚拟表名.md)                     | FTS5 MATCH 虚拟表              |
| [桌面应用-Tailwind-ring与overflow-hidden裁切.md](./桌面应用-Tailwind-ring与overflow-hidden裁切.md) | `ring` 与 `overflow-hidden`  |
| [桌面应用-会话提炼-JSON解析与标签技术栈.md](./桌面应用-会话提炼-JSON解析与标签技术栈.md)                             | 提炼 JSON / 标签 / 技术栈          |
| [桌面应用-会话重同步覆盖analysis标题.md](./桌面应用-会话重同步覆盖analysis标题.md)                             | 重同步与 analysis 标题            |
| [桌面应用-分段控件-WebView原生button默认样式.md](./桌面应用-分段控件-WebView原生button默认样式.md)               | WebView 原生按钮外观              |
| [桌面应用-脚手架-UnoCSS-Tauri与存储层常见问题.md](./桌面应用-脚手架-UnoCSS-Tauri与存储层常见问题.md)               | 脚手架、UnoCSS、Tauri、存储         |

