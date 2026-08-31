<script setup lang="ts">
/**
 * 知识库 —— 档案索引的舒展 / 紧凑双密度视图，内容区固定高度可滚动，分页贴底。
 *
 * 视图模式持久化：localStorage key `chattake:knowledgeViewMode`
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import {
  NSpin,
  NTag,
  NCheckbox,
  NButton,
  NDropdown,
  NInput,
  NModal,
  NSelect,
  NTooltip,
  useMessage,
  useDialog,
} from 'naive-ui'
import type { DropdownOption } from 'naive-ui'
import { getCardTypeLabel } from '@chattake/shared'
import { api } from '../lib/tauri'
import { exportAllCardsToDir, exportSelectedCards } from '../lib/cardExport'
import type { CardSummary, TagRecord } from '../types'
import { useFiltersStore } from '../stores/filters'
import { useSidebarStore } from '../stores/sidebar'
import Pagination from '../components/Pagination.vue'

const VIEW_MODE_KEY = 'chattake:knowledgeViewMode'
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
const exportOptions = computed<DropdownOption[]>(() => [
  { key: 'selected', label: `导出所选${selectedCount.value ? `（${selectedCount.value}）` : ''}`, disabled: !selectedCount.value },
  { key: 'all', label: '导出全部笔记' },
])

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

function onExportSelect(key: string | number) {
  if (key === 'selected') void onExportSelected()
  if (key === 'all') onExportAll()
}
</script>

<template>
  <div class="flex flex-col h-full min-h-0 max-w-5xl mx-auto w-full px-5 pt-5">
    <!-- 顶栏：标题 + 工具栏 -->
    <header class="library-header">
      <div class="library-heading">
        <p>KNOWLEDGE ARCHIVE</p>
        <div class="library-title-row">
          <h1>知识笔记</h1>
          <span>{{ total }} 条</span>
        </div>
      </div>
      <div class="library-actions">
        <div class="publication-tabs" role="group" aria-label="发布状态">
          <button
            v-for="option in [{ value: 'published', label: '已发布' }, { value: 'draft', label: '待审草稿' }] as const"
            :key="option.value"
            type="button"
            class="ui-tab publication-tab"
            :class="{ active: publicationStatus === option.value }"
            :aria-pressed="publicationStatus === option.value"
            @click="publicationStatus = option.value"
          >
            {{ option.label }}
          </button>
        </div>
        <span class="library-divider" aria-hidden="true" />
        <n-button size="small" quaternary @click="openTagManager">
          <template #icon><span class="i-lucide-tags w-4 h-4" /></template>
          标签治理
        </n-button>
        <n-button v-if="items.length" size="small" quaternary @click="selectAllOnPage">
          选择本页
        </n-button>
        <n-dropdown trigger="click" :options="exportOptions" @select="onExportSelect">
          <n-button size="small" secondary :loading="exportBusy" :disabled="exportBusy">
            <template #icon><span class="i-lucide-download w-4 h-4" /></template>
            导出
          </n-button>
        </n-dropdown>
        <div class="density-switch" role="group" aria-label="列表密度">
          <n-tooltip trigger="hover"><template #trigger>
            <button type="button" class="ui-icon-toggle" :class="{ active: viewMode === 'list' }" :aria-pressed="viewMode === 'list'" aria-label="紧凑视图" @click="viewMode = 'list'">
              <span class="i-lucide-list w-4 h-4" aria-hidden="true" />
            </button>
          </template>紧凑视图</n-tooltip>
          <n-tooltip trigger="hover"><template #trigger>
            <button type="button" class="ui-icon-toggle" :class="{ active: viewMode === 'card' }" :aria-pressed="viewMode === 'card'" aria-label="舒展视图" @click="viewMode = 'card'">
              <span class="i-lucide-rows-3 w-4 h-4" aria-hidden="true" />
            </button>
          </template>舒展视图</n-tooltip>
        </div>
      </div>
    </header>

    <div v-if="selectedCount" class="selection-bar" aria-live="polite">
      <span><strong>{{ selectedCount }}</strong> 条笔记已选择</span>
      <div>
        <n-button size="tiny" quaternary @click="clearSelection">取消选择</n-button>
        <n-button size="tiny" secondary :loading="exportBusy" @click="onExportSelected">导出所选</n-button>
      </div>
    </div>

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

      <div v-else-if="!items.length" class="knowledge-empty">
        <span class="i-lucide-book-dashed knowledge-empty-icon" aria-hidden="true" />
        <p>{{ publicationStatus === 'published' ? '还没有已发布的知识' : '没有等待审核的草稿' }}</p>
        <span>{{ publicationStatus === 'published' ? '从有价值的对话中提炼第一条可复用笔记。' : '中价值分析结果和重新分析结果会先出现在这里。' }}</span>
        <n-button v-if="publicationStatus === 'published'" size="small" secondary @click="router.push({ name: 'sessions' })">前往对话档案</n-button>
        <n-button v-else size="small" quaternary @click="publicationStatus = 'published'">查看已发布</n-button>
      </div>

      <div v-else class="knowledge-index" :data-density="viewMode">
        <article
          v-for="c in items"
          :key="c.id"
          class="knowledge-row"
          :class="{ selected: selectedIds.has(c.id) }"
        >
          <button type="button" class="knowledge-hit" :aria-label="`阅读知识：${c.title}`" @click="open(c)" />
          <span class="knowledge-rail" :data-value="c.value || 'none'" />
          <div class="knowledge-select" @click.stop>
            <n-checkbox
              :checked="selectedIds.has(c.id)"
              :aria-label="`选择 ${c.title}`"
              @update:checked="(value: boolean) => toggleSelect(c.id, value)"
            />
          </div>
          <div class="knowledge-copy">
            <div class="knowledge-eyebrow">
              <span v-if="c.type">{{ getCardTypeLabel(c.type) }}</span>
              <span v-if="c.value">{{ c.value === 'high' ? '高价值' : c.value === 'medium' ? '中价值' : c.value }}</span>
              <span v-if="c.publicationStatus === 'draft'" class="draft-mark">草稿</span>
            </div>
            <h3>{{ c.title }}</h3>
            <p>{{ c.summary || '暂无摘要' }}</p>
            <div v-if="c.tags.length || c.technologies.length" class="knowledge-tags">
              <span v-for="tag in c.tags.slice(0, 3)" :key="tag"># {{ tag }}</span>
              <span v-for="tech in c.technologies.slice(0, 3)" :key="tech" class="technology">{{ tech }}</span>
            </div>
          </div>
          <div class="knowledge-meta">
            <span>{{ c.projectName || '未关联项目' }}</span>
            <span v-if="c.sourceName">{{ c.sourceName }}</span>
            <time>{{ formatTime(c.updatedAt) }}</time>
          </div>
          <span class="i-lucide-arrow-up-right knowledge-arrow" aria-hidden="true" />
        </article>
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

<style scoped>
.library-header { display:flex; align-items:flex-end; justify-content:space-between; gap:24px; flex-shrink:0; margin-bottom:16px; }
.library-heading p { margin:0 0 7px; color:var(--muted); font-size:9px; letter-spacing:.16em; }
.library-title-row { display:flex; align-items:baseline; gap:10px; }
.library-title-row h1 { margin:0; color:var(--ink); font-family:var(--font-editorial); font-size:24px; font-weight:500; letter-spacing:.02em; }
.library-title-row span { color:var(--muted); font-size:11px; }
.library-actions { display:flex; align-items:center; justify-content:flex-end; gap:5px; min-width:0; }
.publication-tabs { display:flex; align-items:center; gap:16px; margin-right:5px; }
.publication-tab { position:relative; height:32px; padding:0; background:transparent; color:var(--muted); font-size:12px; cursor:pointer; transition:color .18s var(--ease-out); }
.publication-tab:hover,.publication-tab.active { color:var(--ink); }
.publication-tab.active::after { content:''; position:absolute; right:0; bottom:0; left:0; height:1px; background:var(--vermilion); }
.library-divider { width:1px; height:18px; margin:0 4px; background:var(--line); }
.density-switch { display:flex; align-items:center; gap:2px; margin-left:4px; padding:2px; border:1px solid var(--line); border-radius:var(--control-radius); }
.ui-icon-toggle { display:grid; width:28px; height:28px; place-items:center; border-radius:6px; background:transparent; color:var(--muted); cursor:pointer; transition:color .16s var(--ease-out), background-color .16s var(--ease-out); }
.ui-icon-toggle:hover { color:var(--ink); }
.ui-icon-toggle.active { background:var(--surface); color:var(--pine); box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--line) 72%,transparent); }
.selection-bar { display:flex; align-items:center; justify-content:space-between; gap:16px; flex-shrink:0; margin-bottom:12px; padding:8px 10px 8px 12px; border:1px solid color-mix(in srgb,var(--pine) 28%,var(--line)); border-radius:var(--control-radius); background:color-mix(in srgb,var(--pine) 7%,transparent); color:var(--ink-soft); font-size:11px; }
.selection-bar strong { color:var(--pine); }
.selection-bar > div { display:flex; gap:4px; }
.knowledge-index { border-top:1px solid var(--line); }
.knowledge-row { position:relative; display:grid; grid-template-columns:minmax(0,1fr) 150px 22px; gap:22px; align-items:center; min-height:142px; padding:20px 18px 20px 48px; border-bottom:1px solid var(--line); transition:background-color .14s ease; }
.knowledge-row:hover,.knowledge-row.selected { background:color-mix(in srgb,var(--surface) 72%,transparent); }
.knowledge-hit { position:absolute; inset:0; z-index:0; appearance:none; border:0; background:transparent; cursor:pointer; }
.knowledge-hit:focus-visible { outline:2px solid var(--pine); outline-offset:-2px; }
.knowledge-rail { position:absolute; left:0; top:24px; bottom:24px; width:2px; background:var(--line-strong); pointer-events:none; }
.knowledge-rail[data-value='high'] { background:var(--vermilion); }
.knowledge-rail[data-value='medium'] { background:var(--pine); }
.knowledge-select { position:absolute; left:16px; top:22px; z-index:2; }
.knowledge-copy { position:relative; z-index:1; min-width:0; pointer-events:none; }
.knowledge-eyebrow { display:flex; gap:16px; margin-bottom:7px; color:var(--muted); font-size:10px; letter-spacing:.09em; text-transform:uppercase; }
.knowledge-eyebrow span + span::before { content:'·'; margin-right:16px; color:var(--line-strong); }
.knowledge-eyebrow .draft-mark { color:var(--vermilion); }
.knowledge-copy h3 { margin:0; color:var(--ink); font-family:var(--font-editorial); font-size:18px; font-weight:600; line-height:1.45; }
.knowledge-copy p { display:-webkit-box; margin:6px 0 0; overflow:hidden; color:var(--muted); font-size:12px; line-height:1.65; -webkit-box-orient:vertical; -webkit-line-clamp:2; }
.knowledge-tags { display:flex; flex-wrap:wrap; gap:11px; margin-top:9px; color:var(--pine); font-size:10px; }
.knowledge-tags .technology { color:var(--ink-soft); }
.knowledge-meta { position:relative; z-index:1; display:flex; flex-direction:column; gap:6px; min-width:0; color:var(--muted); font-size:11px; pointer-events:none; }
.knowledge-meta span { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.knowledge-arrow { position:relative; z-index:1; width:15px; height:15px; color:var(--muted); opacity:.35; pointer-events:none; }
.knowledge-row:hover .knowledge-arrow { color:var(--pine); opacity:1; }
.knowledge-index[data-density='list'] .knowledge-row { min-height:112px; padding-top:15px; padding-bottom:15px; }
.knowledge-empty { display:flex; min-height:360px; flex-direction:column; align-items:flex-start; justify-content:center; max-width:430px; margin:0 auto; color:var(--muted); }
.knowledge-empty-icon { width:30px; height:30px; margin-bottom:18px; color:var(--line-strong); }
.knowledge-empty p { margin:0 0 7px; color:var(--ink); font-family:var(--font-editorial); font-size:20px; }
.knowledge-empty > span:not(.knowledge-empty-icon) { margin-bottom:18px; font-size:12px; line-height:1.7; }
@media (max-width:1100px) { .library-header { align-items:flex-start; flex-direction:column; gap:12px; } .library-actions { justify-content:flex-start; flex-wrap:wrap; } }
@media (max-width:1000px) { .knowledge-row { grid-template-columns:minmax(0,1fr) 22px; } .knowledge-meta { display:none; } }
</style>
