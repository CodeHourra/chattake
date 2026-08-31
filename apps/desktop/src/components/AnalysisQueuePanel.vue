<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { NButton, NDropdown, NEmpty, NModal, NProgress, NSpin, NTag } from 'naive-ui'
import { useAnalysisQueueStore } from '../stores/analysisQueue'
import { api } from '../lib/tauri'
import type { Job, JobItem } from '../types'

const queue = useAnalysisQueueStore()
const { jobs, clock } = storeToRefs(queue)
const show = defineModel<boolean>('show', { default: false })
const providerOptions = ref<Array<{ label: string; key: string }>>([])
const hasCompleted = computed(() => jobs.value.some((job) => isTerminal(job)))

const statusLabel: Record<string, string> = {
  queued: '排队中', running: '执行中', succeeded: '已完成', failed: '失败',
  cancelled: '已取消', interrupted: '已中断',
}

const phaseLabel: Record<string, string> = {
  queued: '等待执行', scanning: '扫描与写入会话', preparing: '准备对话内容', judging: '价值判断', extracting: '提取知识', saving: '保存知识结果',
  completed: '处理完成', failed: '处理失败', cancelled: '已取消', interrupted: '异常中断',
  succeeded: '处理完成', imported: '新增导入', updated: '内容更新', skipped: '无变化跳过',
}

function progress(job: Job) { return job.total ? Math.round(job.done / job.total * 100) : 0 }
function itemTitle(item: JobItem) { return item.sourceId || item.sessionId || item.rawPath || item.id }
function canCancel(job: Job) { return job.status === 'queued' || job.status === 'running' }
function isTerminal(job: Job) { return ['succeeded', 'failed', 'cancelled', 'interrupted'].includes(job.status) }
function canRetry(item: JobItem) { return ['failed', 'cancelled', 'interrupted'].includes(item.status) }
function visibleItems(job: Job) { return job.kind === 'sync' ? job.items.slice(-40) : job.items }
function elapsed(job: Job) {
  const start = job.startedAt || job.createdAt
  const end = job.finishedAt ? Date.parse(job.finishedAt) : clock.value
  const seconds = Math.max(0, Math.floor((end - Date.parse(start)) / 1000))
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}
function currentItemSummary(job: Job) {
  const items = job.items.filter((candidate) => candidate.status === 'running')
  if (!items.length) return null
  if (items.length === 1) return `当前：${itemTitle(items[0])}`
  return `正在并行处理 ${items.length} 项：${items.slice(0, 2).map(itemTitle).join('、')}${items.length > 2 ? '…' : ''}`
}
function jobPhase(job: Job) {
  if (job.kind === 'analysis' && job.status === 'running') {
    const running = job.items.filter((item) => item.status === 'running').length
    return running === 0 ? '等待执行' : running === 1 ? '处理中' : '并行处理中'
  }
  return phaseLabel[job.phase] ?? job.phase
}

onMounted(async () => {
  const config = await api.getConfig().catch(() => null)
  providerOptions.value = config?.distiller.profiles.map((profile) => ({
    label: profile.id === config.distiller.activeProfileId ? `${profile.name}（当前）` : profile.name,
    key: profile.id,
  })) ?? []
})
</script>

<template>
  <n-modal v-model:show="show">
    <section class="task-center" role="dialog" aria-modal="true" aria-labelledby="progress-center-title">
      <header class="task-header">
        <div class="flex items-center gap-2"><span class="i-lucide-activity w-4 h-4" /><strong id="progress-center-title">进度中心</strong></div>
        <div class="flex items-center gap-1">
          <n-button v-if="hasCompleted" quaternary size="tiny" @click="queue.clear()">
            <span class="inline-flex items-center gap-1"><span class="i-lucide-trash-2 w-3.5 h-3.5" />清除已完成</span>
          </n-button>
          <n-button quaternary size="tiny" aria-label="关闭进度中心" @click="show = false"><span class="i-lucide-x w-4 h-4" /></n-button>
        </div>
      </header>

      <div class="task-list">
        <n-empty v-if="!jobs.length" description="暂无任务" class="task-empty" />
        <article v-for="job in jobs" :key="job.id" class="job-card">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <n-spin v-if="job.status === 'running'" size="small" />
                <span v-else :class="job.kind === 'sync' ? 'i-lucide-refresh-cw' : 'i-lucide-sparkles'" class="w-4 h-4" />
                <strong class="text-sm">{{ job.kind === 'sync' ? '同步任务' : '知识分析' }}</strong>
                <n-tag size="tiny" :type="job.status === 'failed' ? 'error' : job.status === 'succeeded' ? 'success' : 'default'">{{ statusLabel[job.status] }}</n-tag>
              </div>
              <p class="job-meta">
                {{ jobPhase(job) }} · {{ job.done }}/{{ job.total }}
                <template v-if="job.provider"> · {{ job.provider }} / {{ job.model }}</template>
                · {{ elapsed(job) }}
              </p>
              <p v-if="currentItemSummary(job)" class="job-current">{{ currentItemSummary(job) }}</p>
            </div>
            <div class="job-actions">
              <n-button v-if="canCancel(job)" size="tiny" secondary type="warning" @click="queue.cancel(job.id)">取消</n-button>
              <n-button v-else-if="isTerminal(job)" size="tiny" quaternary @click="queue.clear(job.id)">清除</n-button>
            </div>
          </div>

          <n-progress type="line" :percentage="progress(job)" :height="5" :show-indicator="false" class="mt-2" />

          <div class="item-list">
            <div v-for="item in visibleItems(job)" :key="item.id" class="item-row">
              <span v-if="item.status === 'running'" class="i-lucide-loader-2 w-3.5 h-3.5 animate-spin text-[#49685c]" />
              <span v-else-if="item.status === 'succeeded'" class="i-lucide-check-circle w-3.5 h-3.5 text-emerald-600" />
              <span v-else-if="item.status === 'queued'" class="i-lucide-clock-3 w-3.5 h-3.5 text-neutral-400" />
              <span v-else class="i-lucide-alert-circle w-3.5 h-3.5 text-[#c95f32]" />
              <div class="min-w-0 flex-1">
                <p class="truncate">{{ itemTitle(item) }}</p>
                <p v-if="item.error" class="item-error">{{ item.error }}</p>
                <p v-else class="item-phase">{{ phaseLabel[item.phase] ?? item.phase }}<template v-if="item.durationMs != null"> · {{ (item.durationMs / 1000).toFixed(1) }}s</template></p>
              </div>
              <n-dropdown
                v-if="job.kind === 'analysis' && canRetry(item)"
                trigger="click"
                :options="providerOptions"
                @select="queue.retry(job.id, item.id, String($event))"
              >
                <n-button quaternary size="tiny">选择配置重试</n-button>
              </n-dropdown>
              <n-button v-else-if="canRetry(item)" quaternary size="tiny" @click="queue.retry(job.id, item.id)">重试</n-button>
            </div>
          </div>
        </article>
      </div>
    </section>
  </n-modal>
</template>

<style scoped>
.task-center { width:min(560px,calc(100vw - 32px)); max-height:calc(100vh - 96px); overflow:hidden; border:1px solid var(--line); border-radius:var(--panel-radius); background:var(--surface); box-shadow:0 18px 46px rgba(20,24,22,.13); }
.dark .task-center { border-color:rgba(255,255,255,.1); }
.task-header { display: flex; align-items: center; justify-content: space-between; padding: 11px 13px; border-bottom: 1px solid rgba(122,125,117,.16); }
.task-list { max-height:calc(100vh - 160px); overflow-y:auto; padding:0 13px 8px; }
.task-empty { padding:48px 0; }
.job-card { padding: 13px 0; border-bottom: 1px solid color-mix(in srgb,var(--line) 72%,transparent); background: transparent; }
.job-card:last-child { border-bottom:0; }
.job-meta { margin-top: 4px; color: var(--muted); font-size: 11px; overflow-wrap: anywhere; }
.job-current { margin-top:3px; overflow:hidden; color:var(--ink-soft); font-size:11px; text-overflow:ellipsis; white-space:nowrap; }
.job-actions { display:flex; flex-shrink:0; align-items:center; gap:4px; }
.item-list { margin-top: 9px; max-height: 230px; overflow-y: auto; }
.item-row { display: flex; align-items: flex-start; gap: 7px; padding: 7px 0; font-size: 11px; border-top: 1px solid rgba(122,125,117,.12); }
.item-error { margin-top: 2px; color: var(--vermilion); overflow-wrap: anywhere; }
.item-phase { margin-top: 2px; color: var(--muted); }
</style>
