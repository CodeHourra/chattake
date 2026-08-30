<script setup lang="ts">
import { computed, onActivated, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { NSpin, NEmpty, NButton, NCheckbox } from 'naive-ui'
import SessionToolbar from '../components/SessionToolbar.vue'
import SessionCard from '../components/SessionCard.vue'
import Pagination from '../components/Pagination.vue'
import { useSessionsStore } from '../stores/sessions'
import { useSearchStore } from '../stores/search'
import { useAnalysisQueueStore } from '../stores/analysisQueue'
import { api } from '../lib/tauri'
import { appendDistillHint } from '../lib/distillHints'
import { getCardTypeLabel } from '@chattake/shared'

const sessions = useSessionsStore()
const router = useRouter()
const search = useSearchStore()
const queue = useAnalysisQueueStore()

/** 知识库卡片总数：用于区分「无笔记可搜」与「关键词无匹配」 */
const libraryCardTotal = ref<number | null>(null)

// ── 批量选择 ─────────────────────────────────────────────────────────────────

/** 是否处于批量选择模式 */
const batchMode = ref(false)
/** 已选中的会话 ID Set */
const selectedIds = ref<Set<string>>(new Set())

// ── Toast ────────────────────────────────────────────────────────────────────

const toast = ref<{ msg: string; type: 'success' | 'error' | 'warning' } | null>(null)

function showToast(msg: string, type: 'success' | 'error' | 'warning' = 'success') {
  const text = type === 'error' ? appendDistillHint(msg) : msg
  toast.value = { msg: text, type }
  setTimeout(() => {
    toast.value = null
  }, type === 'error' ? 9000 : 4000)
}

function plainSnippet(value: string | null) {
  return value?.replace(/<\/?mark>/g, '') ?? ''
}

// ── 生命周期 ─────────────────────────────────────────────────────────────────

onMounted(() => {
  void sessions.loadPage()
  void api
    .countAllCards()
    .then((n) => {
      libraryCardTotal.value = n
    })
    .catch(() => {
      libraryCardTotal.value = null
    })
})

onActivated(() => {
  void sessions.loadPage()
})

// ── 计算属性 ─────────────────────────────────────────────────────────────────

/**
 * 判断会话是否「待分析」（可参与批量分析）。
 * 已分析（含低价值 status=analyzed）和分析中的均不可选。
 */
function isPending(s: { cardId?: string | null; status: string }) {
  return !s.cardId && s.status !== 'analyzing' && s.status !== 'analyzed'
}

const unanalyzedCount = computed(() => sessions.items.filter(isPending).length)

const selectableItems = computed(() => sessions.items.filter(isPending))

const allSelected = computed(
  () =>
    selectableItems.value.length > 0 &&
    selectableItems.value.every((s) => selectedIds.value.has(s.id)),
)

const indeterminate = computed(
  () => selectedIds.value.size > 0 && !allSelected.value,
)

// 批量分析进行中：队列里仍有来自本次批量预期的任务未完成
const activeAnalysisJob = computed(() => queue.jobs.find(
  (job) => job.kind === 'analysis' && (job.status === 'queued' || job.status === 'running'),
))
const batchRunning = computed(() => (activeAnalysisJob.value?.total ?? 0) > 1)
const batchFinished = computed(() => activeAnalysisJob.value?.done ?? 0)
const batchExpected = computed(() => activeAnalysisJob.value?.total ?? 0)

// ── 方法 ─────────────────────────────────────────────────────────────────────

/** 单条分析（从 SessionCard 触发） */
async function onAnalyze(sessionId: string) {
  try {
    await queue.startAnalysis([sessionId])
    showToast('分析任务已创建，可在任务中心查看阶段和进度')
  } catch (error) {
    showToast(error instanceof Error ? error.message : String(error), 'error')
  }
}

function toggleBatchMode() {
  batchMode.value = !batchMode.value
  if (!batchMode.value) {
    selectedIds.value = new Set()
  }
}

function toggleSelectAll(checked: boolean) {
  if (checked) {
    selectedIds.value = new Set(sessions.items.filter(isPending).map((s) => s.id))
  } else {
    selectedIds.value = new Set()
  }
}

function onSelectionChange(id: string, checked: boolean) {
  const next = new Set(selectedIds.value)
  if (checked) {
    next.add(id)
  } else {
    next.delete(id)
  }
  selectedIds.value = next
}

/**
 * 批量分析：全部入队，由全局队列串行执行；通过 callbacks 统计完成后 Toast
 */
async function startBatchAnalyze() {
  const ids = [...selectedIds.value]
  if (!ids.length) return

  try {
    await queue.startAnalysis(ids)
    showToast(`已创建批量分析任务，共 ${ids.length} 条`)
    batchMode.value = false
    selectedIds.value = new Set()
  } catch (error) {
    showToast(error instanceof Error ? error.message : String(error), 'error')
  }
}

function openSearchHit(cardId: string, sessionId: string) {
  void router.push({
    name: 'session-detail',
    params: { sessionId },
    query: { cardId },
  })
}
</script>

<template>
  <div class="sessions-page">
    <header class="page-intro">
      <div>
        <p>CONVERSATION ARCHIVE / 对话档案</p>
        <h2>追踪每一次思考的轨迹</h2>
      </div>
      <span>本地归档 · 用户确认后分析</span>
    </header>
    <SessionToolbar />

    <!-- Toast -->
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="-translate-y-3 opacity-0"
      enter-to-class="translate-y-0 opacity-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="translate-y-0 opacity-100"
      leave-to-class="-translate-y-3 opacity-0"
    >
      <div
        v-if="toast"
        class="fixed top-4 left-1/2 -translate-x-1/2 z-50 rounded-lg border px-3 py-2 shadow-lg flex items-center gap-2 text-sm"
        :class="{
          'border-red-200 bg-red-50 dark:bg-red-950/80 dark:border-red-800 text-red-800 dark:text-red-200': toast.type === 'error',
          'border-amber-200 bg-amber-50 dark:bg-amber-950/80 dark:border-amber-800 text-amber-800 dark:text-amber-200': toast.type === 'warning',
          'border-brand-200 bg-brand-50 dark:bg-brand-950/80 dark:border-brand-800 text-brand-800 dark:text-brand-200': toast.type === 'success',
        }"
      >
        <span
          :class="{
            'i-lucide-x-circle text-red-500': toast.type === 'error',
            'i-lucide-alert-triangle text-amber-500': toast.type === 'warning',
            'i-lucide-check-circle text-brand-500': toast.type === 'success',
          }"
          class="w-4 h-4"
        />
        {{ toast.msg }}
      </div>
    </Transition>

    <!-- 搜索结果（FTS 知识卡片） -->
    <div v-if="search.query.trim()" class="flex-1 min-h-0 overflow-y-auto space-y-3 pb-4">
      <div class="text-xs text-slate-500 dark:text-slate-400 font-medium">搜索结果（{{ search.results.length }}）</div>
      <div v-if="search.searching && !search.results.length" class="flex justify-center py-16">
        <n-spin size="medium" />
      </div>
      <n-empty
        v-else-if="!search.searching && !search.results.length"
        class="py-12"
      >
        <template #default>
          <div class="text-center space-y-1 px-4">
            <template v-if="libraryCardTotal === 0">
              <p class="text-sm text-slate-600 dark:text-slate-300">知识库中暂无笔记</p>
              <p class="text-xs text-slate-400">请先同步对话并完成提炼，再使用全文搜索。</p>
            </template>
            <template v-else-if="libraryCardTotal != null && libraryCardTotal > 0">
              <p class="text-sm text-slate-600 dark:text-slate-300">未找到包含该关键词的笔记</p>
              <p class="text-xs text-slate-400">试试更短的关键词、同义词，或检查拼写。</p>
            </template>
            <template v-else>
              <p class="text-sm text-slate-600 dark:text-slate-300">未找到匹配内容</p>
            </template>
          </div>
        </template>
      </n-empty>
      <div
        v-for="c in search.results"
        :key="c.id"
        class="rounded-lg border border-slate-200 dark:border-neutral-800 bg-white dark:bg-neutral-900 p-3 cursor-pointer hover:border-emerald-300 dark:hover:border-emerald-800 transition-all group"
        @click="openSearchHit(c.id, c.sessionId)"
      >
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0">
            <div class="text-sm font-medium text-slate-800 dark:text-slate-200 group-hover:text-emerald-600 dark:group-hover:text-emerald-400 truncate">{{ c.title }}</div>
            <div class="flex flex-wrap items-center gap-1.5 mt-1 text-[10px] text-slate-500">
              <span v-if="c.type">{{ getCardTypeLabel(c.type) }}</span>
              <span v-if="c.sourceName">· {{ c.sourceName }}</span>
              <span v-for="tag in c.tags" :key="tag" class="rounded border px-1.5 py-0.5">{{ tag }}</span>
              <span v-for="tech in c.technologies" :key="tech" class="rounded px-1.5 py-0.5" style="background: var(--pine-soft); color: var(--pine)">{{ tech }}</span>
            </div>
            <div class="text-xs text-slate-500 line-clamp-2 mt-1">{{ plainSnippet(c.matchSnippet) || c.summary }}</div>
          </div>
          <span class="i-lucide-arrow-right w-4 h-4 text-slate-400 group-hover:text-emerald-500 shrink-0" />
        </div>
      </div>
    </div>

    <!-- 会话列表 -->
    <template v-else>
      <div v-if="sessions.loading" class="flex-1 flex items-center justify-center">
        <n-spin size="medium" />
      </div>

      <div v-else-if="!sessions.items.length" class="flex-1 flex items-center justify-center">
        <n-empty description="暂无会话，请先同步">
          <template #extra>
            <n-button type="primary" @click="queue.startSync()">
              <span class="inline-flex items-center gap-1.5">
                <span class="i-lucide-refresh-cw w-3.5 h-3.5" />
                立即同步
              </span>
            </n-button>
          </template>
        </n-empty>
      </div>

      <template v-else>
        <!-- 列表标题行 -->
        <div class="list-heading">
          <div><span>INDEX</span><strong>会话索引</strong></div>
          <n-button
            v-if="unanalyzedCount > 0 && !batchMode"
            size="small"
            secondary
            :disabled="batchRunning"
            @click="toggleBatchMode"
          >
            <span class="inline-flex items-center gap-1.5">
              <span class="i-lucide-layers w-3.5 h-3.5" />
              批量操作
            </span>
          </n-button>
        </div>

        <p v-if="batchRunning" class="text-xs text-slate-500 dark:text-slate-400 mb-2">
          已加入全局分析队列，进度见右下角面板；请勿关闭应用。
        </p>

        <!-- 会话卡片列表 -->
        <div class="session-ledger">
          <SessionCard
            v-for="s in sessions.items"
            :key="s.id"
            :session="s"
            :selectable="batchMode"
            :selected="selectedIds.has(s.id)"
            @analyze="onAnalyze"
            @update:selected="onSelectionChange"
          />
        </div>

        <div class="shrink-0 py-3 border-t border-slate-200 dark:border-neutral-800">
          <Pagination
            :page="sessions.page"
            :page-size="sessions.pageSize"
            :total="sessions.total"
            @update:page="sessions.setPage"
            @update:page-size="sessions.setPageSize"
          />
        </div>

        <!-- 批量操作浮条 (Glass Bar) -->
        <Transition
          enter-active-class="transition ease-out duration-300 transform"
          enter-from-class="translate-y-24 opacity-0"
          enter-to-class="translate-y-0 opacity-100"
          leave-active-class="transition ease-in duration-200 transform"
          leave-from-class="translate-y-0 opacity-100"
          leave-to-class="translate-y-24 opacity-0"
        >
          <div
            v-show="batchMode"
            class="absolute bottom-8 left-0 right-0 flex justify-center z-50 pointer-events-none"
          >
            <div class="glass-bar rounded-full p-2 pl-5 flex items-center gap-5 pointer-events-auto shadow-xl">
              <!-- 选择状态 -->
              <div class="flex items-center gap-3 border-r border-slate-200/80 dark:border-neutral-600/80 pr-4">
                <n-checkbox
                  :checked="allSelected"
                  :indeterminate="indeterminate"
                  @update:checked="toggleSelectAll"
                />
                <span class="text-sm font-semibold text-slate-700 dark:text-slate-200 w-[60px]">
                  已选 <span class="text-emerald-600 dark:text-emerald-400">{{ selectedIds.size }}</span> 项
                </span>
              </div>
              <!-- 操作按钮 -->
              <div class="flex items-center gap-2">
                <n-button
                  type="primary"
                  round
                  size="large"
                  class="px-6"
                  :loading="batchRunning"
                  :disabled="selectedIds.size === 0 || batchRunning"
                  @click="startBatchAnalyze"
                >
                  <template #icon>
                    <span v-if="!batchRunning" class="i-lucide-sparkles w-4 h-4" />
                  </template>
                  {{ batchRunning ? `处理中 ${batchFinished}/${batchExpected}…` : '开始分析' }}
                </n-button>
                <n-button
                  round
                  size="large"
                  secondary
                  class="px-6"
                  :disabled="batchRunning"
                  @click="toggleBatchMode"
                >
                  取消
                </n-button>
              </div>
            </div>
          </div>
        </Transition>
      </template>
    </template>
  </div>
</template>

<style scoped>
.sessions-page { position:relative; display:flex; flex-direction:column; width:100%; max-width:1380px; height:100%; margin:0 auto; padding:30px 42px 0; }
.page-intro { display:flex; align-items:flex-end; justify-content:space-between; gap:24px; margin-bottom:24px; padding-bottom:20px; border-bottom:1px solid var(--line-strong); }
.page-intro p { margin:0 0 9px; color:var(--muted); font-size:10px; letter-spacing:.16em; }
.page-intro h2 { margin:0; max-width:600px; color:var(--ink); font-family:var(--font-editorial); font-size:27px; font-weight:500; letter-spacing:.02em; line-height:1.2; }
.page-intro > span { padding-bottom:3px; color:var(--muted); font-size:11px; }
.list-heading { display:flex; align-items:center; justify-content:space-between; min-height:48px; border-bottom:1px solid var(--line-strong); }
.list-heading > div { display:flex; align-items:baseline; gap:11px; }
.list-heading span { color:var(--vermilion); font-size:9px; letter-spacing:.14em; }
.list-heading strong { font-family:var(--font-editorial); font-size:15px; font-weight:600; }
.session-ledger { flex:1; min-height:0; overflow-y:auto; padding-bottom:108px; }
.glass-bar {
  background: rgba(255, 255, 255, 0.85);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid rgba(226, 232, 240, 0.8);
  box-shadow:
    0 20px 25px -5px rgba(5, 150, 105, 0.1),
    0 10px 10px -5px rgba(5, 150, 105, 0.05),
    inset 0 1px 2px 0 rgba(255, 255, 255, 0.1);
}
.dark .glass-bar {
  background: rgba(23, 23, 23, 0.85);
  border-color: rgba(64, 64, 64, 0.8);
  box-shadow:
    0 20px 25px -5px rgba(0, 0, 0, 0.3),
    0 10px 10px -5px rgba(0, 0, 0, 0.2);
}
@media (max-width:900px) { .sessions-page { padding:22px 22px 0; } .page-intro > span { display:none; } }
</style>
