# 踩坑：Naive UI 替换后白屏 —— Vue 实例重复

本文记录将 UI 组件库从 **Radix Vue** 迁移到 **Naive UI** 后，应用出现白屏的问题排查与解决过程。

---

## 一、背景

项目 `apps/desktop` 原使用 Radix Vue 作为 UI 组件库。为了获得更丰富的开箱即用组件（Table、Form、Tree 等），决定替换为 Naive UI。

替换涉及 3 个文件：`TopBar.vue`（Tabs + Tooltip）、`Sidebar.vue`（Select）、`Pagination.vue`（Select），改动量不大。

---

## 二、问题现象

替换完成、TypeScript 类型检查通过后，应用启动**白屏**，Vite 无编译报错，但浏览器控制台报错：

```
[Vue warn]: withDirectives can only be used inside render functions.

[Vue warn]: Unhandled error during execution of render function
  at <ResizeObserver onResize=fn<handleSegmentResize> >
  at <Tabs type="segment" value="sessions" size="small" ... >
  at <TopBar>
  at <AppLayout>
  at <ConfigProvider theme=undefined >
  at <App>

Uncaught TypeError: Cannot read properties of null (reading 'ce')
```

---

## 三、根因分析

### 3.1 直接原因：Vue 实例重复

`withDirectives can only be used inside render functions` 是 Vue 3 内部检查。它依赖模块级变量 `currentRenderingInstance`，当同一页面存在**两份不同的 Vue 运行时**时，一份的渲染上下文对另一份不可见，导致该检查失败。

### 3.2 根本原因：bun + pnpm 混用安装依赖

项目最初用 **bun** 安装依赖（`bun install`），依赖存放在根目录 `node_modules/.bun/` 下。后续为安装 naive-ui，使用了 **pnpm**（`pnpm add naive-ui --filter @chattake/desktop`），pnpm 在 `apps/desktop/node_modules/.pnpm/` 下创建了独立的依赖树。

Vite 预构建（dep optimization）时的解析路径：

| 包 | 解析来源 |
|---|---|
| `vue` | `node_modules/.bun/vue@3.5.30+.../node_modules/vue` |
| `naive-ui` | `apps/desktop/node_modules/.pnpm/naive-ui@2.44.1_vue@3.5.30/node_modules/naive-ui` |

naive-ui 内部 `import { withDirectives } from 'vue'` 解析到的是 pnpm 管理的 Vue 副本，而应用代码使用的是 bun 管理的 Vue 副本。**两个不同的 Vue 模块实例** → 内部状态不共享 → 白屏。

### 3.3 `.ce` 属性报错

`.ce` 是 Vue 3 VNode 上的 `customElement` 内部属性。在 `withDirectives` 失败后，VNode 未被正确初始化，后续代码尝试访问 null 引用上的 `.ce` 属性。这是前述问题的**连锁反应**，而非独立 bug。

---

## 四、解决方案

### 4.1 Vite 配置添加 `resolve.dedupe`

在 `apps/desktop/vite.config.ts` 中添加：

```typescript
export default defineConfig({
  plugins: [vue(), UnoCSS()],
  resolve: {
    dedupe: ['vue'],
  },
  // ...
})
```

`resolve.dedupe` 告诉 Vite：无论依赖树中有多少个 `vue` 的物理副本，都**强制解析到同一个模块实例**。

### 4.2 清除 Vite 预构建缓存

```bash
rm -rf apps/desktop/node_modules/.vite
```

旧缓存中仍然保留了双 Vue 解析结果，需要清除后让 Vite 重新预构建。

---

## 五、额外踩坑：NTabs 的 segment 类型需要 NTabPane

Naive UI 的 `NTabs` 组件在 `type="segment"` 模式下，子组件必须使用 `NTabPane`，不能使用 `NTab`。`NTab` 是"仅标签无面板"的组件，但 segment 模式需要通过 `NTabPane` 来正确测量每个标签宽度（内部使用 `ResizeObserver`）。

如果只需要标签触发器而不需要面板内容（如 vue-router 控制页面切换），可通过 `pane-style` 隐藏面板：

```vue
<n-tabs type="segment" :value="activeTab" size="small"
  :pane-style="{ display: 'none' }"
  @update:value="onTabChange"
>
  <n-tab-pane name="sessions">
    <template #tab>对话记录</template>
  </n-tab-pane>
  <n-tab-pane name="library">
    <template #tab>知识库</template>
  </n-tab-pane>
</n-tabs>
```

---

## 六、经验总结

1. **不要混用包管理器**。同一项目如果用 bun 初始化，后续安装也应统一使用 bun；pnpm 同理。混用会导致依赖树分裂，引发运行时单例冲突。
2. **`resolve.dedupe` 是保底措施**。即使统一了包管理器，monorepo 或 peerDependency 场景下仍可能出现 Vue 重复。对 Vue 项目建议始终配置 `resolve.dedupe: ['vue']`。
3. **白屏先查控制台**。Tauri webview 中白屏不一定是 Rust 问题，优先在浏览器（`http://localhost:1420`）中打开并检查 JS 控制台错误。
4. **Vite 缓存导致修改不生效**。修改 `vite.config.ts` 后 Vite 会自动重启，但旧的 `.vite/deps` 缓存不一定会刷新。遇到玄学问题时，手动删除 `node_modules/.vite` 是有效排查手段。
