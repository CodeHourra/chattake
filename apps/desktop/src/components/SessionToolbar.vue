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
  <div class="session-toolbar">
    <div class="search-row">
      <n-input
        v-model:value="search.query"
        placeholder="搜索知识笔记、主题或技术…"
        clearable
        class="archive-search"
        @clear="refresh"
      >
        <template #prefix>
          <span class="i-lucide-search w-4 h-4" />
        </template>
        <template #suffix>
          <span v-if="search.searching" class="i-lucide-loader-2 w-3.5 h-3.5 animate-spin text-slate-400" />
        </template>
      </n-input>

      <n-button secondary @click="refresh">
        <template #icon>
          <span class="i-lucide-rotate-ccw w-4 h-4 text-slate-500" />
        </template>
        <span>刷新</span>
      </n-button>
    </div>

    <div class="filter-row">
      <div class="status-tabs">
        <button
          type="button"
          class="segment-pill-btn"
          :class="{ active: !filters.statusFilter }"
          @click="setStatusFilter('')"
        >
          全部会话
        </button>
        <button
          type="button"
          class="segment-pill-btn"
          :class="{ active: filters.statusFilter === 'pending' }"
          @click="setStatusFilter('pending')"
        >
          待分析
        </button>
        <button
          type="button"
          class="segment-pill-btn"
          :class="{ active: filters.statusFilter === 'analyzed' }"
          @click="setStatusFilter('analyzed')"
        >
          已分析
        </button>
      </div>

      <div class="record-count">
        <span>共 <strong>{{ sessions.total }}</strong> 条记录</span>
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

<style scoped>
.session-toolbar { display:flex; flex-direction:column; gap:13px; flex-shrink:0; margin-bottom:12px; }
.search-row { display:flex; align-items:center; gap:10px; }
.archive-search { flex:1; }
.archive-search :deep(.n-input-wrapper) { min-height:42px; padding:0 13px; background:color-mix(in srgb,var(--surface) 66%,transparent); box-shadow:inset 0 0 0 1px var(--line) !important; }
.filter-row { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:10px; }
.status-tabs { display:inline-flex; align-items:center; gap:22px; }
.status-tabs button { position:relative; padding:4px 0 8px; border:0; background:transparent; color:var(--muted); font-size:12px; cursor:pointer; }
.status-tabs button:hover,.status-tabs button.active { color:var(--ink); }
.status-tabs button.active::after { content:''; position:absolute; right:0; bottom:0; left:0; height:1px; background:var(--vermilion); }
.record-count { display:flex; align-items:center; gap:8px; color:var(--muted); font-size:11px; }
.record-count strong { color:var(--ink); font-size:13px; font-weight:600; }
</style>
