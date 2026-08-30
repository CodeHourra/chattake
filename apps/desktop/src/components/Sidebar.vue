<script setup lang="ts">
import { computed, h, onMounted, ref, watch } from 'vue'
import {
  NTree,
  NEmpty,
  NSpin,
  NTooltip,
  NButton,
  NCollapse,
  NCollapseItem,
  useDialog,
  useMessage,
} from 'naive-ui'
import type { TreeOption } from 'naive-ui'
import { useUiStore } from '../stores/ui'
import { useFiltersStore } from '../stores/filters'
import { useSessionsStore } from '../stores/sessions'
import { useSidebarStore } from '../stores/sidebar'
import { getCardTypeLabel } from '@chattake/shared'
import { api } from '../lib/tauri'
import type { SessionFilterPayload } from '../types'

// 品牌图标（官方 logo，通过 Vite asset URL 导入）
import claudeCodeIcon from '../assets/brands/claude-code.png?url'
import cursorIcon from '../assets/brands/cursor.png?url'
import codebuddyIcon from '../assets/brands/codebuddy.svg?url'
import openaiIcon from '../assets/brands/openai.svg?url'

const ui = useUiStore()
const filters = useFiltersStore()
const sessions = useSessionsStore()
const sidebar = useSidebarStore()
const dialog = useDialog()
const message = useMessage()

/** 侧栏批量移除会话时的按钮 loading */
const removeGroupBusy = ref(false)

/**
 * 会话整理模式：目录树显示多选框，勾选多个分组后可批量删除（筛选并集去重）。
 * 进入整理时列表重置为全部对话；单击节点筛选列表的逻辑仍保留。
 */
const sessionOrganizeMode = ref(false)
/** 整理模式下 NTree 勾选的分组 key（与 treeData 中 key 一致） */
const checkedKeys = ref<string[]>([])

// ────────────────── 数据源配置 ──────────────────

/**
 * 数据源元信息：每个 source_id 对应其品牌标签和图标
 * imgIcon 优先使用品牌 SVG；fallbackIcon 为 UnoCSS 类名兜底
 */
const SOURCE_META: Record<string, { label: string; imgIcon?: string; fallbackIcon: string }> = {
  'claude-code': { label: 'Claude Code', imgIcon: claudeCodeIcon, fallbackIcon: 'i-lucide-terminal' },
  'cursor': { label: 'Cursor', imgIcon: cursorIcon, fallbackIcon: 'i-lucide-mouse-pointer-click' },
  codebuddy: { label: 'CodeBuddy', imgIcon: codebuddyIcon, fallbackIcon: 'i-lucide-code' },
  codex: { label: 'Codex', imgIcon: openaiIcon, fallbackIcon: 'i-lucide-code-2' },
  omp: { label: 'Oh My Pi', fallbackIcon: 'i-lucide-orbit' },
  pi: { label: 'Pi', fallbackIcon: 'i-lucide-circle-dot' },
}

function getSourceMeta(sourceId: string) {
  return SOURCE_META[sourceId] ?? { label: sourceId, fallbackIcon: 'i-lucide-folder' }
}

/** 根据 source meta 渲染品牌图标前缀 */
function renderSourceIcon(sourceId: string) {
  const meta = getSourceMeta(sourceId)
  if (meta.imgIcon) {
    return h('img', { src: meta.imgIcon, alt: '', 'aria-hidden': 'true', class: 'brand-icon', width: 16, height: 16 })
  }
  return h('span', { class: `${meta.fallbackIcon} w-4 h-4 opacity-70` })
}

/** host 显示名映射，"local" → "本地" */
const HOST_LABELS: Record<string, string> = {
  local: '本地',
  localhost: '本地',
}

function hostLabel(host: string) {
  return HOST_LABELS[host] ?? host
}

/**
 * 树选中 / 展开：必须在 treeData 之前声明。
 * treeData 依赖 selectedKeys，项目节点 prefix（拐角线 + 文件夹）才能随选中切换颜色，与 design/ui_demo 一致。
 */
const selectedKeys = ref<string[]>([])
/** 默认展开由下方 watch(treeData) 在首屏填充 */
const expandedKeys = ref<string[]>([])

// ────────────────── NTree 数据构建 ──────────────────

/**
 * 将扁平的 SessionGroupCount[] 组装成 NTree 需要的 TreeOption[]
 *
 * 树结构：数据源 → 主机 → 项目
 * key 编码规则：
 *   "src:<sourceId>" / "host:<sourceId>/<host>" / "proj:<sourceId>/<host>/<project>"
 */
const treeData = computed<TreeOption[]>(() => {
  /** 当前选中节点集合，用于项目行文件夹 icon 高亮（emerald / slate） */
  const selectedSet = new Set(selectedKeys.value)
  const groups = sidebar.sessionGroups
  const sourceMap = new Map<string, Map<string, { project: string | null; count: number }[]>>()

  for (const g of groups) {
    if (!sourceMap.has(g.sourceId)) sourceMap.set(g.sourceId, new Map())
    const hostMap = sourceMap.get(g.sourceId)!
    if (!hostMap.has(g.sourceHost)) hostMap.set(g.sourceHost, [])
    hostMap.get(g.sourceHost)!.push({ project: g.projectName, count: g.count })
  }

  const result: TreeOption[] = []
  for (const [sourceId, hostMap] of sourceMap) {
    const hostChildren: TreeOption[] = []
    let sourceTotal = 0

    for (const [host, items] of hostMap) {
      const projChildren: TreeOption[] = []
      let hostTotal = 0
      for (const item of items) {
        hostTotal += item.count
        const projName = item.project ?? '(未关联项目)'
        const projKey = `proj:${sourceId}/${host}/${projName}`
        const projSelected = selectedSet.has(projKey)
        projChildren.push({
          key: projKey,
          label: projName,
          isLeaf: true,
          // 与 ui_demo 一致：层级拐角线 + 文件夹；选中时文件夹用 emerald
          prefix: () =>
            h('span', { class: 'flex items-center gap-0.5 flex-shrink-0' }, [
              h('span', {
                class: 'i-lucide-corner-down-right w-3 h-3 text-slate-300 dark:text-neutral-600',
              }),
              h('span', {
                class: projSelected
                  ? 'i-lucide-folder w-4 h-4 text-emerald-500 dark:text-emerald-400'
                  : 'i-lucide-folder w-4 h-4 text-slate-400 dark:text-neutral-500',
              }),
            ]),
          suffix: () => h('span', { class: 'ml-auto text-[10px] text-slate-400 font-mono' }, String(item.count)),
        })
      }
      sourceTotal += hostTotal
      hostChildren.push({
        key: `host:${sourceId}/${host}`,
        label: hostLabel(host),
        children: projChildren,
        prefix: () => h('span', { class: 'i-lucide-server w-3.5 h-3.5 opacity-50' }),
        suffix: () => h('span', { class: 'ml-auto text-[10px] text-slate-400 font-mono' }, String(hostTotal)),
      })
    }

    result.push({
      key: `src:${sourceId}`,
      label: getSourceMeta(sourceId).label,
      children: hostChildren,
      prefix: () => renderSourceIcon(sourceId),
      suffix: () => h('span', { class: 'ml-auto text-[10px] text-slate-400 font-mono' }, String(sourceTotal)),
    })
  }
  return result
})

const totalSessions = computed(() =>
  sidebar.sessionGroups.reduce((sum, g) => sum + g.count, 0),
)

/** 自定义树节点 label 渲染：超长截断 + tooltip；项目行未选中/选中字重与 demo 对齐 */
function renderLabel({ option }: { option: TreeOption }) {
  const label = String(option.label ?? '')
  const key = String(option.key ?? '')
  const isProj = key.startsWith('proj:')
  const projSelected = isProj && selectedKeys.value.includes(key)
  const labelClass =
    'sidebar-label' +
    (isProj
      ? projSelected
        ? ' font-medium text-slate-800 dark:text-slate-100'
        : ' text-slate-600 dark:text-slate-300'
      : '')
  return h(NTooltip, {
    trigger: 'hover',
    delay: 500,
    placement: 'right',
  }, {
    trigger: () => h('span', {
      class: labelClass,
      style: 'display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;',
    }, label),
    default: () => label,
  })
}

watch(treeData, (data) => {
  if (expandedKeys.value.length === 0 && data.length > 0) {
    expandedKeys.value = data.map((n) => n.key as string)
  }
}, { immediate: true })

/** 将树节点 key 转为后端 `SessionFilters`（并合并当前 `statusFilter`） */
function treeKeyToFilterPayload(key: string): SessionFilterPayload {
  const status = filters.statusFilter || null
  if (key.startsWith('src:')) {
    return { source: key.slice(4), host: null, project: null, status }
  }
  if (key.startsWith('host:')) {
    const parts = key.slice(5).split('/')
    return {
      source: parts[0],
      host: parts.slice(1).join('/') || null,
      project: null,
      status,
    }
  }
  if (key.startsWith('proj:')) {
    const rest = key.slice(5)
    const firstSlash = rest.indexOf('/')
    const sourceId = rest.slice(0, firstSlash)
    const afterSource = rest.slice(firstSlash + 1)
    const secondSlash = afterSource.indexOf('/')
    const host = afterSource.slice(0, secondSlash)
    const project = afterSource.slice(secondSlash + 1)
    return { source: sourceId, host, project, status }
  }
  throw new Error(`无效的树节点 key: ${key}`)
}

function exitSessionOrganizeMode() {
  sessionOrganizeMode.value = false
  checkedKeys.value = []
}

/** 进入整理：列表回到全部对话，便于专注勾选分组 */
function enterSessionOrganizeMode() {
  checkedKeys.value = []
  filters.resetSessions()
  selectedKeys.value = []
  sessions.page = 1
  void sessions.loadPage()
  sessionOrganizeMode.value = true
}

function toggleSessionOrganizeMode() {
  if (sessionOrganizeMode.value) {
    exitSessionOrganizeMode()
  } else {
    enterSessionOrganizeMode()
  }
}

/** 解析 key 并设置对应的筛选条件 */
function onTreeSelect(keys: string[]) {
  if (sessionOrganizeMode.value) {
    selectedKeys.value = keys.length ? [keys[0] as string] : []
    return
  }

  const key = keys[0]
  if (!key) return

  if (key.startsWith('src:')) {
    const sourceId = key.slice(4)
    filters.sourceId = sourceId
    filters.sourceHost = ''
    filters.projectQuery = ''
  } else if (key.startsWith('host:')) {
    const parts = key.slice(5).split('/')
    filters.sourceId = parts[0]
    filters.sourceHost = parts.slice(1).join('/')
    filters.projectQuery = ''
  } else if (key.startsWith('proj:')) {
    const rest = key.slice(5)
    const firstSlash = rest.indexOf('/')
    const sourceId = rest.slice(0, firstSlash)
    const afterSource = rest.slice(firstSlash + 1)
    const secondSlash = afterSource.indexOf('/')
    const host = afterSource.slice(0, secondSlash)
    const project = afterSource.slice(secondSlash + 1)
    filters.sourceId = sourceId
    filters.sourceHost = host
    filters.projectQuery = project
  }
  selectedKeys.value = keys
  sessions.page = 1
  void sessions.loadPage()
}

/** "全部对话"按钮：清空筛选 + 清空树选中 + 重新加载 */
function selectAll() {
  exitSessionOrganizeMode()
  filters.resetSessions()
  selectedKeys.value = []
  sessions.page = 1
  void sessions.loadPage()
}

/** 当前是否处于"全部对话"模式 */
const isAllMode = computed(() => selectedKeys.value.length === 0)

/** 仅在「会话整理」且已勾选分组时展示移除按钮 */
const showRemoveSessionsButton = computed(
  () =>
    !sidebar.groupsLoading
    && treeData.value.length > 0
    && sessionOrganizeMode.value
    && checkedKeys.value.length > 0,
)

/**
 * summaryText 用于确认框文案。
 */
async function confirmAndRemoveSessionGroups(
  groups: SessionFilterPayload[],
  summaryText: string,
) {
  if (groups.length === 0 || removeGroupBusy.value)
    return

  removeGroupBusy.value = true
  let n = 0
  try {
    n = await api.countSessionsByFilterGroups(groups)
  }
  catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
    removeGroupBusy.value = false
    return
  }
  removeGroupBusy.value = false

  if (n === 0) {
    message.info('没有可删除的会话')
    return
  }

  dialog.warning({
    title: '移除会话',
    content: `将永久删除 ${summaryText} 所覆盖范围内当前匹配的 ${n} 条会话及其消息、关联笔记。多选时同一会话只计一次。仅删除应用内数据，不会删除本地目录中的原始文件。此操作不可恢复。`,
    positiveText: '移除',
    negativeText: '取消',
    onPositiveClick: async () => {
      removeGroupBusy.value = true
      try {
        const deleted = await api.deleteSessionsByFilterGroups(groups)
        message.success(`已移除 ${deleted} 条会话`)
        exitSessionOrganizeMode()
        selectAll()
        await sidebar.loadSessionGroups()
        await sessions.loadPage()
      }
      catch (e) {
        message.error(e instanceof Error ? e.message : String(e))
        throw e
      }
      finally {
        removeGroupBusy.value = false
      }
    },
  })
}

/** 整理模式：移除勾选的多组并集 */
function removeCheckedSessionGroups() {
  if (!sessionOrganizeMode.value || checkedKeys.value.length === 0)
    return
  const groups = checkedKeys.value.map(k => treeKeyToFilterPayload(k))
  const summary = `已选的 ${groups.length} 个分组`
  void confirmAndRemoveSessionGroups(groups, summary)
}

function onUpdateCheckedKeys(keys: Array<string | number>) {
  checkedKeys.value = keys.map(String)
}

const totalCards = computed(() =>
  sidebar.cardTypes.reduce((sum, t) => sum + t.count, 0),
)

function selectCardType(typeName: string) {
  filters.cardType = filters.cardType === typeName ? '' : typeName
}

function toggleTag(tagName: string) {
  const idx = filters.selectedTags.indexOf(tagName)
  if (idx >= 0) {
    filters.selectedTags.splice(idx, 1)
  } else {
    filters.selectedTags.push(tagName)
  }
}

/** 技术栈 chip 切换（与标签同为多选 AND，写入 filters.selectedTechStacks） */
function toggleTechStack(name: string) {
  const idx = filters.selectedTechStacks.indexOf(name)
  if (idx >= 0) {
    filters.selectedTechStacks.splice(idx, 1)
  } else {
    filters.selectedTechStacks.push(name)
  }
}

/**
 * 知识库筛选折叠面板：默认全展开；display-directive=if 折叠时卸载内容减轻 DOM。
 * 不设置 max-height / overflow-y，仅由侧栏最外层滚动。
 */
const libraryCollapseExpanded = ref<string[]>(['cardType', 'tags', 'techStack'])

// ────────────────── 初始化与 Tab 切换时刷新 ──────────────────

onMounted(() => {
  if (ui.activeTab === 'sessions') {
    void sidebar.loadSessionGroups()
  } else {
    void sidebar.loadLibraryMeta()
  }
})

watch(() => ui.activeTab, (tab) => {
  if (tab === 'sessions') {
    void sidebar.loadSessionGroups()
  } else {
    exitSessionOrganizeMode()
    void sidebar.loadLibraryMeta()
  }
})
</script>

<template>
  <!-- min-h-0：参与 flex 布局时让内层 overflow-y-auto 能正确截断高度，单滚动条生效 -->
  <aside class="glass-sidebar shrink-0 flex flex-col min-h-0 border-r select-none">
    <div class="flex-1 overflow-y-auto">

      <!-- ============== 对话记录模式 ============== -->
      <template v-if="ui.activeTab === 'sessions'">
        <!-- 顶部操作区 -->
        <div class="p-3 pb-2 space-y-2 border-b border-slate-100 dark:border-neutral-800">
          <!-- 全部对话按钮 -->
          <button
            type="button"
            class="appearance-none border-0 bg-transparent w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md transition-colors cursor-pointer text-sm font-medium"
            :class="isAllMode
              ? 'all-active'
              : 'text-slate-700 dark:text-neutral-300 hover:bg-slate-100 dark:hover:bg-neutral-900'"
            :aria-pressed="isAllMode"
            @click="selectAll"
          >
            <span class="i-lucide-messages-square w-4 h-4 shrink-0" />
            <span>全部对话</span>
            <span class="ml-auto text-xs opacity-80 tabular-nums">{{ totalSessions }}</span>
          </button>

          <!-- 会话整理按钮 -->
          <n-tooltip
            v-if="!sidebar.groupsLoading && treeData.length"
            placement="bottom"
            trigger="hover"
          >
            <template #trigger>
              <n-button
                size="small"
                block
                secondary
                :type="sessionOrganizeMode ? 'primary' : 'default'"
                @click="toggleSessionOrganizeMode"
              >
                <template #icon>
                  <span class="i-lucide-list-checks w-4 h-4" />
                </template>
                {{ sessionOrganizeMode ? '退出整理' : '会话整理' }}
              </n-button>
            </template>
            点「会话整理」后勾选目录中的分组，即可批量移除（仅删除应用内记录）。
          </n-tooltip>
        </div>

        <!-- 整理模式提示与移除按钮 -->
        <div
          v-if="sessionOrganizeMode && !sidebar.groupsLoading && treeData.length"
          class="px-3 py-2 space-y-1.5"
        >
          <p class="text-[11px] text-slate-500 dark:text-neutral-400 leading-snug px-0.5">
            勾选目录中的多个分组，再点「移除已选分组会话」。与顶部列表状态筛选组合时，每组都会带上当前状态条件。
          </p>
          <n-button
            v-if="showRemoveSessionsButton"
            secondary
            type="error"
            size="small"
            block
            :loading="removeGroupBusy"
            :disabled="sessions.loading"
            @click="removeCheckedSessionGroups"
          >
            <template #icon>
              <span class="i-lucide-trash-2 w-4 h-4" />
            </template>
            移除已选分组会话
          </n-button>
        </div>

        <!-- 来源项目标题 -->
        <div class="px-4 pt-3 pb-2">
          <div class="text-[11px] font-semibold text-slate-400 dark:text-neutral-500 uppercase tracking-widest mb-2 flex items-center gap-1.5">
            <span class="i-lucide-folders w-3.5 h-3.5" /> 来源项目
          </div>
        </div>

        <!-- 加载中 -->
        <div v-if="sidebar.groupsLoading" class="flex items-center justify-center py-8">
          <n-spin size="small" />
        </div>

        <!-- NTree 目录树 -->
        <n-tree
          v-else-if="treeData.length"
          :data="treeData"
          :selected-keys="selectedKeys"
          :expanded-keys="expandedKeys"
          :checked-keys="checkedKeys"
          :checkable="sessionOrganizeMode"
          :cascade="true"
          :render-label="renderLabel"
          block-line
          selectable
          expand-on-click
          class="px-1 py-1 sidebar-tree"
          @update:selected-keys="onTreeSelect"
          @update:checked-keys="onUpdateCheckedKeys"
          @update:expanded-keys="(keys: string[]) => expandedKeys = keys"
        />

        <!-- 空状态 -->
        <div v-else class="py-8 px-4">
          <n-empty size="small" description="暂无对话数据，请先同步" />
        </div>
      </template>

      <!-- ============== 知识库模式 ============== -->
      <template v-else>
        <!-- 不单独 overflow：与外层 aside 共用一个滚动条，避免类型/标签/技术栈多重竖条 -->
        <div class="library-filter-panel">
          <!-- 有筛选时提供一键重置，避免类型/标签/技术栈分散清除 -->
          <div
            v-if="filters.hasLibraryFilters"
            class="filter-summary"
          >
            <span>筛选已启用</span>
            <n-button size="tiny" quaternary class="shrink-0" @click="filters.resetLibrary()">
              清除
            </n-button>
          </div>

          <!-- 类型 / 标签 / 技术栈：n-collapse 折叠；不设内层滚动，仅侧栏最外层 overflow-y-auto -->
          <n-collapse
            v-model:expanded-names="libraryCollapseExpanded"
            display-directive="if"
            :accordion="false"
            size="small"
            class="library-filters-collapse -mx-1"
          >
            <n-collapse-item name="cardType">
              <template #header>
                <div class="flex items-center gap-1.5 min-w-0 flex-1 pr-1">
                  <span class="i-lucide-layout-grid w-3.5 h-3.5 shrink-0 opacity-70" />
                  <span class="text-[11px] font-semibold text-slate-500 dark:text-neutral-400 uppercase tracking-widest">
                    类型
                  </span>
                  <span
                    v-if="filters.cardType"
                    class="truncate text-[10px] font-medium normal-case tracking-normal selected-count"
                  >
                    {{ getCardTypeLabel(filters.cardType) }}
                  </span>
                </div>
              </template>
              <div class="filter-chip-list">
                <button
                  type="button"
                  class="filter-chip"
                  :class="{ selected: !filters.cardType }"
                  :aria-pressed="!filters.cardType"
                  @click="filters.cardType = ''"
                >
                  <span>全部</span><small>{{ totalCards }}</small>
                </button>
                <button
                  v-for="t in sidebar.cardTypes"
                  :key="t.name"
                  type="button"
                  class="filter-chip"
                  :class="{ selected: filters.cardType === t.name }"
                  :aria-pressed="filters.cardType === t.name"
                  @click="selectCardType(t.name)"
                >
                  <span>{{ getCardTypeLabel(t.name) }}</span><small>{{ t.count }}</small>
                </button>
              </div>
            </n-collapse-item>

            <n-collapse-item name="tags">
              <template #header>
                <div class="flex items-center gap-1.5 min-w-0 flex-1 pr-1">
                  <span class="i-lucide-tags w-3.5 h-3.5 shrink-0 opacity-70" />
                  <span class="text-[11px] font-semibold text-slate-500 dark:text-neutral-400 uppercase tracking-widest">
                    标签
                  </span>
                  <span
                    v-if="filters.selectedTags.length"
                    class="text-[10px] font-medium normal-case tracking-normal tabular-nums selected-count"
                  >
                    {{ filters.selectedTags.length }} 项
                  </span>
                </div>
              </template>
              <div v-if="sidebar.tagsLoading" class="flex items-center justify-center py-4">
                <n-spin size="small" />
              </div>
              <div v-else-if="sidebar.tags.length" class="filter-chip-list">
                <button
                  v-for="tag in sidebar.tags"
                  :key="tag.name"
                  type="button"
                  class="filter-chip"
                  :class="{ selected: filters.selectedTags.includes(tag.name) }"
                  :aria-pressed="filters.selectedTags.includes(tag.name)"
                  @click="toggleTag(tag.name)"
                >
                  <span>{{ tag.name }}</span><small>{{ tag.count }}</small>
                </button>
              </div>
              <n-empty v-else size="small" description="暂无标签" class="py-2" />
            </n-collapse-item>

            <n-collapse-item name="techStack">
              <template #header>
                <div class="flex items-center gap-1.5 min-w-0 flex-1 pr-1">
                  <span class="i-lucide-layers w-3.5 h-3.5 shrink-0 opacity-70" />
                  <span class="text-[11px] font-semibold text-slate-500 dark:text-neutral-400 uppercase tracking-widest">
                    技术栈
                  </span>
                  <span
                    v-if="filters.selectedTechStacks.length"
                    class="text-[10px] font-medium normal-case tracking-normal tabular-nums selected-count"
                  >
                    {{ filters.selectedTechStacks.length }} 项
                  </span>
                </div>
              </template>
              <div v-if="sidebar.tagsLoading" class="flex items-center justify-center py-3">
                <n-spin size="small" />
              </div>
              <div v-else-if="sidebar.techStacks.length" class="filter-chip-list">
                <button
                  v-for="row in sidebar.techStacks"
                  :key="row.name"
                  type="button"
                  class="filter-chip"
                  :class="{ selected: filters.selectedTechStacks.includes(row.name) }"
                  :aria-pressed="filters.selectedTechStacks.includes(row.name)"
                  @click="toggleTechStack(row.name)"
                >
                  <span>{{ row.name }}</span><small>{{ row.count }}</small>
                </button>
              </div>
              <p v-else class="filter-empty">提炼完成后，识别出的技术项会显示在这里。</p>
            </n-collapse-item>
          </n-collapse>
        </div>
      </template>

    </div>
  </aside>
</template>

<style scoped>
.glass-sidebar { width: clamp(232px, 18vw, 282px); border-color:var(--line); }
.all-active { color:var(--pine); background:color-mix(in srgb,var(--pine) 10%,transparent); }
.sidebar-tree :deep(.n-tree-node-content__text) {
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  /* 限定文本区域宽度，让截断生效 */
  min-width: 0;
}
.sidebar-tree :deep(.n-tree-node-content__prefix) {
  margin-right: 6px;
  flex-shrink: 0;
}
.sidebar-tree :deep(.n-tree-node-content__suffix) {
  margin-left: auto;
  flex-shrink: 0;
}
.sidebar-tree :deep(.n-tree-node-content) {
  overflow: hidden;
}
/* 品牌图标：圆角使外观更精致 */
.brand-icon {
  width: 16px;
  height: 16px;
  border-radius: 3px;
  display: inline-block;
  flex-shrink: 0;
}
.library-filter-panel { display:flex; flex-direction:column; gap:18px; padding:16px 16px 24px; }
.filter-summary { display:flex; align-items:center; justify-content:space-between; gap:10px; padding:7px 4px 7px 10px; border-left:2px solid var(--vermilion); color:var(--muted); font-size:11px; }
.filter-chip-list { display:flex; flex-wrap:wrap; gap:6px; }
.filter-chip { display:inline-flex; align-items:center; gap:6px; min-height:28px; padding:0 9px; border:1px solid var(--line) !important; border-radius:7px; background:transparent; color:var(--ink-soft); font-size:11px; cursor:pointer; transition:border-color .16s var(--ease-out), background-color .16s var(--ease-out), color .16s var(--ease-out); }
.filter-chip:hover { border-color:var(--line-strong) !important; background:color-mix(in srgb,var(--surface) 58%,transparent); color:var(--ink); }
.filter-chip.selected { border-color:color-mix(in srgb,var(--pine) 42%,var(--line)) !important; background:color-mix(in srgb,var(--pine) 10%,transparent); color:var(--pine); font-weight:600; }
.filter-chip small { color:var(--muted); font-size:10px; font-variant-numeric:tabular-nums; }
.selected-count { color:var(--pine); }
.filter-empty { margin:2px 0 0; color:var(--muted); font-size:11px; line-height:1.65; }

/* 知识库折叠：面板内容随侧栏整体滚动，禁止 Naive 内部再出现滚动条 */
.library-filters-collapse :deep(.n-collapse-item__content-wrapper) {
  overflow: visible !important;
}
.library-filters-collapse :deep(.n-collapse-item__content-inner) {
  overflow: visible !important;
  max-height: none !important;
  padding-top: 2px;
  padding-bottom: 12px;
}
.library-filters-collapse :deep(.n-collapse-item__header) {
  padding-top: 6px;
  padding-bottom: 6px;
}
.library-filters-collapse :deep(.n-collapse-item:not(:first-child)) {
  border-top: 1px solid var(--line);
}
</style>
