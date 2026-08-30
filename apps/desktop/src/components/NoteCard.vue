<script setup lang="ts">
import type { Card } from '../types'
import NoteHeader from './NoteHeader.vue'
import MarkdownRenderer from './MarkdownRenderer.vue'
import SourceInfo from './SourceInfo.vue'
import ActionBar from './ActionBar.vue'

defineProps<{
  card: Card
  mode: 'note' | 'chat'
  /** 重新分析进行中 */
  analyzing?: boolean
  /** 导出 Markdown 进行中 */
  exportLoading?: boolean
}>()

const emit = defineEmits<{
  'update:mode': [m: 'note' | 'chat']
  close: []
  reanalyze: []
  exportMarkdown: []
}>()
</script>

<template>
  <article class="note-document">
    <NoteHeader
      :title="card.title"
      :summary="card.summary"
      :card-type="card.type"
      :tags="card.tags"
      :tech-stack="card.techStack"
    />
    <MarkdownRenderer v-if="mode === 'note'" :source="card.note" />
    <slot v-else name="chat" />
    <SourceInfo
      :source-name="card.sourceName"
      :project-name="card.projectName"
      :created-at="card.createdAt"
      :prompt-tokens="card.promptTokens"
      :completion-tokens="card.completionTokens"
      :cost-yuan="card.costYuan"
      :source-session-path="card.sourceSessionPath"
      :source-session-external-id="card.sourceSessionExternalId"
    />
    <ActionBar
      :mode="mode"
      :analyzing="analyzing"
      :export-loading="exportLoading"
      @update:mode="emit('update:mode', $event)"
      @close="emit('close')"
      @reanalyze="emit('reanalyze')"
      @export-markdown="emit('exportMarkdown')"
    />
  </article>
</template>

<style scoped>
.note-document { padding:30px 4px 44px; border-top:1px solid var(--line); background:transparent; }
</style>
