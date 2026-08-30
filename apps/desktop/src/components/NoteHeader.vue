<script setup lang="ts">
import { NTag } from 'naive-ui'
import { getCardTypeLabel } from '@chattake/shared'

defineProps<{
  title: string
  summary: string | null
  /** 存储值为英文枚举，展示用 getCardTypeLabel */
  cardType?: string | null
  tags: string[]
  /** LLM 识别的技术栈列表 */
  techStack?: string[]
}>()
</script>

<template>
  <header class="space-y-2 mb-4">
    <h1 class="text-xl font-semibold tracking-tight">
      {{ title }}
    </h1>
    <p v-if="summary" class="text-sm text-neutral-600 dark:text-neutral-400">
      {{ summary }}
    </p>
    <!-- 知识类型（与知识库/侧栏同一套中文映射） -->
    <div v-if="cardType" class="flex flex-wrap gap-1">
      <n-tag size="small" round :bordered="false" type="success">
        {{ getCardTypeLabel(cardType) }}
      </n-tag>
    </div>
    <!-- 技术栈：蓝色系标签，与普通 tag 区分 -->
    <div v-if="techStack && techStack.length" class="flex flex-wrap gap-1">
      <n-tag
        v-for="t in techStack"
        :key="t"
        size="small"
        round
        :bordered="false"
        type="info"
      >
        {{ t }}
      </n-tag>
    </div>
    <!-- 普通 tag -->
    <div v-if="tags.length" class="flex flex-wrap gap-1">
      <n-tag v-for="t in tags" :key="t" size="small" round :bordered="false">
        {{ t }}
      </n-tag>
    </div>
  </header>
</template>
