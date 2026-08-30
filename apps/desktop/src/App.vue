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
    primaryColor: ui.darkMode ? '#86aa9e' : '#31594f',
    primaryColorHover: ui.darkMode ? '#9bbcaf' : '#406f62',
    primaryColorPressed: ui.darkMode ? '#6e9286' : '#27493f',
    borderRadius: '7px',
    fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", sans-serif',
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
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'PingFang SC', sans-serif;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  --canvas: #f3f0e8;
  --surface: #faf8f2;
  --surface-glass: rgba(250, 248, 242, .78);
  --ink: #1a1c1a;
  --ink-soft: #4f524e;
  --muted: #747671;
  --line: #d7d2c8;
  --line-strong: #c9c5bb;
  --pine: #31594f;
  --pine-soft: #dce8e2;
  --vermilion: #c85f32;
  --paper: var(--canvas);
  --paper-raised: var(--surface);
  --font-editorial: 'Songti SC', 'STSong', 'Noto Serif CJK SC', serif;
  --brand-500: var(--pine);
  --brand-600: #254339;
  color: var(--ink);
  background: var(--paper);
}

:root.dark {
  --canvas: #151714;
  --surface: #1c1f1b;
  --surface-glass: rgba(28, 31, 27, .8);
  --ink: #f2efe7;
  --ink-soft: #c5c5bd;
  --muted: #989a94;
  --line: #343832;
  --line-strong: #464a43;
  --pine: #86aa9e;
  --pine-soft: #243a33;
  --vermilion: #dc7650;
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
  background: var(--surface-glass) !important;
  backdrop-filter: blur(24px) saturate(1.06);
  -webkit-backdrop-filter: blur(24px) saturate(1.06);
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
