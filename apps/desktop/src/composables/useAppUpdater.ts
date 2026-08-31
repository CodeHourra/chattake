import { ref, shallowRef } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { getVersion } from '@tauri-apps/api/app'

/** 与 UI 同步的更新流程阶段（独立「软件更新」弹窗用） */
export type AppUpdatePhase =
  | 'idle'
  | 'checking'
  | 'uptodate'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'error'

/**
 * 应用内更新：封装 tauri-plugin-updater 的检查与下载安装，并输出进度供进度条绑定。
 * 浏览器预览无 Tauri 时 check 会抛错，由调用方展示 errorText。
 */
export function useAppUpdater() {
  const phase = ref<AppUpdatePhase>('idle')
  const currentVersion = ref('')
  const busy = ref(false)
  /** 0–100；仅在已知总大小时递增 */
  const downloadPercent = ref(0)
  /** 未返回 contentLength 时仅展示不确定文案，不强行伪造百分比 */
  const downloadIndeterminate = ref(false)
  const statusText = ref('')
  const errorText = ref('')
  /** check() 返回带私有字段的类实例；必须保持原对象，不能被 Vue 深层代理 */
  const pendingUpdate = shallowRef<Awaited<ReturnType<typeof check>>>(null)

  function reset() {
    phase.value = 'idle'
    downloadPercent.value = 0
    downloadIndeterminate.value = false
    statusText.value = ''
    errorText.value = ''
    pendingUpdate.value = null
  }

  async function loadCurrentVersion() {
    try {
      currentVersion.value = await getVersion()
    } catch {
      currentVersion.value = '—'
    }
  }

  async function runCheck() {
    if (busy.value) return
    busy.value = true
    errorText.value = ''
    pendingUpdate.value = null
    phase.value = 'checking'
    try {
      const update = await check()
      if (!update) {
        phase.value = 'uptodate'
        return
      }
      pendingUpdate.value = update
      phase.value = 'available'
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      errorText.value =
        msg.includes('Tauri') || msg.includes('not allowed')
          ? '请在有得桌面客户端内使用（浏览器预览不支持检查更新）。'
          : `检查更新失败：${msg}`
      phase.value = 'error'
      console.error('[updater] check failed', e)
    } finally {
      busy.value = false
    }
  }

  async function runInstall() {
    const update = pendingUpdate.value
    if (!update || busy.value) return
    busy.value = true
    phase.value = 'downloading'
    downloadPercent.value = 0
    downloadIndeterminate.value = false
    statusText.value = ''
    let total = 0
    let downloaded = 0
    try {
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            total = event.data.contentLength ?? 0
            downloadIndeterminate.value = total <= 0
            statusText.value =
              total > 0 ? '正在下载更新…' : '正在下载更新（未获知总大小，进度以已接收数据为准）…'
            break
          case 'Progress':
            downloaded += event.data.chunkLength
            if (total > 0) {
              downloadPercent.value = Math.min(100, Math.round((downloaded / total) * 100))
              statusText.value = `已下载 ${downloadPercent.value}%`
            } else {
              statusText.value = `已接收 ${(downloaded / 1024 / 1024).toFixed(2)} MB`
            }
            break
          case 'Finished':
            downloadPercent.value = 100
            statusText.value = '安装完成，即将重启…'
            break
          default:
            break
        }
      })
      phase.value = 'installing'
      await relaunch()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      errorText.value = `更新失败：${msg}`
      phase.value = 'error'
      console.error('[updater] download/install failed', e)
    } finally {
      busy.value = false
    }
  }

  return {
    phase,
    currentVersion,
    busy,
    downloadPercent,
    downloadIndeterminate,
    statusText,
    errorText,
    pendingUpdate,
    reset,
    loadCurrentVersion,
    runCheck,
    runInstall,
  }
}
