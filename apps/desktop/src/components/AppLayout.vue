<script setup lang="ts">
import { onMounted } from 'vue'
import { useMessage } from 'naive-ui'
import TopBar from './TopBar.vue'
import Sidebar from './Sidebar.vue'
import AnalysisQueuePanel from './AnalysisQueuePanel.vue'
import { api } from '../lib/tauri'

const message = useMessage()

onMounted(async () => {
  const path = await api.getDatabaseBackupPath().catch(() => null)
  if (path) message.warning(`v0.2 数据库已重建，旧库备份位于：${path}`, { duration: 0, closable: true })
})
</script>

<template>
  <div class="h-screen flex flex-col overflow-hidden bg-white text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
    <TopBar />
    <div class="flex flex-1 min-h-0">
      <Sidebar />
      <main class="flex-1 min-w-0 min-h-0 overflow-hidden bg-[#fafafa] dark:bg-neutral-950">
        <router-view />
      </main>
    </div>
    <AnalysisQueuePanel />
  </div>
</template>
