import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { api } from '../lib/tauri'
import type { Job } from '../types'
import { useSessionsStore } from './sessions'

export interface QueueTask {
  jobId: string
  itemId: string
  sessionId: string
  displayTitle: string
  status: 'pending' | 'running' | 'done' | 'error'
  errorMessage?: string
  elapsedSec: number
}

const terminal = new Set(['succeeded', 'failed', 'cancelled', 'interrupted'])

export const useAnalysisQueueStore = defineStore('analysisQueue', () => {
  const jobs = ref<Job[]>([])
  const clock = ref(Date.now())
  let initialized = false
  let unlisten: UnlistenFn | null = null
  let timer: ReturnType<typeof setInterval> | null = null

  const tasks = computed<QueueTask[]>(() => jobs.value
    .filter((job) => job.kind === 'analysis')
    .flatMap((job) => job.items.map((item) => ({
      jobId: job.id,
      itemId: item.id,
      sessionId: item.sessionId ?? '',
      displayTitle: item.sessionId ?? '未知会话',
      status: item.status === 'queued' ? 'pending'
        : item.status === 'running' ? 'running'
          : item.status === 'succeeded' ? 'done' : 'error',
      errorMessage: item.error ?? undefined,
      elapsedSec: item.startedAt && item.status === 'running'
        ? Math.max(0, Math.floor((clock.value - Date.parse(item.startedAt)) / 1000)) : 0,
    }))))

  const currentTask = computed(() => tasks.value.find((task) => task.status === 'running') ?? null)
  const pendingCount = computed(() => tasks.value.filter((task) => task.status === 'pending').length)
  const totalCount = computed(() => jobs.value.reduce((sum, job) => sum + job.total, 0))
  const doneCount = computed(() => jobs.value.reduce((sum, job) => sum + job.done, 0))
  const hasAny = computed(() => jobs.value.length > 0)
  const isIdle = computed(() => hasAny.value && jobs.value.every((job) => terminal.has(job.status)))
  const progressPercent = computed(() => totalCount.value ? Math.round(doneCount.value / totalCount.value * 100) : 0)

  function upsert(job: Job) {
    const index = jobs.value.findIndex((item) => item.id === job.id)
    if (index < 0) jobs.value.unshift(job)
    else jobs.value[index] = job
    if (terminal.has(job.status)) void useSessionsStore().loadPage()
  }

  async function initialize() {
    if (initialized) return
    initialized = true
    jobs.value = await api.listJobs(true).catch(() => [])
    unlisten = await listen<Job>('job://updated', (event) => upsert(event.payload))
    timer = setInterval(() => { clock.value = Date.now() }, 1000)
  }

  async function startAnalysis(sessionIds: string[], providerProfileId?: string) {
    const sessions = useSessionsStore()
    sessionIds.forEach((id) => sessions.patchItem(id, { status: 'analyzing' }))
    const job = await api.startAnalysis(sessionIds, providerProfileId)
    upsert(job)
    return job
  }

  async function startSync(scope?: string) {
    const job = await api.startSync(scope)
    upsert(job)
    return job
  }

  async function cancel(jobId?: string) {
    const targets = jobs.value.filter((job) => !terminal.has(job.status) && (!jobId || job.id === jobId))
    await Promise.all(targets.map(async (job) => upsert(await api.cancelJob(job.id))))
  }

  async function retry(jobId: string, itemId: string, providerProfileId?: string) {
    const job = await api.retryJobItem(jobId, itemId, providerProfileId)
    upsert(job)
    return job
  }

  function clear() { jobs.value = jobs.value.filter((job) => !terminal.has(job.status)) }

  function dispose() {
    unlisten?.()
    unlisten = null
    if (timer) clearInterval(timer)
    timer = null
    initialized = false
  }

  return {
    jobs, clock, tasks, currentTask, pendingCount, totalCount, doneCount, hasAny, isIdle,
    progressPercent, initialize, startAnalysis, startSync, cancel, retry, clear, dispose,
  }
})
