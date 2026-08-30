<script setup lang="ts">
/**
 * 知识库 —— 笔记卡片列表 / 卡片网格双视图，内容区固定高度可滚动，分页贴底。
 *
 * 视图模式持久化：localStorage key `xunji:knowledgeViewMode`
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import {
  NSpin,
  NEmpty,
  NTag,
  NCheckbox,
  NButton,
  NInput,
  NModal,
  NSelect,
  useMessage,
  useDialog,
} from 'naive-ui'
import { getCardTypeLabel } from '@xunji/shared'
import { api } from '../lib/tauri'
import { exportAllCardsToDir, exportSelectedCards } from '../lib/cardExport'
import type { CardSummary, TagRecord } from '../types'
import { useFiltersStore } from '../stores/filters'
import { useSidebarStore } from '../stores/sidebar'
import Pagination from '../components/Pagination.vue'

const VIEW_MODE_KEY = 'xunji:knowledgeViewMode'
type ViewMode = 'list' | 'card'

const router = useRouter()
const message = useMessage()
const dialog = useDialog()
const filters = useFiltersStore()
const sidebar = useSidebarStore()
const items = ref<CardSummary[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = ref(20)
const loading = ref(false)
/** 多选导出：跨分页保留 id */
const selectedIds = ref(new Set<string>())
const exportBusy = ref(false)

const viewMode = ref<ViewMode>('card')
const publicationStatus = ref<'published' | 'draft'>('published')
const tagManagerOpen = ref(false)
const tagRecords = ref<TagRecord[]>([])
const tagKind = ref<'topic' | 'technology'>('topic')
const tagSources = ref<string[]>([])
const tagTarget = ref('')

const selectedCount = computed(() => selectedIds.value.size)

watch(viewMode, (v) => {
  localStorage.setItem(VIEW_MODE_KEY, v)
})

async function load() {
  loading.value = true
  try {
    const r = await api.listCards({
      cardType: filters.cardType || undefined,
      tags: filters.selectedTags.length ? [...filters.selectedTags] : undefined,
      techStack: filters.selectedTechStacks.length ? [...filters.selectedTechStacks] : undefined,
      publicationStatus: publicationStatus.value,
      page: page.value,
      pageSize: pageSize.value,
    })
    items.value = r.items
    total.value = r.total
  } finally {
    loading.value = false
  }
}

async function openTagManager() {
  tagRecords.value = await api.listTagRecords()
  tagSources.value = []
  tagTarget.value = ''
  tagManagerOpen.value = true
}

const tagOptions = computed(() => tagRecords.value
  .filter((tag) => tag.kind === tagKind.value)
  .map((tag) => ({ label: tag.name, value: tag.name })))

async function mergeSelectedTags() {
  try {
    await api.mergeTags(tagKind.value, tagSources.value, tagTarget.value)
    await Promise.all([sidebar.loadLibraryMeta(), load()])
    tagRecords.value = await api.listTagRecords()
    tagSources.value = []
    tagTarget.value = ''
    message.success('标签已合并并重建检索索引')
  } catch (error) { message.error(error instanceof Error ? error.message : String(error)) }
}

onMounted(() => {
  const raw = localStorage.getItem(VIEW_MODE_KEY)
  if (raw === 'list' || raw === 'card') viewMode.value = raw
  void load()
})

watch(
  [
    () => publicationStatus.value,
    () => filters.cardType,
    () => filters.selectedTags.length,
    () => filters.selectedTechStacks.length,
  ],
  () => {
    page.value = 1
    void load()
  },
)

function removeTagFilter(name: string) {
  const i = filters.selectedTags.indexOf(name)
  if (i >= 0) {
    filters.selectedTags.splice(i, 1)
  }
}

function removeTechFilter(name: string) {
  const i = filters.selectedTechStacks.indexOf(name)
  if (i >= 0) {
    filters.selectedTechStacks.splice(i, 1)
  }
}

function open(c: CardSummary) {
  void router.push({
    name: 'session-detail',
    params: { sessionId: c.sessionId },
    query: { cardId: c.id },
  })
}

function setPage(p: number) {
  page.value = p
  void load()
}

function setPageSize(n: number) {
  pageSize.value = n
  page.value = 1
  void load()
}

function formatTime(iso: string) {
  return iso?.replace('T', ' ').slice(0, 16) ?? '—'
}

function toggleSelect(id: string, checked: boolean) {
  const next = new Set(selectedIds.value)
  if (checked) next.add(id)
  else next.delete(id)
  selectedIds.value = next
}

function selectAllOnPage() {
  const next = new Set(selectedIds.value)
  for (const c of items.value) next.add(c.id)
  selectedIds.value = next
}

function clearSelection() {
  selectedIds.value = new Set()
}

async function onExportSelected() {
  const ids = [...selectedIds.value]
  if (!ids.length) {
    message.warning('请先勾选要导出的笔记')
    return
  }
  exportBusy.value = true
  try {
    const r = await exportSelectedCards(ids)
    if (r.ok && r.count != null) message.success(`已导出 ${r.count} 条笔记`)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  } finally {
    exportBusy.value = false
  }
}

function onExportAll() {
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
        exportBusy.value = true
        try {
          const r = await exportAllCardsToDir()
          if (r.ok && r.count != null) message.success(`已导出 ${r.count} 条笔记`)
        } catch (e) {
          message.error(e instanceof Error ? e.message : String(e))
        } finally {
          exportBusy.value = false
        }
      },
    })
  })()
}
</script>

<template>
  <div class="flex flex-col h-full min-h-0 max-w-5xl mx-auto w-full px-5 pt-5">
    <!-- 顶栏：标题 + 工具栏 -->
    <header class="shrink-0 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between mb-4">
      <div>
        <h1 class="text-base font-semibold text-slate-800 dark:text-slate-100 tracking-tight">
          知识库
        </h1>
        <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
          共 {{ total }} 条记录 · 当前{{ viewMode === 'card' ? '卡片' : '列表' }}视图
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-2 justify-end">
        <div class="bg-slate-100/80 dark:bg-neutral-900/55 p-1 rounded-lg inline-flex">
          <button
            v-for="option in [{ value: 'published', label: '已发布' }, { value: 'draft', label: '待审草稿' }] as const"
            :key="option.value"
            type="button"
            class="segment-pill-btn px-3 py-1 rounded-md text-sm border-0"
            :class="publicationStatus === option.value ? 'bg-white dark:bg-neutral-800 text-slate-900 dark:text-white' : 'bg-transparent text-slate-500'"
            @click="publicationStatus = option.value"
          >
            {{ option.label }}
          </button>
        </div>
        <div class="flex flex-wrap gap-1.5 items-center">
          <n-button size="small" secondary @click="openTagManager">标签治理</n-button>
          <span v-if="selectedCount" class="text-[11px] text-slate-500 dark:text-slate-400">已选 {{ selectedCount }} 条</span>
          <n-button size="small" secondary :disabled="!items.length" @click="selectAllOnPage">
            全选当页
          </n-button>
          <n-button size="small" secondary :disabled="!selectedCount" @click="clearSelection">
            清除选择
          </n-button>
          <n-button
            size="small"
            secondary
            :loading="exportBusy"
            :disabled="exportBusy || !selectedCount"
            @click="onExportSelected"
          >
            <span class="inline-flex items-center gap-1">
              <span class="i-lucide-folder-output w-3.5 h-3.5" />
              导出所选
            </span>
          </n-button>
          <n-button
            type="primary"
            size="small"
            :loading="exportBusy"
            :disabled="exportBusy"
            @click="onExportAll"
          >
            <span class="inline-flex items-center gap-1">
              <span class="i-lucide-archive w-3.5 h-3.5" />
              导出全部笔记
            </span>
          </n-button>
        </div>
        <!-- 视图切换：与顶栏/会话分段条一致，segment-pill-btn 避免 WebView 默认灰底 -->
        <div class="bg-slate-100/80 dark:bg-neutral-900/55 p-1 rounded-lg inline-flex shrink-0">
          <button
            type="button"
            class="segment-pill-btn"
            :class="[
              'flex items-center gap-1.5 px-3 py-1 rounded-md text-sm font-medium transition-colors cursor-pointer border-0 outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/35',
              viewMode === 'list'
                ? 'bg-white dark:bg-neutral-800 text-slate-800 dark:text-slate-100 ring-1 ring-slate-200/90 dark:ring-white/10'
                : 'bg-transparent text-slate-500 hover:text-slate-800 dark:text-neutral-500 dark:hover:text-neutral-200',
            ]"
            @click="viewMode = 'list'"
          >
            <span class="i-lucide-list w-3.5 h-3.5" />
            列表
          </button>
          <button
            type="button"
            class="segment-pill-btn"
            :class="[
              'flex items-center gap-1.5 px-3 py-1 rounded-md text-sm font-medium transition-colors cursor-pointer border-0 outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/35',
              viewMode === 'card'
                ? 'bg-white dark:bg-neutral-800 text-slate-800 dark:text-slate-100 ring-1 ring-slate-200/90 dark:ring-white/10'
                : 'bg-transparent text-slate-500 hover:text-slate-800 dark:text-neutral-500 dark:hover:text-neutral-200',
            ]"
            @click="viewMode = 'card'"
          >
            <span class="i-lucide-layout-grid w-3.5 h-3.5" />
            卡片
          </button>
        </div>
      </div>
    </header>

    <!-- 与侧栏筛选联动：在主区域可摘除条件，无需回到侧栏 -->
    <div
      v-if="filters.hasLibraryFilters"
      class="shrink-0 flex flex-wrap items-center gap-2 mb-3 pb-3 border-b border-slate-100 dark:border-neutral-800"
    >
      <span class="text-[11px] text-slate-500 dark:text-neutral-400">当前筛选</span>
      <n-tag
        v-if="filters.cardType"
        size="small"
        closable
        round
        @close="filters.cardType = ''"
      >
        类型 · {{ getCardTypeLabel(filters.cardType) }}
      </n-tag>
      <n-tag
        v-for="t in filters.selectedTags"
        :key="'kf-tag-' + t"
        size="small"
        closable
        round
        @close="removeTagFilter(t)"
      >
        标签 · {{ t }}
      </n-tag>
      <n-tag
        v-for="s in filters.selectedTechStacks"
        :key="'kf-tech-' + s"
        size="small"
        closable
        round
        type="info"
        @close="removeTechFilter(s)"
      >
        技术栈 · {{ s }}
      </n-tag>
      <n-button size="tiny" quaternary @click="filters.resetLibrary()">
        全部清除
      </n-button>
    </div>

    <!-- 内容区 -->
    <div class="flex-1 min-h-0 overflow-y-auto pb-32">
      <div v-if="loading" class="flex items-center justify-center py-24">
        <n-spin size="medium" />
      </div>

      <n-empty v-else-if="!items.length" description="暂无知识卡片" class="py-16" />

      <!-- 卡片视图；选中 ring-inset：根节点 overflow-hidden 会裁掉外向 ring，见 docs/踩坑/桌面应用-Tailwind-ring与overflow-hidden裁切.md -->
      <div
        v-else-if="viewMode === 'card'"
        class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5"
      >
        <div
          v-for="c in items"
          :key="c.id"
          class="group relative rounded-2xl border bg-white dark:bg-neutral-900 transition-all duration-300 cursor-pointer overflow-hidden flex flex-col"
          :class="[
            selectedIds.has(c.id)
              ? 'ring-2 ring-inset ring-emerald-500 border-transparent bg-emerald-50/30 dark:bg-emerald-950/30'
              : 'border-slate-200/80 dark:border-neutral-700/60 hover:border-emerald-300 dark:hover:border-emerald-700',
          ]"
          @click="open(c)"
        >
          <!-- 卡片主体 -->
          <div class="p-5 flex-1 flex flex-col gap-3">
            <div class="flex items-start justify-between">
              <div class="flex items-center gap-2 flex-wrap">
                <n-tag v-if="c.type" size="small" :bordered="false" type="success" class="font-medium rounded">
                  {{ getCardTypeLabel(c.type) }}
                </n-tag>
                <n-tag
                  v-if="c.value"
                  size="small"
                  :bordered="false"
                  :type="c.value === 'high' ? 'success' : c.value === 'medium' ? 'warning' : 'default'"
                  class="rounded"
                >
                  {{ c.value === 'high' ? '高价值' : c.value === 'medium' ? '中价值' : c.value }}
                </n-tag>
                <n-tag v-if="c.publicationStatus === 'draft'" size="small" :bordered="false" type="warning">草稿</n-tag>
              </div>
              <div class="shrink-0" @click.stop>
                <n-checkbox
                  :checked="selectedIds.has(c.id)"
                  size="small"
                  @update:checked="(v: boolean) => toggleSelect(c.id, v)"
                />
              </div>
            </div>

            <div class="flex-1">
              <h3 class="text-base font-semibold text-slate-800 dark:text-slate-100 leading-snug group-hover:text-emerald-700 dark:group-hover:text-emerald-400 transition-colors">
                {{ c.title }}
              </h3>
              <p class="text-[13px] text-slate-500 dark:text-slate-400 line-clamp-3 mt-2 leading-relaxed">
                {{ c.summary || '暂无摘要' }}
              </p>
            </div>
          </div>

          <!-- 卡片底部 -->
          <div class="px-5 py-3.5 bg-slate-50/50 dark:bg-neutral-800/30 border-t border-slate-100 dark:border-neutral-800 flex flex-col gap-2.5">
            <div class="flex items-center justify-between text-[12px]">
              <span class="flex items-center gap-1.5 font-medium text-slate-700 dark:text-slate-300 bg-white dark:bg-neutral-800 px-2 py-0.5 rounded border border-slate-200 dark:border-neutral-700">
                <span class="i-lucide-folder w-3.5 h-3.5 text-slate-400" />
                {{ c.projectName || '—' }}
              </span>
              <span v-if="c.sourceName" class="flex items-center gap-1.5 text-slate-400 dark:text-slate-500 font-mono bg-white dark:bg-neutral-800 px-1.5 py-0.5 rounded border border-slate-200 dark:border-neutral-700">
                <span class="i-lucide-terminal w-3 h-3" />
                {{ c.sourceName }}
              </span>
            </div>
            <div class="flex items-center justify-between text-[11px] text-slate-400 dark:text-slate-500 mt-1">
              <span class="flex items-center gap-1.5">
                <span class="i-lucide-clock w-3 h-3" />
                {{ formatTime(c.updatedAt) }}
              </span>
              <span class="opacity-0 group-hover:opacity-100 transition-opacity font-medium text-emerald-600 dark:text-emerald-400 flex items-center gap-1">
                阅读笔记 <span class="i-lucide-arrow-right w-3 h-3" />
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- 列表视图；选中态同卡片：ring-inset 避免 overflow-hidden 裁切 -->
      <div v-else class="space-y-4">
        <div
          v-for="c in items"
          :key="'list-'+c.id"
          class="group relative rounded-xl border bg-white dark:bg-neutral-900 transition-all duration-300 cursor-pointer overflow-hidden flex items-center"
          :class="[
            selectedIds.has(c.id)
              ? 'ring-2 ring-inset ring-emerald-500 border-transparent bg-emerald-50/30 dark:bg-emerald-950/30'
              : 'border-slate-200/80 dark:border-neutral-700/60 hover:border-emerald-300 dark:hover:border-emerald-700',
          ]"
          @click="open(c)"
        >
          <!-- Checkbox -->
          <div class="pl-4 shrink-0" @click.stop>
            <n-checkbox
              :checked="selectedIds.has(c.id)"
              size="small"
              @update:checked="(v: boolean) => toggleSelect(c.id, v)"
            />
          </div>

          <!-- 列表项内容 -->
          <div class="flex-1 p-4 flex items-center gap-4">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-1">
                <n-tag v-if="c.type" size="small" :bordered="false" type="success" class="font-medium rounded !text-[11px]">
                  {{ getCardTypeLabel(c.type) }}
                </n-tag>
                <n-tag
                  v-if="c.value"
                  size="small"
                  :bordered="false"
                  :type="c.value === 'high' ? 'success' : c.value === 'medium' ? 'warning' : 'default'"
                  class="rounded !text-[11px]"
                >
                  {{ c.value === 'high' ? '高价值' : c.value === 'medium' ? '中价值' : c.value }}
                </n-tag>
                <h3 class="text-[15px] font-semibold text-slate-800 dark:text-slate-100 truncate group-hover:text-emerald-700 dark:group-hover:text-emerald-400 transition-colors">
                  {{ c.title }}
                </h3>
              </div>
              <p class="text-[13px] text-slate-500 dark:text-slate-400 truncate">
                {{ c.summary || '暂无摘要' }}
              </p>
            </div>

            <!-- 元数据 -->
            <div class="flex items-center gap-4 shrink-0 text-[12px] text-slate-400 dark:text-slate-500">
              <span class="flex items-center gap-1.5 w-24">
                <span class="i-lucide-folder w-3.5 h-3.5" />
                <span class="truncate">{{ c.projectName || '—' }}</span>
              </span>
              <span v-if="c.sourceName" class="flex items-center gap-1.5 w-24">
                <span class="i-lucide-terminal w-3 h-3" />
                <span class="truncate">{{ c.sourceName }}</span>
              </span>
              <span class="flex items-center gap-1.5 w-32">
                <span class="i-lucide-clock w-3.5 h-3.5" />
                {{ formatTime(c.updatedAt) }}
              </span>
            </div>

            <!-- 操作按钮 -->
            <div class="opacity-0 group-hover:opacity-100 transition-opacity duration-200 shrink-0 w-24 flex justify-end">
              <n-button secondary size="small" class="rounded-lg">
                <template #icon>
                  <span class="i-lucide-book-open w-4 h-4" />
                </template>
                阅读笔记
              </n-button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 分页 -->
    <footer
      v-if="total > 0"
      class="shrink-0 py-3 border-t border-slate-200 dark:border-neutral-800"
    >
      <Pagination
        :page="page"
        :page-size="pageSize"
        :total="total"
        @update:page="setPage"
        @update:page-size="setPageSize"
      />
    </footer>

    <n-modal v-model:show="tagManagerOpen" preset="card" title="标签治理" class="max-w-lg">
      <div class="space-y-3">
        <n-select v-model:value="tagKind" :options="[{ label: '主题标签', value: 'topic' }, { label: '技术项', value: 'technology' }]" />
        <n-select v-model:value="tagSources" multiple filterable :options="tagOptions" placeholder="选择要重命名或合并的标签" />
        <n-input v-model:value="tagTarget" placeholder="目标名称；选择一个来源即可完成重命名" />
        <div class="flex justify-end gap-2">
          <n-button @click="tagManagerOpen = false">关闭</n-button>
          <n-button type="primary" :disabled="!tagSources.length || !tagTarget.trim()" @click="mergeSelectedTags">合并</n-button>
        </div>
      </div>
    </n-modal>
  </div>
</template>
