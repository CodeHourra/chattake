import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api } from '../lib/tauri'
import type { SessionSummary } from '../types'
import { useFiltersStore } from './filters'

/**
 * 会话列表分页与加载状态
 */
export const useSessionsStore = defineStore('sessions', () => {
  const items = ref<SessionSummary[]>([])
  const total = ref(0)
  const page = ref(1)
  const pageSize = ref(20)
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function loadPage() {
    loading.value = true
    error.value = null
    const filters = useFiltersStore()
    try {
      const res = await api.listSessions({
        source: filters.sourceId || undefined,
        host: filters.sourceHost || undefined,
        project: filters.projectQuery || undefined,
        // statusFilter: '' 表示全部，传 undefined 让后端不过滤
        status: filters.statusFilter || undefined,
        page: page.value,
        pageSize: pageSize.value,
      })
      items.value = res.items
      total.value = res.total
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      console.error('[sessions] loadPage', e)
    } finally {
      loading.value = false
    }
  }

  function setPage(p: number) {
    page.value = Math.max(1, p)
    void loadPage()
  }

  function setPageSize(n: number) {
    pageSize.value = Math.min(200, Math.max(1, n))
    page.value = 1
    void loadPage()
  }

  /**
   * 分析完成后原地更新单条会话（避免整页刷新），
   * 将 status / value / cardId 写回 items 列表对应行。
   */
  function patchItem(id: string, patch: Partial<SessionSummary>) {
    const idx = items.value.findIndex((s) => s.id === id)
    if (idx >= 0) {
      items.value[idx] = { ...items.value[idx], ...patch }
    }
  }

  return {
    items,
    total,
    page,
    pageSize,
    loading,
    error,
    loadPage,
    setPage,
    setPageSize,
    patchItem,
  }
})
