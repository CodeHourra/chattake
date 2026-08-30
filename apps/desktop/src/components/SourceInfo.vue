<script setup lang="ts">
/**
 * 底部来源元信息：与 SessionDetailView 头部次要信息保持同类排版（图标 + 标签/数值分层）。
 */
defineProps<{
  sourceName: string | null
  projectName: string | null
  createdAt: string
  promptTokens: number
  completionTokens: number
  costYuan: number
  /** 来源会话路径（优先 raw_path，后端 COALESCE） */
  sourceSessionPath?: string | null
  /** 数据源侧会话 ID（非本应用 DB 主键） */
  sourceSessionExternalId?: string | null
}>()
</script>

<template>
  <div class="text-xs text-neutral-500 space-y-2 border-t border-neutral-200 dark:border-neutral-800 pt-3 mt-3">
    <!-- 来源与时间：可换行，长项目名 hover 可看全 -->
    <div
      class="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[11px] text-neutral-500 dark:text-neutral-400"
    >
      <span class="inline-flex items-center gap-1.5 min-w-0 max-w-full">
        <span class="i-lucide-messages-square w-3.5 h-3.5 shrink-0 opacity-80" aria-hidden="true" />
        <span class="text-neutral-400 dark:text-neutral-500 shrink-0">数据源</span>
        <span class="font-medium text-neutral-700 dark:text-neutral-200 truncate">
          {{ sourceName ?? '—' }}
        </span>
        <span class="text-neutral-300 dark:text-neutral-600 shrink-0" aria-hidden="true">/</span>
        <span
          class="text-neutral-600 dark:text-neutral-300 truncate min-w-0"
          :title="projectName || undefined"
        >
          {{ projectName ?? '—' }}
        </span>
      </span>
      <span class="inline-flex items-center gap-1.5 shrink-0 tabular-nums">
        <span class="i-lucide-calendar w-3.5 h-3.5 opacity-80" aria-hidden="true" />
        {{ createdAt ? createdAt.replace('T', ' ').slice(0, 16) : '—' }}
      </span>
    </div>
    <!-- <div
      v-if="sourceSessionPath || sourceSessionExternalId"
      class="text-[11px] text-neutral-400 dark:text-neutral-500 space-y-0.5 pl-0.5"
    >
      <div v-if="sourceSessionPath" class="break-all">
        <span class="text-neutral-400">来源会话路径</span>
        {{ sourceSessionPath }}
      </div>
      <div v-if="sourceSessionExternalId" class="font-mono break-all">
        <span class="text-neutral-400 font-sans">来源会话 ID</span>
        {{ sourceSessionExternalId }}
      </div>
    </div> -->
    <div
      class="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-neutral-500 dark:text-neutral-400"
    >
      <span class="inline-flex items-center gap-1.5">
        <span class="i-lucide-coins w-3.5 h-3.5 opacity-80" aria-hidden="true" />
        <span class="text-neutral-400 dark:text-neutral-500">提炼消耗</span>
        <span class="tabular-nums text-neutral-700 dark:text-neutral-200">
          {{ promptTokens + completionTokens }} tokens
        </span>
      </span>
      <span class="text-neutral-300 dark:text-neutral-600" aria-hidden="true">·</span>
      <span class="tabular-nums">¥{{ costYuan.toFixed(4) }}</span>
    </div>
  </div>
</template>
