<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from 'vue'
import {
  NTooltip,
  NButton,
  NDropdown,
  useMessage,
  useDialog,
} from 'naive-ui'
import type { DropdownOption } from 'naive-ui'
import { useRouter } from 'vue-router'
import { useUiStore } from '../stores/ui'
import { useAnalysisQueueStore } from '../stores/analysisQueue'
import { api } from '../lib/tauri'
import { exportAllCardsToDir } from '../lib/cardExport'
const SettingsModal = defineAsyncComponent(() => import('./SettingsModal.vue'))
const AppUpdateModal = defineAsyncComponent(() => import('./AppUpdateModal.vue'))

const ui = useUiStore()
const queue = useAnalysisQueueStore()
const router = useRouter()
const message = useMessage()
const dialog = useDialog()

const showSettings = ref(false)
/** 独立「软件更新」弹窗（与设置解耦） */
const showAppUpdate = ref(false)
const syncing = computed(() => queue.jobs.some((job) => job.kind === 'sync' && ['queued', 'running'].includes(job.status)))
const activeJob = computed(() => queue.jobs.find((job) => ['queued', 'running'].includes(job.status)))

/** 顶栏「导出」下拉：与知识库页能力对齐 */
const exportDropdownOptions: DropdownOption[] = [
  { key: 'library', label: '前往知识库（多选 / 所选导出）' },
  { type: 'divider' },
  { key: 'all', label: '导出全部笔记…' },
]

function onExportDropdownSelect(key: string | number) {
  if (key === 'library') {
    void router.push({ name: 'library' })
    return
  }
  if (key !== 'all') return
  void (async () => {
    const total = await api.countAllCards()
    if (total === 0) {
      message.info('知识库暂无笔记')
      return
    }
    dialog.warning({
      title: '导出全部笔记',
      content: `将导出库内全部 ${total} 条笔记到所选文件夹，不受当前列表筛选影响。`,
      positiveText: '选择文件夹',
      negativeText: '取消',
      onPositiveClick: async () => {
        try {
          const r = await exportAllCardsToDir()
          if (r.ok && r.count != null) message.success(`已导出 ${r.count} 条笔记`)
        } catch (e) {
          message.error(e instanceof Error ? e.message : String(e))
        }
      },
    })
  })()
}

function onTabChange(tab: 'sessions' | 'library') {
  ui.activeTab = tab
  if (tab === 'sessions') {
    void router.push({ name: 'sessions' })
  } else {
    void router.push({ name: 'library' })
  }
}

async function onSync() {
  try {
    await queue.startSync()
    message.success('同步任务已创建，可在任务中心查看文件级进度')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    message.error(msg, { duration: 12000, closable: true })
  }
}
</script>

<template>
  <header class="top-bar glass-bar">
    <!-- 左侧：Logo + 分割线 + Tab 导航 -->
    <div class="top-bar-start">
      <!-- Logo 区域 -->
      <div class="flex items-center gap-3">
        <div class="app-mark w-8 h-8 rounded-lg flex items-center justify-center">
          <span class="i-lucide-book-open-check w-5 h-5" aria-hidden="true" />
        </div>
        <div class="flex flex-col">
          <h1 class="product-name">
            有得
            <span class="app-version px-1 py-0.5 rounded text-[9px] font-bold tracking-wider">0.2</span>
          </h1>
          <span class="product-tagline">AI 对话知识库</span>
        </div>
      </div>

      <!-- 竖线分割 -->
      <span class="top-divider" aria-hidden="true" />

      <nav class="top-nav" aria-label="主要页面">
        <button
          type="button"
          class="ui-tab top-nav-item"
          :class="{ active: ui.activeTab === 'sessions' }"
          :aria-current="ui.activeTab === 'sessions' ? 'page' : undefined"
          @click="onTabChange('sessions')"
        >
          <span class="i-lucide-messages-square top-nav-icon" aria-hidden="true" />
          对话档案
        </button>
        <button
          type="button"
          class="ui-tab top-nav-item"
          :class="{ active: ui.activeTab === 'library' }"
          :aria-current="ui.activeTab === 'library' ? 'page' : undefined"
          @click="onTabChange('library')"
        >
          <span class="i-lucide-library top-nav-icon" aria-hidden="true" />
          知识笔记
        </button>
      </nav>
    </div>

    <!-- 右侧：同步 + 工具按钮 -->
    <div class="flex items-center gap-2">
      <div v-if="activeJob" class="top-progress" aria-live="polite">
        <span>{{ activeJob.kind === 'sync' ? '同步' : '分析' }}</span>
        <span class="tabular-nums">{{ activeJob.done }}/{{ activeJob.total }}</span>
        <span class="top-progress-track"><i :style="{ width: `${activeJob.total ? activeJob.done / activeJob.total * 100 : 0}%` }" /></span>
      </div>
      <n-button
        size="small"
        secondary
        class="sync-button"
        :loading="syncing"
        :disabled="syncing"
        @click="onSync"
      >
        <span class="inline-flex items-center gap-1.5">
          <span v-if="!syncing" class="i-lucide-refresh-cw w-3.5 h-3.5" />
          {{ syncing ? '同步中…' : '同步' }}
        </span>
      </n-button>

      <span class="toolbar-divider" aria-hidden="true" />

      <n-dropdown
        trigger="click"
        :options="exportDropdownOptions"
        @select="onExportDropdownSelect"
      >
        <n-tooltip trigger="hover" :delay="400">
          <template #trigger>
            <n-button quaternary circle size="small" aria-label="导出">
              <span class="i-lucide-download w-4 h-4 text-slate-500 dark:text-slate-400" />
            </n-button>
          </template>
          导出
        </n-tooltip>
      </n-dropdown>

      <n-tooltip trigger="hover" :delay="400">
        <template #trigger>
          <n-button quaternary circle size="small" aria-label="检查更新" @click="showAppUpdate = true">
            <span class="i-lucide-download-cloud w-4 h-4 text-slate-500 dark:text-slate-400" />
          </n-button>
        </template>
        检查更新
      </n-tooltip>

      <n-tooltip trigger="hover" :delay="400">
        <template #trigger>
          <n-button quaternary circle size="small" aria-label="设置" @click="showSettings = true">
            <span class="i-lucide-settings w-4 h-4 text-slate-500 dark:text-slate-400" />
          </n-button>
        </template>
        设置
      </n-tooltip>

      <n-tooltip trigger="hover" :delay="400">
        <template #trigger>
          <n-button quaternary circle size="small" :aria-label="ui.darkMode ? '切换到浅色模式' : '切换到深色模式'" @click="ui.toggleTheme()">
            <span :class="ui.darkMode ? 'i-lucide-sun' : 'i-lucide-moon'" class="w-4 h-4 text-slate-500 dark:text-slate-400" />
          </n-button>
        </template>
        {{ ui.darkMode ? '浅色模式' : '深色模式' }}
      </n-tooltip>
    </div>
  </header>

  <app-update-modal v-model:show="showAppUpdate" />
  <settings-modal v-model:show="showSettings" />
</template>

<style scoped>
.top-bar { display:flex; align-items:center; justify-content:space-between; height:64px; padding:0 22px; flex-shrink:0; border-bottom:1px solid var(--line); z-index:50; }
.top-bar-start { display:flex; align-items:center; align-self:stretch; gap:20px; }
.app-mark { background: var(--ink); color: var(--paper); }
.app-version { background: color-mix(in srgb, var(--vermilion) 16%, transparent); color: var(--vermilion); }
.product-name { display:flex; align-items:center; gap:6px; margin:0; color:var(--ink); font-family:var(--font-editorial); font-size:16px; font-weight:600; line-height:1.2; letter-spacing:.08em; }
.product-tagline { color:var(--muted); font-size:9px; font-weight:500; line-height:1.3; letter-spacing:.12em; }
.top-divider,.toolbar-divider { width:1px; background:var(--line); }
.top-divider { height:24px; }
.toolbar-divider { height:18px; margin:0 4px; }
.top-nav { display:flex; align-items:stretch; align-self:stretch; gap:2px; }
.top-nav-item { position:relative; display:flex; align-items:center; gap:8px; min-width:122px; padding:0 15px; background:transparent; color:var(--muted); font-size:13px; font-weight:500; letter-spacing:.01em; cursor:pointer; transition:color .18s var(--ease-out), background-color .18s var(--ease-out); }
.top-nav-item:hover { color:var(--ink); background:color-mix(in srgb,var(--surface) 44%,transparent); }
.top-nav-item.active { color:var(--ink); }
.top-nav-item.active::after { content:''; position:absolute; right:15px; bottom:-1px; left:15px; height:2px; background:var(--vermilion); }
.top-nav-icon { width:16px; height:16px; color:currentColor; opacity:.82; }
.top-nav-item.active .top-nav-icon { color:var(--pine); opacity:1; }
.sync-button { min-width:70px; }
.top-progress { display:flex; align-items:center; gap:6px; color:var(--muted); font-size:10px; }
.top-progress-track { width:48px; height:2px; overflow:hidden; background:var(--line); }
.top-progress-track i { display:block; height:100%; background:var(--pine); transition:width .16s linear; }
@media (max-width:1000px) { .product-tagline,.app-version { display:none; } .top-bar-start { gap:12px; } .top-nav-item { min-width:auto; padding:0 12px; } }
</style>
