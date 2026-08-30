import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from '@unocss/vite'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
/** workspace 包：显式指向源码，避免部分环境下 node_modules 未链接导致解析失败 */
const sharedRoot = path.resolve(__dirname, '../../packages/shared/src')

const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [vue(), UnoCSS()],
  // 强制 vue 单例，防止 bun/pnpm 混合安装导致多份 Vue 实例
  resolve: {
    alias: {
      '@chattake/shared': sharedRoot,
    },
    dedupe: ['vue'],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          const naive = id.match(/\/node_modules\/naive-ui\/es\/([^/]+)/)
          if (naive) {
            const core = new Set(['config-provider', 'modal', 'drawer', 'dialog', 'popover', 'button', 'button-group', 'empty', 'tag', 'tree', 'tree-select'])
            return naive[1].startsWith('_') || core.has(naive[1]) ? 'vendor-naive-core' : 'vendor-naive-components'
          }
          if (id.includes('/node_modules/highlight.js/') || id.includes('/node_modules/marked/')) return 'vendor-markdown'
          if (id.includes('/node_modules/vue/') || id.includes('/node_modules/vue-router/') || id.includes('/node_modules/pinia/')) return 'vendor-vue'
        },
      },
    },
  },
})
