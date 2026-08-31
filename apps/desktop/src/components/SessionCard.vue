<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { getCardTypeLabel } from '@chattake/shared'
import { NButton, NCheckbox, NTooltip } from 'naive-ui'
import type { SessionSummary } from '../types'

const props = defineProps<{ session: SessionSummary; analyzing?: boolean; selectable?: boolean; selected?: boolean }>()
const emit = defineEmits<{ analyze: [id: string]; 'update:selected': [id: string, checked: boolean] }>()
const router = useRouter()
const analyzed = computed(() => Boolean(props.session.cardId) || props.session.status === 'analyzed')
const disabled = computed(() => Boolean(props.selectable && (analyzed.value || props.session.status === 'analyzing')))
const loading = computed(() => props.analyzing || props.session.status === 'analyzing')
const title = computed(() => {
  const preferred = analyzed.value ? props.session.cardTitle?.trim() : props.session.firstUserPreview?.trim()
  return preferred || props.session.cardTitle?.trim() || props.session.projectName || props.session.projectPath || '未命名会话'
})
const description = computed(() => analyzed.value
  ? props.session.cardSummary?.trim() || '已完成知识分析，点击查看结果。'
  : props.session.status === 'error'
    ? props.session.errorMessage || '分析失败，可进入会话查看并重试。'
    : '尚未分析 · 可先预览对话，再决定是否提取知识。')
const topics = computed(() => props.session.cardTags?.split(',').map((tag) => tag.trim()).filter(Boolean).slice(0, 3) ?? [])
const status = computed(() => {
  if (loading.value) return { label: '分析中', tone: 'active' }
  if (props.session.status === 'error') return { label: '失败', tone: 'danger' }
  if (!analyzed.value) return { label: '待分析', tone: 'pending' }
  if (props.session.value === 'high') return { label: '高价值', tone: 'active' }
  if (props.session.value === 'medium') return { label: '待确认', tone: 'pending' }
  return { label: '已判断', tone: 'quiet' }
})
const sourceLabel = computed(() => ({
  'claude-code': 'Claude Code', codex: 'Codex', cursor: 'Cursor', grok: 'Grok', omp: 'Oh My Pi', pi: 'Pi', codebuddy: 'CodeBuddy',
}[props.session.sourceId] ?? props.session.sourceId))

function relativeTime(value: string | null | undefined) {
  if (!value) return '—'
  const minutes = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 60_000))
  if (minutes < 60) return `${minutes} 分钟前`
  if (minutes < 1440) return `${Math.floor(minutes / 60)} 小时前`
  if (minutes < 43_200) return `${Math.floor(minutes / 1440)} 天前`
  return value.slice(0, 10)
}
function openSession() {
  if (props.selectable) {
    if (!disabled.value) emit('update:selected', props.session.id, !props.selected)
    return
  }
  void router.push({ name: 'session-detail', params: { sessionId: props.session.id }, query: props.session.cardId ? { cardId: props.session.cardId } : {} })
}
function onAction(event: MouseEvent) {
  event.stopPropagation()
  if (analyzed.value) openSession()
  else emit('analyze', props.session.id)
}
</script>

<template>
  <article class="session-row" :class="{ selected, disabled, analyzed }">
    <button
      type="button"
      class="row-hit"
      :disabled="disabled"
      :aria-label="selectable ? `${selected ? '取消选择' : '选择'} ${title}` : `打开 ${sourceLabel} 会话：${title}`"
      @click="openSession"
    />
    <span class="status-rail" :data-tone="status.tone" />
    <div v-if="selectable" class="select-cell" @click.stop>
      <n-checkbox :checked="selected" :disabled="disabled" :aria-label="`选择 ${title}`" @update:checked="!disabled && emit('update:selected', session.id, $event)" />
    </div>
    <div class="session-copy">
      <div class="eyebrow-row">
        <span class="status-label" :data-tone="status.tone">{{ status.label }}</span>
        <span class="source-label">{{ sourceLabel }}</span>
        <span v-if="session.cardType" class="type-label">{{ getCardTypeLabel(session.cardType) }}</span>
      </div>
      <h3>{{ title }}</h3>
      <p class="description">{{ description }}</p>
      <div v-if="topics.length" class="topic-row"><span v-for="topic in topics" :key="topic"># {{ topic }}</span></div>
    </div>
    <div class="session-meta">
      <span class="project" :title="session.projectPath || ''">{{ session.projectName || '未关联项目' }}</span>
      <n-tooltip trigger="hover">
        <template #trigger><span>{{ relativeTime(session.updatedAt) }}</span></template>
        {{ session.updatedAt?.replace('T', ' ').slice(0, 19) || '—' }}
      </n-tooltip>
      <span>{{ session.messageCount }} 条消息</span>
    </div>
    <n-button v-if="!selectable" class="row-action" quaternary size="small" :loading="loading" :disabled="loading" @click="onAction">
      {{ analyzed ? '查看' : '分析' }}
      <template #icon><span :class="analyzed ? 'i-lucide-arrow-up-right' : 'i-lucide-sparkles'" /></template>
    </n-button>
  </article>
</template>

<style scoped>
.session-row { position:relative; display:grid; grid-template-columns:minmax(0,1fr) 150px 74px; gap:20px; align-items:center; min-height:132px; padding:18px 18px 18px 22px; border-top:1px solid var(--line); background:transparent; cursor:pointer; transition:background-color .14s ease; }
.session-row:last-child { border-bottom:1px solid var(--line); }
.session-row:hover { background:color-mix(in srgb,var(--surface) 72%,transparent); }
.row-hit { position:absolute; inset:0; z-index:0; appearance:none; border:0; background:transparent; cursor:pointer; }
.row-hit:focus-visible { outline:2px solid var(--pine); outline-offset:-2px; }
.session-row.selected { background:color-mix(in srgb,var(--pine) 9%,transparent); }
.session-row.disabled { cursor:default; opacity:.58; }
.status-rail { position:absolute; left:0; top:22px; bottom:22px; width:2px; background:var(--line-strong); pointer-events:none; }
.status-rail[data-tone='active'] { background:var(--pine); }
.status-rail[data-tone='pending'] { background:var(--vermilion); }
.status-rail[data-tone='danger'] { background:var(--vermilion); }
.select-cell { position:absolute; z-index:2; left:12px; top:15px; }
.session-row:has(.select-cell) { padding-left:46px; }
.session-copy { position:relative; z-index:1; min-width:0; pointer-events:none; }
.eyebrow-row { display:flex; align-items:center; gap:9px; margin-bottom:7px; min-width:0; font-size:10px; letter-spacing:.08em; text-transform:uppercase; }
.status-label,.source-label,.type-label { color:var(--muted); }
.status-label[data-tone='active'] { color:var(--pine); }
.status-label[data-tone='pending'] { color:var(--vermilion); }
.status-label[data-tone='danger'] { color:var(--vermilion); }
.source-label::before,.type-label::before { content:'·'; margin-right:9px; color:var(--line-strong); }
h3 { margin:0; overflow:hidden; color:var(--ink); font-family:var(--font-editorial); font-size:17px; font-weight:600; line-height:1.45; text-overflow:ellipsis; white-space:nowrap; }
.description { display:-webkit-box; max-width:860px; margin:5px 0 0; overflow:hidden; color:var(--muted); font-size:12px; line-height:1.6; -webkit-box-orient:vertical; -webkit-line-clamp:2; }
.topic-row { display:flex; flex-wrap:wrap; gap:10px; margin-top:8px; color:var(--pine); font-size:10px; }
.session-meta { position:relative; z-index:1; display:flex; flex-direction:column; gap:5px; min-width:0; color:var(--muted); font-size:11px; line-height:1.35; pointer-events:none; }
.project { overflow:hidden; color:var(--ink-soft); text-overflow:ellipsis; white-space:nowrap; }
.row-action { z-index:2; opacity:.2; transition:opacity .14s ease; }
.session-row:hover .row-action,.row-action:focus-visible { opacity:1; }
@media (max-width:900px) { .session-row { grid-template-columns:minmax(0,1fr) 70px; } .session-meta { display:none; } }
</style>
