<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { NButton, NDropdown, NProgress, NSpin, NTag } from 'naive-ui'
import { useAnalysisQueueStore } from '../stores/analysisQueue'
import { api } from '../lib/tauri'
import type { Job, JobItem } from '../types'

const queue = useAnalysisQueueStore()
const { jobs, clock, hasAny, isIdle } = storeToRefs(queue)
const collapsed = ref(false)
const providerOptions = ref<Array<{ label: string; key: string }>>([])
const activeJob = computed(() => jobs.value.find((job) => job.status === 'running' || job.status === 'queued') ?? jobs.value[0])

watch(hasAny, (value) => { if (!value) collapsed.value = false })
watch(isIdle, (value) => { if (value && hasAny.value) collapsed.value = true }, { immediate: true })

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
function canRetry(item: JobItem) { return ['failed', 'cancelled', 'interrupted'].includes(item.status) }
function visibleItems(job: Job) { return job.kind === 'sync' ? job.items.slice(-40) : job.items }
function elapsed(job: Job) {
  const start = job.startedAt || job.createdAt
  const end = job.finishedAt ? Date.parse(job.finishedAt) : clock.value
  const seconds = Math.max(0, Math.floor((end - Date.parse(start)) / 1000))
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}
function currentItemTitle(job: Job) {
  const item = job.items.find((candidate) => candidate.status === 'running')
  return item ? itemTitle(item) : null
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
  <Transition name="task-center">
    <button
      v-if="hasAny && collapsed"
      type="button"
      class="task-pill"
      aria-label="展开任务中心"
      @click="collapsed = false"
    >
      <span v-if="activeJob?.status === 'running'" class="i-lucide-loader-2 w-4 h-4 animate-spin" />
      <span v-else class="i-lucide-activity w-4 h-4" />
      <span>{{ activeJob?.kind === 'sync' ? '同步' : '分析' }}</span>
      <span class="tabular-nums">{{ activeJob?.done ?? 0 }}/{{ activeJob?.total ?? 0 }}</span>
      <span class="i-lucide-chevron-up w-4 h-4" />
    </button>
  </Transition>

  <Transition name="task-center">
    <section v-if="hasAny && !collapsed" class="task-center" aria-live="polite" aria-label="任务中心">
      <header class="task-header">
        <div class="flex items-center gap-2"><span class="i-lucide-activity w-4 h-4" /><strong>任务中心</strong></div>
        <div class="flex items-center gap-1">
          <n-button quaternary size="tiny" aria-label="收起任务中心" @click="collapsed = true"><span class="i-lucide-chevron-down w-4 h-4" /></n-button>
          <n-button v-if="isIdle" quaternary size="tiny" aria-label="关闭任务中心" @click="queue.clear()"><span class="i-lucide-x w-4 h-4" /></n-button>
        </div>
      </header>

      <div class="task-list">
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
                {{ phaseLabel[job.phase] ?? job.phase }} · {{ job.done }}/{{ job.total }}
                <template v-if="job.provider"> · {{ job.provider }} / {{ job.model }}</template>
                · {{ elapsed(job) }}
              </p>
              <p v-if="currentItemTitle(job)" class="job-current">当前：{{ currentItemTitle(job) }}</p>
            </div>
            <n-button v-if="canCancel(job)" size="tiny" secondary type="warning" @click="queue.cancel(job.id)">取消</n-button>
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
  </Transition>
</template>

<style scoped>
.task-center { position: fixed; right: 20px; bottom: 20px; z-index: 60; width: min(360px, calc(100vw - 32px)); max-height: min(520px, calc(100vh - 100px)); overflow: hidden; border: 1px solid var(--line); border-radius: 14px; background: var(--surface-glass); box-shadow: 0 18px 54px rgba(20, 24, 22, .15); backdrop-filter: blur(22px) saturate(1.08); }
.dark .task-center { --task-bg: #171917; border-color: rgba(255,255,255,.1); }
.task-header { display: flex; align-items: center; justify-content: space-between; padding: 11px 13px; border-bottom: 1px solid rgba(122,125,117,.16); }
.task-list { max-height: 442px; overflow-y: auto; padding: 10px; }
.job-card { padding: 12px; border: 1px solid rgba(122,125,117,.16); border-radius: 12px; background: rgba(255,255,255,.42); }
.job-card + .job-card { margin-top: 8px; }
.dark .job-card { background: rgba(0,0,0,.14); }
.job-meta { margin-top: 4px; color: var(--muted); font-size: 11px; overflow-wrap: anywhere; }
.job-current { margin-top:3px; overflow:hidden; color:var(--ink-soft); font-size:11px; text-overflow:ellipsis; white-space:nowrap; }
.item-list { margin-top: 9px; max-height: 230px; overflow-y: auto; }
.item-row { display: flex; align-items: flex-start; gap: 7px; padding: 7px 0; font-size: 11px; border-top: 1px solid rgba(122,125,117,.12); }
.item-error { margin-top: 2px; color: var(--vermilion); overflow-wrap: anywhere; }
.item-phase { margin-top: 2px; color: var(--muted); }
.task-pill { position: fixed; right: 20px; bottom: 20px; z-index: 60; display: flex; align-items: center; gap: 8px; padding: 9px 11px; border: 1px solid rgba(122,125,117,.22); border-radius: 12px; background: color-mix(in srgb, var(--task-bg, #f7f5ef) 90%, transparent); color: inherit; box-shadow: 0 10px 32px rgba(20,24,22,.14); backdrop-filter: blur(18px); font-size: 12px; }
.task-center-enter-active,.task-center-leave-active { transition: opacity .16s ease, transform .16s ease; }
.task-center-enter-from,.task-center-leave-to { opacity: 0; transform: translateY(8px); }
</style>
