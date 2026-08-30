<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { getCardTypeLabel } from '@chattake/shared'
import { NTag, NButton, NCheckbox, NTooltip } from 'naive-ui'
import type { SessionSummary } from '../types'

const props = defineProps<{
  session: SessionSummary
  analyzing?: boolean
  selectable?: boolean
  selected?: boolean
}>()

const emit = defineEmits<{
  analyze: [id: string]
  'update:selected': [id: string, checked: boolean]
}>()

const router = useRouter()

// ── 价值 ───────────────────────────────────────────────────────────────────

const valueKey = computed(() => (props.session.value || 'none').toLowerCase())

/** 高/中价值色条颜色；低/无 不展示色条 */
const barColors: Record<string, string> = {
  high: '#10b981',
  medium: '#f59e0b',
}
const barColor = computed(() => barColors[valueKey.value] ?? null)

/** 是否低价值（low 或 none） */
const isLowValue = computed(() => valueKey.value === 'low' || valueKey.value === 'none')

/** 价值标签样式映射 */
const valueBadgeClass: Record<string, string> = {
  high:   'bg-emerald-50 dark:bg-emerald-950/40 text-emerald-600 dark:text-emerald-400 border-emerald-100 dark:border-emerald-800/60',
  medium: 'bg-amber-50  dark:bg-amber-950/40  text-amber-700  dark:text-amber-400  border-amber-200  dark:border-amber-800/60',
  low:    'bg-slate-50  dark:bg-slate-900/60  text-slate-500  dark:text-slate-400  border-slate-200  dark:border-slate-700/60',
  none:   'bg-slate-50  dark:bg-slate-900/60  text-slate-400  dark:text-slate-500  border-slate-200  dark:border-slate-700/60',
}
const valueBadgeText: Record<string, string> = {
  high: '高价值', medium: '中价值', low: '⚠ 低价值', none: '⚠ 无价值',
}

// ── 已分析判断 ─────────────────────────────────────────────────────────────
const isAnalyzed = computed(
  () => !!props.session.cardId || props.session.status === 'analyzed',
)

// ── 卡片元信息 ─────────────────────────────────────────────────────────────

/** 标签数组，最多 3 个 */
const tagList = computed(() => {
  const raw = props.session.cardTags
  if (!raw) return []
  return raw.split(',').filter(Boolean).slice(0, 3)
})

// ── 交互 ──────────────────────────────────────────────────────────────────

const isSelectDisabled = computed(
  () => props.selectable && (isAnalyzed.value || props.session.status === 'analyzing'),
)

function openSession() {
  if (props.selectable) {
    if (isSelectDisabled.value) return
    emit('update:selected', props.session.id, !props.selected)
    return
  }
  void router.push({
    name: 'session-detail',
    params: { sessionId: props.session.id },
    query: props.session.cardId ? { cardId: props.session.cardId } : {},
  })
}

function onAction(e: MouseEvent) {
  e.stopPropagation()
  if (isAnalyzed.value) {
    void router.push({
      name: 'session-detail',
      params: { sessionId: props.session.id },
      query: props.session.cardId ? { cardId: props.session.cardId } : {},
    })
  } else {
    emit('analyze', props.session.id)
  }
}

const sourceIcon = computed(() => {
  if (props.session.sourceId === 'claude-code') return 'i-lucide-terminal'
  if (props.session.sourceId === 'cursor') return 'i-lucide-mouse-pointer-click'
  return 'i-lucide-message-square'
})

const actionLoading = computed(
  () => props.analyzing || props.session.status === 'analyzing',
)

// ── 列表主标题 ──────────────────────────────────────────────

const listPrimaryTitle = computed(() => {
  const preview = props.session.firstUserPreview?.trim()
  const title = props.session.cardTitle?.trim()
  if (!isAnalyzed.value && title) return title
  if (preview) return preview
  if (title) return title
  return (
    props.session.projectName
    || props.session.projectPath
    || props.session.sessionId.slice(0, 14) + (props.session.sessionId.length > 14 ? '…' : '')
  )
})

const timeTooltip = computed(() => {
  const u = props.session.updatedAt
  if (!u) return '会话时间：—'
  const readable = u.replace('T', ' ').slice(0, 19)
  return `会话时间：${readable}（updatedAt）`
})

/** 相对时间简写 */
function relativeTime(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  const now = Date.now()
  const diff = now - d.getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 60) return `${mins} 分钟前`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours} 小时前`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days} 天前`
  return iso.replace('T', ' ').slice(0, 10)
}

// ── 大小格式化 ─────────────────────────────────────────────────────────────

function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return '—'
  if (bytes < 1024) return '<1 KB'
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

/**
 * 卡片样式
 */
const cardClass = computed(() => {
  // 批量选中：须用 ring-inset。根节点有 overflow-hidden 时，默认 ring 画在盒外会被裁掉，看起来像「描边/底色拉不全」
  if (props.selectable && props.selected) {
    return 'ring-2 ring-inset ring-emerald-500 border-transparent bg-emerald-50/30 dark:bg-emerald-950/30'
  }
  if (isLowValue.value && isAnalyzed.value) {
    return 'opacity-85 bg-slate-50/80 dark:bg-neutral-900/60 border-slate-200/80 dark:border-neutral-700/60 hover:opacity-100 hover:border-slate-300 dark:hover:border-neutral-600'
  }
  return 'bg-white dark:bg-neutral-900 border-slate-200/80 dark:border-neutral-700/60 hover:border-emerald-300 dark:hover:border-emerald-700'
})
</script>

<template>
  <div
    class="group relative border transition-all duration-300 cursor-pointer overflow-hidden flex"
    :class="[
      isAnalyzed ? 'rounded-2xl' : 'rounded-xl',
      cardClass,
      isSelectDisabled ? 'cursor-default opacity-60' : '',
    ]"
    @click="openSession"
  >
    <!-- 左侧高/中价值色条（仅已分析且高/中价值） -->
    <div
      v-if="barColor"
      class="absolute left-0 top-2.5 bottom-2.5 w-[3px] rounded-r-full z-10 pointer-events-none"
      :style="{ backgroundColor: barColor }"
    />

    <!-- 未分析：更小内边距与纵向间距；已分析：略收紧 -->
    <div
      class="flex-1 min-w-0"
      :class="[
        isAnalyzed ? 'p-4' : 'p-3',
        barColor ? 'pl-[18px]' : '',
      ]"
    >
      <!-- 头部：标签 + 操作按钮 -->
      <div class="flex items-start justify-between" :class="isAnalyzed ? 'gap-3' : 'gap-2'">
        <div class="flex-1 min-w-0" :class="isAnalyzed ? 'space-y-2' : 'space-y-1'">
          <!-- 状态标签行 -->
          <div class="flex items-center gap-2 flex-wrap">
            <!-- 批量模式 checkbox -->
            <div v-if="selectable" class="shrink-0" @click.stop>
              <n-checkbox
                :checked="selected"
                :disabled="isSelectDisabled"
                @update:checked="!isSelectDisabled && emit('update:selected', session.id, $event)"
              />
            </div>

            <template v-if="isAnalyzed">
              <!-- 价值标签 -->
              <n-tag
                v-if="session.value && (valueKey === 'high' || valueKey === 'medium')"
                size="small"
                :bordered="false"
                class="font-medium rounded"
                :class="valueBadgeClass[valueKey]"
              >
                {{ valueBadgeText[valueKey] }}
              </n-tag>
              <!-- 类型标签 -->
              <n-tag
                v-if="session.cardType"
                size="small"
                type="info"
                :bordered="false"
                class="rounded"
              >
                {{ getCardTypeLabel(session.cardType) }}
              </n-tag>
              <!-- 低/无价值标签 -->
              <span
                v-if="isLowValue && session.value"
                class="inline-flex items-center text-[10px] font-medium px-1.5 py-px rounded border italic"
                :class="valueBadgeClass[valueKey] ?? valueBadgeClass.none"
              >
                {{ valueBadgeText[valueKey] ?? session.value }}
              </span>
            </template>

            <template v-else>
              <!-- 未分析：仅状态 + 来源两行标签，无笔记类型/技术标签区 -->
              <n-tag
                size="small"
                :bordered="false"
                :type="session.status === 'analyzing' ? 'info'
                     : session.status === 'error' ? 'error'
                     : 'warning'"
                class="rounded !text-[11px]"
              >
                {{ session.status === 'pending'   ? '待分析'
                 : session.status === 'analyzing' ? '分析中'
                 : session.status === 'error'     ? '失败'
                 : session.status }}
              </n-tag>
              <n-tag size="small" :bordered="false" class="rounded !text-[11px]">
                {{ session.sourceId }}
              </n-tag>
            </template>
          </div>

          <!-- 标题（略小字号 + 紧行高，与摘要间距收窄以提升列表密度） -->
          <div>
            <h3
              class="font-semibold leading-snug pr-4"
              :class="isLowValue && isAnalyzed
                ? 'text-slate-500 dark:text-slate-400 italic'
                : 'text-slate-800 dark:text-slate-100'"
            >
              {{ listPrimaryTitle }}
            </h3>
            <!-- 摘要 -->
            <p
              v-if="isAnalyzed && session.cardSummary"
              class="text-[14px] leading-snug line-clamp-1 mt-0.5"
              :class="isLowValue
                ? 'text-slate-400 dark:text-slate-500 italic'
                : 'text-slate-500 dark:text-slate-400'"
            >
              {{ session.cardSummary }}
            </p>
            <p
              v-else-if="!isAnalyzed"
              class="text-[14px] leading-snug text-slate-500 dark:text-slate-400 line-clamp-1 mt-0"
            >
              {{ session.status === 'error'
                ? (session.errorMessage || '分析失败，点击查看详情或重试。')
                : '点击预览或提炼为笔记' }}
            </p>
          </div>

          <!-- 技术标签（来自卡片提炼，仅已分析展示） -->
          <div v-if="isAnalyzed && tagList.length" class="flex flex-wrap gap-1.5 pt-0.5">
            <span
              v-for="tag in tagList"
              :key="tag"
              class="text-[10px] px-2 py-0.5 rounded-full bg-slate-100 dark:bg-neutral-800 text-slate-500 dark:text-slate-400 border border-slate-200 dark:border-neutral-700"
            >{{ tag }}</span>
          </div>
        </div>

        <!-- 悬浮操作按钮 -->
        <div
          v-if="!selectable && !(isAnalyzed && valueKey === 'none')"
          class="opacity-0 group-hover:opacity-100 transition-opacity duration-200 shrink-0"
          @click.stop
        >
          <n-button
            secondary
            size="small"
            class="rounded-lg"
            :loading="actionLoading"
            :disabled="actionLoading"
            @click="onAction"
          >
            <template #icon>
              <span
                :class="session.cardId
                  ? 'i-lucide-book-open'
                  : isAnalyzed
                    ? 'i-lucide-messages-square'
                    : 'i-lucide-sparkles'"
                class="w-4 h-4"
              />
            </template>
            {{
              actionLoading ? '分析中…'
              : session.cardId ? '查看笔记'
              : isAnalyzed   ? '查看对话'
              : '分析'
            }}
          </n-button>
        </div>
      </div>

      <!-- 底部元数据：容器 m/p 为 0，仅靠 border-t 与主区隔开；项间距用 gap -->
      <div
        class="m-0 p-0 flex flex-wrap items-center border-t border-slate-100 dark:border-neutral-800 text-slate-500 dark:text-slate-400"
        :class="
          isAnalyzed
            ? 'gap-x-4 gap-y-1.5 text-[12px]'
            : 'gap-x-2.5 gap-y-1 text-[11px]'
        "
      >
        <span
          class="flex items-center gap-1 font-medium text-slate-700 dark:text-slate-300 bg-slate-100/50 dark:bg-neutral-800/50 rounded-md max-w-[min(100%,14rem)] min-w-0"
          :class="isAnalyzed ? 'px-2 py-1' : 'px-1.5 py-0.5'"
        >
          <span class="i-lucide-folder shrink-0 text-slate-400 dark:text-slate-500" :class="isAnalyzed ? 'w-3.5 h-3.5' : 'w-3 h-3'" />
          <span class="truncate">{{ session.projectName || session.projectPath || '未命名项目' }}</span>
        </span>
        <n-tooltip trigger="hover" :delay="300">
          <template #trigger>
            <span class="flex items-center gap-1 cursor-default shrink-0">
              <span class="i-lucide-clock text-slate-400 dark:text-slate-500" :class="isAnalyzed ? 'w-3.5 h-3.5' : 'w-3 h-3'" />
              {{ relativeTime(session.updatedAt) }}
            </span>
          </template>
          {{ timeTooltip }}
        </n-tooltip>
        <span class="flex items-center gap-1 shrink-0">
          <span class="i-lucide-message-circle text-slate-400 dark:text-slate-500" :class="isAnalyzed ? 'w-3.5 h-3.5' : 'w-3 h-3'" />
          {{ session.messageCount }} 条
        </span>
        <span v-if="session.rawSizeBytes" class="flex items-center gap-1 shrink-0">
          <span class="i-lucide-hard-drive text-slate-400 dark:text-slate-500" :class="isAnalyzed ? 'w-3.5 h-3.5' : 'w-3 h-3'" />
          {{ formatSize(session.rawSizeBytes) }}
        </span>
        <span
          v-if="isAnalyzed"
          class="flex items-center gap-1 text-slate-400 dark:text-slate-500 font-mono bg-slate-50 dark:bg-neutral-800 px-1.5 py-0.5 rounded border border-slate-100 dark:border-neutral-700"
        >
          <span :class="sourceIcon" class="w-3 h-3" />
          {{ session.sourceId }}
        </span>
      </div>
    </div>
  </div>
</template>
