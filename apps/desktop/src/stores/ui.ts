import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

/** 主导航 Tab：对话记录 | 知识库 */
export type MainTab = 'sessions' | 'library'

/**
 * 全局 UI 状态：Tab、侧栏宽度、暗色主题
 */
export const useUiStore = defineStore('ui', () => {
  const activeTab = ref<MainTab>('sessions')
  const sidebarWidth = ref(280)
  const darkMode = ref(localStorage.getItem('chattake:theme') === 'dark'
    || (!localStorage.getItem('chattake:theme') && matchMedia('(prefers-color-scheme: dark)').matches))

  function toggleTheme() {
    darkMode.value = !darkMode.value
  }

  watch(
    darkMode,
    (v) => {
      const root = document.documentElement
      if (v) {
        root.classList.add('dark')
      } else {
        root.classList.remove('dark')
      }
      localStorage.setItem('chattake:theme', v ? 'dark' : 'light')
    },
    { immediate: true },
  )

  return { activeTab, sidebarWidth, darkMode, toggleTheme }
})
