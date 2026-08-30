<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { storeToRefs } from 'pinia'
import { NButton, NInput, NModal, NResult, NSelect, NSpin, NTag, NTooltip, useDialog, useMessage } from 'naive-ui'
import NoteCard from '../components/NoteCard.vue'
import ChatReplay from '../components/ChatReplay.vue'
import { useAnalysisQueueStore } from '../stores/analysisQueue'
import { api } from '../lib/tauri'
import { exportOneCardToFile } from '../lib/cardExport'
import { appendDistillHint } from '../lib/distillHints'
import type { Card, Message, Session } from '../types'

const props = defineProps<{
  sessionId: string
}>()

const route = useRoute()
const router = useRouter()
const message = useMessage()
const dialog = useDialog()

const queue = useAnalysisQueueStore()
const { currentTask, tasks } = storeToRefs(queue)

const session = ref<Session | null>(null)
const messages = ref<Message[]>([])
const card = ref<Card | null>(null)
const loading = ref(true)
const analyzeError = ref<string | null>(null)
const loadError = ref<string | null>(null)
const exportMdLoading = ref(false)
const mode = ref<'note' | 'chat'>('note')
const editOpen = ref(false)
const editSaving = ref(false)
const edit = ref({ title: '', cardType: 'explanation', summary: '', note: '', tags: '', technologies: '' })
const typeOptions = [
  { label: '决策', value: 'decision' }, { label: '排障', value: 'troubleshooting' },
  { label: '实现', value: 'implementation' }, { label: '解释', value: 'explanation' },
  { label: '片段', value: 'snippet' },
]

/** 当前会话是否正在执行 distill（队列头） */
const analyzing = computed(
  () => currentTask.value?.sessionId === props.sessionId,
)

/** 已在队列中但尚未轮到 */
const analyzeQueued = computed(() =>
  tasks.value.some((t) => t.sessionId === props.sessionId && t.status === 'pending'),
)

/** 耗时超过 150s 提示「分析较慢」 */
const analyzeSlow = computed(
  () => analyzing.value && (currentTask.value?.elapsedSec ?? 0) > 150,
)

// ────────────── 数据加载 ──────────────

async function load() {
  loading.value = true
  loadError.value = null
  try {
    const [sess, msgs] = await Promise.all([
      api.getSession(props.sessionId),
      api.getSessionMessages(props.sessionId),
    ])
    session.value = sess
    messages.value = msgs

    const cardId = typeof route.query.cardId === 'string' ? route.query.cardId : null
    if (cardId) {
      card.value = await api.getCard(cardId)
      mode.value = 'note'
    } else {
      card.value = null
    }
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(() => [props.sessionId, route.query.cardId], () => {
  void load()
})

// ────────────── 操作 ──────────────

function close() {
  void router.push({ name: 'sessions' })
}

async function onExportMarkdown() {
  if (!card.value) return
  exportMdLoading.value = true
  try {
    const ok = await exportOneCardToFile(card.value.id, card.value.title)
    if (ok) message.success('已导出 Markdown')
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  } finally {
    exportMdLoading.value = false
  }
}

async function analyze() {
  analyzeError.value = null
  if (session.value) session.value.status = 'analyzing'
  try {
    await queue.startAnalysis([props.sessionId])
    analyzeError.value = '分析任务已创建；完成后可从对话列表打开生成的知识项。'
  } catch (error) {
    analyzeError.value = appendDistillHint(error instanceof Error ? error.message : String(error))
    if (session.value) session.value.status = 'error'
  }
}

function openEdit() {
  if (!card.value) return
  edit.value = {
    title: card.value.title,
    cardType: card.value.type ?? 'explanation',
    summary: card.value.summary ?? '',
    note: card.value.note,
    tags: card.value.tags.join('，'),
    technologies: card.value.techStack.join('，'),
  }
  editOpen.value = true
}

function splitLabels(value: string) {
  return value.split(/[,，]/).map((item) => item.trim()).filter(Boolean)
}

async function saveEdit() {
  if (!card.value) return
  editSaving.value = true
  try {
    card.value = await api.updateCard(card.value.id, {
      title: edit.value.title,
      cardType: edit.value.cardType,
      summary: edit.value.summary,
      note: edit.value.note,
      tags: splitLabels(edit.value.tags),
      technologies: splitLabels(edit.value.technologies),
    })
    editOpen.value = false
    message.success('草稿已保存')
  } catch (error) {
    message.error(error instanceof Error ? error.message : String(error))
  } finally { editSaving.value = false }
}

function publish(replaceExisting: boolean) {
  if (!card.value) return
  const label = replaceExisting ? '替换同会话中未人工编辑的旧知识' : '与现有知识并存'
  dialog.warning({
    title: '发布草稿', content: `将发布此草稿，并${label}。人工编辑过的旧知识不会被自动删除。`,
    positiveText: '确认发布', negativeText: '取消',
    onPositiveClick: async () => {
      if (!card.value) return
      card.value = await api.publishCard(card.value.id, replaceExisting)
      message.success('知识已发布')
    },
  })
}

// ────────────── 会话元数据展示 ──────────────

const sourceIcon = computed(() => {
  if (!session.value) return 'i-lucide-message-square'
  if (session.value.sourceId === 'claude-code') return 'i-lucide-bot'
  if (session.value.sourceId === 'cursor') return 'i-lucide-terminal-square'
  return 'i-lucide-message-square'
})

const statusTagType = computed<'default' | 'info' | 'success' | 'error'>(() => {
  switch (session.value?.status) {
    case 'analyzed':
      return 'success'
    case 'analyzing':
      return 'info'
    case 'error':
      return 'error'
    default:
      return 'default'
  }
})

const statusLabel = computed(() => {
  switch (session.value?.status) {
    case 'pending':
      return '待分析'
    case 'analyzing':
      return '分析中'
    case 'analyzed':
      return '已分析'
    case 'error':
      return '失败'
    default:
      return session.value?.status ?? ''
  }
})

const valueColors: Record<string, string> = {
  high: '#10b981',
  medium: '#f59e0b',
  low: '#94a3b8',
}
</script>

<template>
  <div class="flex flex-col h-full">
    <div class="shrink-0 border-b border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-950 px-5 py-3">
      <div class="max-w-4xl mx-auto flex items-start gap-4">
        <n-button text size="small" class="mt-0.5 shrink-0" @click="close">
          <span class="inline-flex items-center gap-1 text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200">
            <span class="i-lucide-arrow-left w-4 h-4" />
            返回
          </span>
        </n-button>

        <div class="flex-1 min-w-0">
          <div v-if="session" class="flex items-center gap-2 flex-wrap">
            <span :class="sourceIcon" class="w-4 h-4 text-neutral-500 shrink-0" />
            <span class="font-semibold text-neutral-900 dark:text-neutral-100 truncate">
              {{ session.analysisTitle || session.projectName || session.projectPath || '未命名项目' }}
            </span>
            <n-tag size="tiny" :bordered="false" :type="statusTagType">{{ statusLabel }}</n-tag>
            <n-tag
              v-if="session.value"
              size="tiny"
              :bordered="false"
              :color="{ color: valueColors[session.value] ?? '#94a3b8', textColor: '#fff' }"
              class="uppercase font-bold"
            >
              {{ session.value }}
            </n-tag>
          </div>
          <div v-if="session" class="flex items-center gap-3 mt-1 text-[11px] text-neutral-400 flex-wrap">
            <span class="flex items-center gap-1">
              <span class="i-lucide-calendar w-3 h-3" />
              {{ session.updatedAt?.replace('T', ' ').slice(0, 16) }}
            </span>
            <span class="flex items-center gap-1">
              <span class="i-lucide-message-circle w-3 h-3" />
              {{ session.messageCount }} 条消息
            </span>
          </div>
          <!-- 会话路径与数据源侧会话 ID（v0.1.1 信息可见性） -->
          <div
            v-if="session"
            class="mt-2 text-[11px] text-neutral-500 dark:text-neutral-400 space-y-1.5 max-w-3xl"
          >
            <div class="flex items-start gap-1.5 min-w-0">
              <span class="i-lucide-folder-open w-3.5 h-3.5 shrink-0 mt-0.5 opacity-80" />
              <n-tooltip trigger="hover" placement="top-start" :delay="200">
                <template #trigger>
                  <span class="break-all line-clamp-2 cursor-default">
                    {{ session.rawPath || session.projectPath || '—' }}
                  </span>
                </template>
                <span class="text-xs whitespace-pre-wrap break-all">{{ session.rawPath || session.projectPath || '—' }}</span>
              </n-tooltip>
            </div>
            <div class="flex items-start gap-1.5 min-w-0 font-mono text-[10px]">
              <span class="i-lucide-fingerprint w-3.5 h-3.5 shrink-0 mt-0.5 opacity-80" />
              <div class="min-w-0">
                <span class="text-neutral-400 mr-1">会话 ID</span>
                <span class="text-neutral-600 dark:text-neutral-300 break-all select-all">{{ session.sessionId }}</span>
              </div>
            </div>
          </div>
        </div>

        <div v-if="session && !card && !loading" class="shrink-0">
          <n-button
            type="primary"
            size="small"
            :loading="analyzing || analyzeQueued"
            :disabled="analyzing || analyzeQueued"
            @click="analyze"
          >
            <span class="inline-flex items-center gap-1.5">
              <span v-if="!analyzing && !analyzeQueued" class="i-lucide-sparkles w-3.5 h-3.5" />
              {{ analyzing ? '分析中…' : analyzeQueued ? '排队中…' : '提炼笔记' }}
            </span>
          </n-button>
        </div>
      </div>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto">
      <div class="max-w-4xl mx-auto px-5 py-5">
        <div v-if="loading" class="flex items-center justify-center py-20">
          <n-spin size="medium" />
        </div>

        <n-result v-else-if="loadError" status="error" :title="loadError" class="py-12">
          <template #footer>
            <n-button @click="close">返回列表</n-button>
            <n-button class="ml-2" @click="load">重试</n-button>
          </template>
        </n-result>

        <template v-else-if="card">
          <div class="mb-3 flex items-center justify-between gap-3 rounded-xl border border-neutral-200 dark:border-neutral-800 px-3 py-2">
            <div class="flex items-center gap-2 text-sm">
              <n-tag size="small" :type="card.publicationStatus === 'draft' ? 'warning' : 'success'">
                {{ card.publicationStatus === 'draft' ? '待审草稿' : '已发布' }}
              </n-tag>
              <span v-if="card.isUserEdited" class="text-xs text-neutral-500">已人工编辑</span>
            </div>
            <div class="flex gap-2">
              <n-button size="small" secondary @click="openEdit">编辑</n-button>
              <template v-if="card.publicationStatus === 'draft'">
                <n-button size="small" secondary @click="publish(false)">发布并存</n-button>
                <n-button size="small" type="primary" @click="publish(true)">发布并替换</n-button>
              </template>
            </div>
          </div>
          <div
            v-if="analyzeQueued && !analyzing"
            class="mb-4 flex items-center gap-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-neutral-50 dark:bg-neutral-900/80 px-4 py-3"
          >
            <span class="i-lucide-clock w-4 h-4 text-neutral-500 shrink-0" />
            <p class="text-sm text-neutral-600 dark:text-neutral-300">
              已加入分析队列，等待前面的任务完成…（见右下角面板）
            </p>
          </div>

          <div
            v-if="analyzing"
            class="mb-4 flex items-center gap-3 rounded-lg border border-brand-200 dark:border-brand-800 bg-brand-50 dark:bg-brand-950/30 px-4 py-3"
          >
            <n-spin size="small" />
            <div>
              <p class="text-sm font-medium text-brand-700 dark:text-brand-300">正在重新提炼笔记…</p>
              <p class="text-xs text-brand-600/70 dark:text-brand-400/70 mt-0.5">
                {{ analyzeSlow ? '分析耗时较长，请耐心等待…' : 'LLM 重新分析对话内容，完成后自动更新笔记' }}
              </p>
            </div>
          </div>

          <div
            v-if="analyzeError"
            class="mb-4 flex items-center gap-2 rounded-lg border border-red-200 dark:border-red-900/50 bg-red-50 dark:bg-red-950/30 px-3 py-2 text-sm text-red-600 dark:text-red-400"
          >
            <span class="i-lucide-alert-circle w-4 h-4 shrink-0" />
            <span class="flex-1">{{ analyzeError }}</span>
            <button class="opacity-60 hover:opacity-100" type="button" @click="analyzeError = null">✕</button>
          </div>

          <NoteCard
            :card="card"
            :mode="mode"
            :analyzing="analyzing"
            :export-loading="exportMdLoading"
            @update:mode="mode = $event"
            @close="close"
            @reanalyze="analyze"
            @export-markdown="onExportMarkdown"
          >
            <template #chat>
              <div class="max-h-[520px] overflow-y-auto pr-1">
                <ChatReplay :messages="messages" />
              </div>
            </template>
          </NoteCard>

          <n-modal v-model:show="editOpen" preset="card" title="编辑知识项" class="max-w-3xl" :mask-closable="false">
            <div class="space-y-3">
              <n-input v-model:value="edit.title" placeholder="标题" />
              <n-select v-model:value="edit.cardType" :options="typeOptions" />
              <n-input v-model:value="edit.summary" type="textarea" :rows="2" placeholder="摘要" />
              <n-input v-model:value="edit.note" type="textarea" :rows="14" placeholder="Markdown 正文" />
              <n-input v-model:value="edit.tags" placeholder="主题标签，逗号分隔，最多 3 个" />
              <n-input v-model:value="edit.technologies" placeholder="技术项，逗号分隔，最多 5 个" />
              <div class="flex justify-end gap-2">
                <n-button @click="editOpen = false">取消</n-button>
                <n-button type="primary" :loading="editSaving" @click="saveEdit">保存</n-button>
              </div>
            </div>
          </n-modal>
        </template>

        <template v-else-if="session">
          <div
            v-if="analyzeQueued && !analyzing"
            class="mb-4 flex items-center gap-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-neutral-50 dark:bg-neutral-900/80 px-4 py-3"
          >
            <span class="i-lucide-clock w-4 h-4 text-neutral-500 shrink-0" />
            <p class="text-sm text-neutral-600 dark:text-neutral-300">
              已加入分析队列，等待前面的任务完成…（见右下角面板）
            </p>
          </div>

          <div
            v-if="analyzeError"
            class="mb-4 flex items-center gap-2 rounded-lg border border-red-200 dark:border-red-900/50 bg-red-50 dark:bg-red-950/30 px-3 py-2 text-sm text-red-600 dark:text-red-400"
          >
            <span class="i-lucide-alert-circle w-4 h-4 shrink-0" />
            <span class="flex-1">{{ analyzeError }}</span>
            <button class="opacity-60 hover:opacity-100" type="button" @click="analyzeError = null">✕</button>
          </div>

          <div
            v-if="analyzing"
            class="mb-4 flex items-center gap-3 rounded-lg border border-brand-200 dark:border-brand-800 bg-brand-50 dark:bg-brand-950/30 px-4 py-3"
          >
            <n-spin size="small" />
            <div>
              <p class="text-sm font-medium text-brand-700 dark:text-brand-300">正在提炼笔记…</p>
              <p class="text-xs text-brand-600/70 dark:text-brand-400/70 mt-0.5">
                {{ analyzeSlow ? '分析耗时较长，请耐心等待…' : 'LLM 分析对话内容，通常需要 15–60 秒' }}
              </p>
            </div>
          </div>

          <div class="rounded-xl border border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-900">
            <div v-if="!messages.length" class="px-6 py-12 text-center">
              <span class="i-lucide-message-square-off w-8 h-8 text-neutral-300 dark:text-neutral-600 mx-auto block mb-2" />
              <p class="text-sm text-neutral-400">该会话暂无消息记录</p>
            </div>
            <div v-else class="p-4">
              <div class="flex items-center justify-between text-[11px] text-neutral-400 mb-3 pb-3 border-b border-neutral-100 dark:border-neutral-800">
                <span>对话记录（{{ messages.length }} 条消息）</span>
                <n-tooltip trigger="hover" :delay="400">
                  <template #trigger>
                    <span class="i-lucide-info w-3.5 h-3.5 cursor-default" />
                  </template>
                  点击「提炼笔记」按钮，AI 将从此对话中提取编程知识
                </n-tooltip>
              </div>
              <ChatReplay :messages="messages" />
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>
