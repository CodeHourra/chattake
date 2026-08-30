<script setup lang="ts">
import { NInput, NButton, NTooltip } from 'naive-ui'
import { useSessionsStore } from '../stores/sessions'
import { useSearchStore } from '../stores/search'
import { useFiltersStore } from '../stores/filters'

const sessions = useSessionsStore()
const search = useSearchStore()
const filters = useFiltersStore()

async function refresh() {
  search.clear()
  await sessions.loadPage()
}

/**
 * 切换状态过滤。
 * 点击已选中的按钮 → 恢复全部；点击未选中的按钮 → 激活该过滤
 */
function setStatusFilter(val: '' | 'analyzed' | 'pending') {
  filters.statusFilter = filters.statusFilter === val ? '' : val
  sessions.page = 1
  void sessions.loadPage()
}
</script>

<template>
  <div class="flex flex-col gap-4 mb-1 shrink-0">
    <!-- 搜索行 -->
    <div class="flex items-center gap-3">
      <n-input
        v-model:value="search.query"
        placeholder="搜索知识笔记（标题 / 摘要 / 正文全文）..."
        clearable
        size="large"
        class="flex-1 rounded-lg"
        @clear="refresh"
      >
        <template #prefix>
          <span class="i-lucide-search w-4 h-4 text-slate-400 mr-1" />
        </template>
        <template #suffix>
          <span v-if="search.searching" class="i-lucide-loader-2 w-3.5 h-3.5 animate-spin text-slate-400" />
        </template>
      </n-input>

      <n-button size="large" secondary class="rounded-lg px-4" @click="refresh">
        <template #icon>
          <span class="i-lucide-rotate-ccw w-4 h-4 text-slate-500" />
        </template>
        <span class="text-slate-600 dark:text-slate-300">刷新</span>
      </n-button>
    </div>

    <!-- 过滤 & 信息行 -->
    <div class="flex flex-wrap items-center justify-between gap-2">
      <!-- 分段选择器：与顶栏 Tab 同一套扁平语义，segment-pill-btn 避免系统按钮灰底 -->
      <div class="bg-slate-100/80 dark:bg-neutral-900/55 p-1 rounded-lg inline-flex shrink-0">
        <button
          type="button"
          class="segment-pill-btn"
          :class="[
            'px-5 py-1.5 rounded-md text-sm font-medium whitespace-nowrap transition-colors cursor-pointer border-0 outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/35',
            !filters.statusFilter
              ? 'bg-white dark:bg-neutral-800 text-slate-800 dark:text-slate-100 ring-1 ring-slate-200/90 dark:ring-white/10'
              : 'bg-transparent text-slate-500 hover:text-slate-800 dark:text-neutral-500 dark:hover:text-neutral-200',
          ]"
          @click="setStatusFilter('')"
        >
          全部会话
        </button>
        <button
          type="button"
          class="segment-pill-btn"
          :class="[
            'px-5 py-1.5 rounded-md text-sm font-medium whitespace-nowrap transition-colors cursor-pointer border-0 outline-none focus-visible:ring-2 focus-visible:ring-amber-500/35 flex items-center gap-1.5',
            filters.statusFilter === 'pending'
              ? 'bg-white dark:bg-neutral-800 text-amber-600 dark:text-amber-400 ring-1 ring-slate-200/90 dark:ring-white/10'
              : 'bg-transparent text-slate-500 hover:text-slate-800 dark:text-neutral-500 dark:hover:text-neutral-200',
          ]"
          @click="setStatusFilter('pending')"
        >
          待分析
        </button>
        <button
          type="button"
          class="segment-pill-btn"
          :class="[
            'px-5 py-1.5 rounded-md text-sm font-medium whitespace-nowrap transition-colors cursor-pointer border-0 outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/35',
            filters.statusFilter === 'analyzed'
              ? 'bg-white dark:bg-neutral-800 text-emerald-600 dark:text-emerald-400 ring-1 ring-slate-200/90 dark:ring-white/10'
              : 'bg-transparent text-slate-500 hover:text-slate-800 dark:text-neutral-500 dark:hover:text-neutral-200',
          ]"
          @click="setStatusFilter('analyzed')"
        >
          已分析
        </button>
      </div>

      <!-- 右侧统计 + 信息提示 -->
      <div class="flex min-w-0 items-center gap-2 text-sm text-slate-500 dark:text-slate-400">
        <span>共 <strong class="text-slate-800 dark:text-slate-200 font-semibold mx-0.5">{{ sessions.total }}</strong> 条记录</span>
        <n-tooltip trigger="hover" placement="top-end">
          <template #trigger>
            <span class="i-lucide-info w-4 h-4 text-slate-400 cursor-help hover:text-emerald-500 transition-colors" />
          </template>
          搜索对象为已入库的知识卡片（FTS），不包含未提炼的会话标题。
        </n-tooltip>

        <span v-if="sessions.error" class="text-red-500 flex items-center gap-1 ml-2 min-w-0 max-w-64 truncate">
          <span class="i-lucide-alert-circle w-3 h-3" />
          {{ sessions.error }}
        </span>
        <span v-if="search.searchError" class="text-red-500 flex items-center gap-1 ml-2 min-w-0 max-w-64 truncate">
          <span class="i-lucide-alert-circle w-3 h-3" />
          {{ search.searchError }}
        </span>
      </div>
    </div>
  </div>
</template>
