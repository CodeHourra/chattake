<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  NAlert, NAutoComplete, NButton, NCard, NDescriptions, NDescriptionsItem, NDivider,
  NForm, NFormItem, NInput, NInputNumber, NModal, NSelect, NSpace, NSpin, NSwitch,
  NTabPane, NTabs, NTag,
} from 'naive-ui'
import { getIdentifier, getTauriVersion, getVersion } from '@tauri-apps/api/app'
import MarkdownRenderer from './MarkdownRenderer.vue'
import appChangelogMd from '../data/app-changelog.md?raw'
import { api } from '../lib/tauri'
import type { AppConfigDto, McpInfo, SourceConfigDto } from '../types'

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ 'update:show': [value: boolean] }>()

const providerPresets = [
  { label: 'OpenAI', value: 'openai', baseUrl: 'https://api.openai.com/v1' },
  { label: 'DeepSeek', value: 'deepseek', baseUrl: 'https://api.deepseek.com/v1' },
  { label: 'Moonshot（Kimi）', value: 'moonshot', baseUrl: 'https://api.moonshot.cn/v1' },
  { label: '智谱 GLM', value: 'zhipu', baseUrl: 'https://open.bigmodel.cn/api/paas/v4' },
  { label: '硅基流动', value: 'siliconflow', baseUrl: 'https://api.siliconflow.cn/v1' },
  { label: '自定义 OpenAI-compatible', value: 'openai-compatible', baseUrl: '' },
]

const loading = ref(false)
const saving = ref(false)
const modelLoading = ref(false)
const testing = ref(false)
const errorMsg = ref('')
const successMsg = ref('')
const workingConfig = ref<AppConfigDto | null>(null)
const selectedProfileId = ref('')
const modelOptions = ref<Record<string, string[]>>({})
const aboutMeta = ref<{ version: string; tauriVersion: string; identifier: string } | null>(null)
const aboutLoading = ref(false)
const aboutError = ref('')
const mcpInfo = ref<McpInfo | null>(null)
const mcpLoading = ref(false)
const mcpError = ref('')

const selectedProfile = computed(() =>
  workingConfig.value?.distiller.profiles.find((profile) => profile.id === selectedProfileId.value) ?? null,
)

const selectedModelOptions = computed(() =>
  (modelOptions.value[selectedProfileId.value] ?? []).map((value) => ({ label: value, value })),
)

watch(() => props.show, async (show) => {
  if (!show) return
  await Promise.all([loadConfig(), loadAboutMeta(), loadMcpInfo()])
}, { immediate: true })

async function loadConfig() {
  loading.value = true
  errorMsg.value = ''
  try {
    workingConfig.value = await api.getConfig()
    selectedProfileId.value = workingConfig.value.distiller.activeProfileId
    modelOptions.value = {}
  } catch (error) {
    errorMsg.value = `配置加载失败：${error}`
  } finally {
    loading.value = false
  }
}

async function loadMcpInfo() {
  mcpLoading.value = true
  mcpError.value = ''
  try { mcpInfo.value = await api.getMcpInfo() }
  catch (error) { mcpError.value = String(error) }
  finally { mcpLoading.value = false }
}

async function copyMcpConfig() {
  if (!mcpInfo.value?.configSnippet) return
  try {
    await navigator.clipboard.writeText(mcpInfo.value.configSnippet)
    successMsg.value = 'MCP 配置片段已复制'
  } catch (error) {
    errorMsg.value = `复制失败：${error}`
  }
}

async function loadAboutMeta() {
  aboutLoading.value = true
  aboutError.value = ''
  try {
    const [version, tauriVersion, identifier] = await Promise.all([
      getVersion(), getTauriVersion(), getIdentifier(),
    ])
    aboutMeta.value = { version, tauriVersion, identifier }
  } catch (error) {
    aboutError.value = error instanceof Error ? error.message : '无法读取应用版本'
  } finally {
    aboutLoading.value = false
  }
}

function makeId() {
  return globalThis.crypto?.randomUUID?.() ?? `profile-${Date.now()}`
}

function addProfile(provider = 'openai') {
  const config = workingConfig.value
  if (!config) return
  const preset = providerPresets.find((item) => item.value === provider) ?? providerPresets[0]
  const id = makeId()
  config.distiller.profiles.push({
    id, name: preset.label, provider: preset.value, baseUrl: preset.baseUrl,
    apiKey: '', model: '', timeoutSecs: 120,
  })
  selectedProfileId.value = id
}

function copyProfile() {
  const config = workingConfig.value
  const profile = selectedProfile.value
  if (!config || !profile) return
  const copy = { ...profile, id: makeId(), name: `${profile.name} 副本` }
  config.distiller.profiles.push(copy)
  selectedProfileId.value = copy.id
}

function deleteProfile() {
  const config = workingConfig.value
  const profile = selectedProfile.value
  if (!config || !profile) return
  if (profile.id === config.distiller.activeProfileId) {
    errorMsg.value = '当前激活配置不能删除，请先设为其他配置'
    return
  }
  config.distiller.profiles = config.distiller.profiles.filter((item) => item.id !== profile.id)
  selectedProfileId.value = config.distiller.profiles[0]?.id ?? ''
}

function setActive() {
  if (workingConfig.value && selectedProfile.value) {
    workingConfig.value.distiller.activeProfileId = selectedProfile.value.id
  }
}

function onProviderChange(provider: string) {
  const profile = selectedProfile.value
  const preset = providerPresets.find((item) => item.value === provider)
  if (!profile || !preset) return
  profile.provider = provider
  profile.baseUrl = preset.baseUrl
  modelOptions.value[profile.id] = []
}

async function refreshModels() {
  const profile = selectedProfile.value
  if (!profile) return
  modelLoading.value = true
  errorMsg.value = ''
  try {
    const models = await api.listProviderModels(profile)
    modelOptions.value[profile.id] = models
    successMsg.value = models.length ? `已加载 ${models.length} 个模型` : '接口连接成功，但未返回模型'
  } catch (error) {
    errorMsg.value = `${error}；仍可手动填写完整模型 ID`
  } finally {
    modelLoading.value = false
  }
}

async function testConnection() {
  const profile = selectedProfile.value
  if (!profile) return
  testing.value = true
  errorMsg.value = ''
  try {
    successMsg.value = await api.testProvider(profile)
  } catch (error) {
    errorMsg.value = String(error)
  } finally {
    testing.value = false
  }
}

async function save() {
  if (!workingConfig.value) return
  saving.value = true
  errorMsg.value = ''
  try {
    await api.saveConfig(workingConfig.value)
    successMsg.value = '配置已保存'
  } catch (error) {
    errorMsg.value = `保存失败：${error}`
  } finally {
    saving.value = false
  }
}

function setSourceEnabled(source: SourceConfigDto, enabled: boolean) { source.enabled = enabled }
function close() { emit('update:show', false) }
</script>

<template>
  <n-modal :show="props.show" :mask-closable="false" @update:show="emit('update:show', $event)">
    <n-card
      class="settings-shell"
      title="设置"
      :bordered="false"
      :content-style="{ padding: 0, minHeight: '280px', overflow: 'hidden' }"
      role="dialog"
      aria-modal="true"
    >
      <template #header-extra>
        <n-button quaternary size="small" aria-label="关闭设置" @click="close">
          <span class="i-lucide-x w-4 h-4" />
        </n-button>
      </template>

      <div v-if="loading" class="flex justify-center py-16"><n-spin size="large" /></div>
      <div v-else-if="errorMsg && !workingConfig" class="p-6">
        <n-alert type="error">{{ errorMsg }}</n-alert>
      </div>

      <n-tabs v-else-if="workingConfig" type="line" animated class="px-6">
        <n-tab-pane name="distiller" tab="AI 配置">
          <div class="settings-scroll py-3">
            <n-alert type="info" :bordered="false" class="!text-xs mb-3">
              每个分析任务固定使用启动时选中的配置；失败后不会自动跨供应商切换。
            </n-alert>
            <div class="profile-layout">
              <aside class="profile-list" aria-label="API 配置列表">
                <button
                  v-for="profile in workingConfig.distiller.profiles"
                  :key="profile.id"
                  type="button"
                  class="profile-item"
                  :class="{ selected: profile.id === selectedProfileId }"
                  @click="selectedProfileId = profile.id"
                >
                  <span class="truncate">{{ profile.name }}</span>
                  <span v-if="profile.id === workingConfig.distiller.activeProfileId" class="active-dot" title="当前配置" />
                </button>
                <n-select
                  size="small"
                  placeholder="新增配置"
                  :options="providerPresets"
                  @update:value="addProfile"
                />
              </aside>

              <section v-if="selectedProfile" class="profile-editor">
                <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
                  <div class="flex items-center gap-2">
                    <n-tag v-if="selectedProfile.id === workingConfig.distiller.activeProfileId" type="success" size="small">当前配置</n-tag>
                    <span class="text-xs text-neutral-400 font-mono truncate max-w-48">{{ selectedProfile.id }}</span>
                  </div>
                  <n-space :size="6">
                    <n-button size="tiny" secondary @click="copyProfile">复制</n-button>
                    <n-button
                      v-if="selectedProfile.id !== workingConfig.distiller.activeProfileId"
                      size="tiny"
                      secondary
                      @click="setActive"
                    >设为当前</n-button>
                    <n-button size="tiny" quaternary type="error" @click="deleteProfile">删除</n-button>
                  </n-space>
                </div>

                <n-form size="small" label-placement="left" label-width="88">
                  <n-form-item label="显示名称"><n-input v-model:value="selectedProfile.name" /></n-form-item>
                  <n-form-item label="供应商">
                    <n-select :value="selectedProfile.provider" :options="providerPresets" @update:value="onProviderChange" />
                  </n-form-item>
                  <n-form-item label="Base URL"><n-input v-model:value="selectedProfile.baseUrl" placeholder="https://example.com/v1" /></n-form-item>
                  <n-form-item label="API Key">
                    <n-input v-model:value="selectedProfile.apiKey" type="password" show-password-on="click" placeholder="sk-..." />
                  </n-form-item>
                  <n-form-item label="模型">
                    <div class="flex gap-2 w-full">
                      <n-auto-complete
                        v-model:value="selectedProfile.model"
                        :options="selectedModelOptions"
                        placeholder="可搜索或手填完整模型 ID"
                        clearable
                      />
                      <n-button secondary :loading="modelLoading" @click="refreshModels">刷新模型</n-button>
                    </div>
                  </n-form-item>
                  <n-form-item label="超时（秒）">
                    <n-input-number v-model:value="selectedProfile.timeoutSecs" :min="10" :max="600" />
                  </n-form-item>
                </n-form>
                <div class="flex items-center gap-3">
                  <n-button secondary :loading="testing" @click="testConnection">测试连接</n-button>
                  <span class="text-xs text-neutral-400">模型接口不可用时会发送最小请求，可能消耗极少 Token。</span>
                </div>
              </section>
            </div>
          </div>
        </n-tab-pane>

        <n-tab-pane name="sources" tab="数据源">
          <div class="settings-scroll py-3 space-y-2">
            <n-alert type="info" :bordered="false" class="!text-xs mb-3">
              仅支持 Claude Code、Cursor、Codex 和 CodeBuddy。启动扫描只发现变化，不会自动调用模型。
            </n-alert>
            <div v-for="source in workingConfig.collector.sources" :key="source.id" class="source-row">
              <div class="min-w-0">
                <div class="flex items-center gap-2"><strong class="text-sm">{{ source.name }}</strong><n-tag size="tiny">{{ source.id }}</n-tag></div>
                <p v-for="dir in source.scanDirs" :key="dir" class="text-xs text-neutral-400 font-mono truncate mt-1">{{ dir }}</p>
              </div>
              <n-switch :value="source.enabled" @update:value="(value: boolean) => setSourceEnabled(source, value)" />
            </div>
            <div class="source-row">
              <div><strong class="text-sm">启动时扫描变化</strong><p class="text-xs text-neutral-400 mt-1">只采集，不分析，不消耗 Token</p></div>
              <n-switch v-model:value="workingConfig.sync.scanOnStartup" />
            </div>
          </div>
        </n-tab-pane>

        <n-tab-pane name="mcp" tab="MCP">
          <div class="settings-scroll py-4 space-y-4">
            <n-alert type="info" :bordered="false" class="!text-xs">
              MCP 仅以只读方式开放已发布知识，不会暴露草稿、任务、API Key 或供应商配置，也不会自动修改外部应用。
            </n-alert>
            <div v-if="mcpLoading" class="flex justify-center py-8"><n-spin /></div>
            <n-alert v-else-if="mcpError" type="warning">{{ mcpError }}</n-alert>
            <template v-else-if="mcpInfo">
              <n-descriptions :column="1" bordered size="small">
                <n-descriptions-item label="状态"><n-tag :type="mcpInfo.available ? 'success' : 'warning'" size="small">{{ mcpInfo.available ? '可用' : '未构建' }}</n-tag></n-descriptions-item>
                <n-descriptions-item label="程序路径"><span class="font-mono text-xs break-all">{{ mcpInfo.binaryPath ?? '请先构建 chattake-mcp' }}</span></n-descriptions-item>
                <n-descriptions-item label="数据库"><span class="font-mono text-xs break-all">{{ mcpInfo.databasePath }}</span></n-descriptions-item>
              </n-descriptions>
              <template v-if="mcpInfo.configSnippet">
                <div class="flex items-center justify-between"><strong class="text-sm">手动配置片段</strong><n-button size="small" secondary @click="copyMcpConfig">复制</n-button></div>
                <pre class="mcp-config">{{ mcpInfo.configSnippet }}</pre>
              </template>
            </template>
          </div>
        </n-tab-pane>

        <n-tab-pane name="about" tab="关于有得">
          <div class="settings-scroll py-4 space-y-4">
            <div class="text-center"><h3 class="text-lg font-semibold">有得 · AI 对话知识库</h3><p class="text-xs text-neutral-500 mt-1">ChatTake — 让每次 AI 对话，都有所得</p></div>
            <div v-if="aboutLoading" class="flex justify-center py-8"><n-spin /></div>
            <n-alert v-else-if="aboutError" type="warning">{{ aboutError }}</n-alert>
            <n-descriptions v-else-if="aboutMeta" :column="1" bordered size="small">
              <n-descriptions-item label="应用版本">{{ aboutMeta.version }}</n-descriptions-item>
              <n-descriptions-item label="Tauri 版本">{{ aboutMeta.tauriVersion }}</n-descriptions-item>
              <n-descriptions-item label="应用标识">{{ aboutMeta.identifier }}</n-descriptions-item>
            </n-descriptions>
            <n-divider />
            <MarkdownRenderer :source="appChangelogMd" />
          </div>
        </n-tab-pane>
      </n-tabs>

      <template #footer>
        <n-alert v-if="errorMsg && workingConfig" type="error" :bordered="false" class="mb-3 !text-xs">{{ errorMsg }}</n-alert>
        <n-alert v-if="successMsg" type="success" :bordered="false" class="mb-3 !text-xs">{{ successMsg }}</n-alert>
        <div class="flex justify-end gap-2">
          <n-button size="small" @click="close">取消</n-button>
          <n-button type="primary" size="small" :loading="saving" @click="save">保存配置</n-button>
        </div>
      </template>
    </n-card>
  </n-modal>
</template>

<style scoped>
.settings-shell { width: min(860px, calc(100vw - 32px)); max-height: 84vh; overflow: hidden; border-radius: 16px; }
.settings-scroll { max-height: calc(76vh - 190px); overflow-y: auto; }
.profile-layout { display: grid; grid-template-columns: 190px minmax(0, 1fr); gap: 16px; }
.profile-list { display: flex; flex-direction: column; gap: 6px; padding-right: 12px; border-right: 1px solid var(--n-border-color); }
.profile-item { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 38px; padding: 8px 10px; border: 1px solid transparent; border-radius: 10px; text-align: left; color: inherit; }
.profile-item:hover { background: rgba(78, 102, 91, .08); }
.profile-item.selected { border-color: rgba(78, 102, 91, .35); background: rgba(78, 102, 91, .12); }
.active-dot { width: 7px; height: 7px; border-radius: 999px; background: #49685c; box-shadow: 0 0 0 3px rgba(73, 104, 92, .14); }
.profile-editor { min-width: 0; padding: 4px 4px 12px; }
.source-row { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 14px; border: 1px solid var(--n-border-color); border-radius: 12px; }
.mcp-config { max-height: 220px; overflow: auto; padding: 14px; border: 1px solid var(--n-border-color); border-radius: 10px; background: rgba(73, 104, 92, .06); font-size: 12px; line-height: 1.6; white-space: pre-wrap; overflow-wrap: anywhere; }
@media (max-width: 700px) {
  .profile-layout { grid-template-columns: 1fr; }
  .profile-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); padding-right: 0; padding-bottom: 12px; border-right: 0; border-bottom: 1px solid var(--n-border-color); }
}
</style>
