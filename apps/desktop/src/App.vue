<script setup lang="ts">
import { computed } from 'vue'
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  darkTheme,
  zhCN,
  dateZhCN,
} from 'naive-ui'
import AppLayout from './components/AppLayout.vue'
import { useUiStore } from './stores/ui'

const ui = useUiStore()
const themeOverrides = computed(() => ({
  common: {
    primaryColor: ui.darkMode ? '#7ea497' : '#2f5145',
    primaryColorHover: ui.darkMode ? '#92b5aa' : '#3c6557',
    primaryColorPressed: ui.darkMode ? '#658b7e' : '#254339',
    borderRadius: '8px',
    fontFamily: 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  },
}))
</script>

<template>
  <!-- NConfigProvider 为 Naive UI 组件提供全局主题 + 中文 locale -->
  <n-config-provider :theme="ui.darkMode ? darkTheme : undefined" :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
    <!-- useMessage / useDialog（导出确认、Toast 等）依赖以下 Provider -->
    <n-message-provider>
      <n-dialog-provider>
        <AppLayout />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
:root {
  font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  --paper: #f3f0e7;
  --paper-raised: #fbf9f3;
  --ink: #181a18;
  --muted: #73766f;
  --line: #cbc6bb;
  --pine: #2f5145;
  --pine-soft: #dce7e1;
  --vermilion: #c85b36;
  --brand-500: var(--pine);
  --brand-600: #254339;
  color: var(--ink);
  background: var(--paper);
}

:root.dark {
  --paper: #141613;
  --paper-raised: #1c1f1b;
  --ink: #f1eee5;
  --muted: #a09f98;
  --line: #393b36;
  --pine: #7ea497;
  --pine-soft: #23372f;
  --vermilion: #df7955;
}

*,
*::before,
*::after {
  box-sizing: border-box;
}

body {
  margin: 0;
  color: var(--ink);
  background: var(--paper);
}

.glass-bar,
.glass-sidebar,
.glass-panel {
  background: color-mix(in srgb, var(--paper-raised) 84%, transparent) !important;
  backdrop-filter: blur(24px) saturate(1.08);
  -webkit-backdrop-filter: blur(24px) saturate(1.08);
}

.glass-bar { border-color: color-mix(in srgb, var(--line) 72%, transparent) !important; }
.glass-sidebar { border-color: color-mix(in srgb, var(--line) 66%, transparent) !important; }

::selection { background: color-mix(in srgb, var(--vermilion) 28%, transparent); }

/* Webkit 滚动条 */
::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background-color: rgba(156, 163, 175, 0.25);
  border-radius: 999px;
}
.dark ::-webkit-scrollbar-thumb {
  background-color: rgba(75, 85, 99, 0.35);
}

/**
 * 分段条内的原生 <button>：Tauri/WebKit 会套用系统按钮外观（灰底、凸起），
 * 未选中项看起来像「实心灰块」，浅色/深色下都会破坏与 ui_demo 一致的扁平分段样式。
 * 仅对带 .segment-pill-btn 的按钮去外观，避免影响 Naive UI 的 n-button。
 */
button.segment-pill-btn {
  -webkit-appearance: none;
  appearance: none;
  margin: 0;
  font: inherit;
}
</style>
