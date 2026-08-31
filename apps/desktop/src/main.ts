import { createApp } from 'vue'
import { createPinia } from 'pinia'
import 'virtual:uno.css'
import App from './App.vue'
import router from './router'
import { useUiStore } from './stores/ui'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)
app.use(router)

const ui = useUiStore()
router.afterEach((to) => {
  if (to.name === 'library') {
    ui.activeTab = 'library'
  } else if (to.name === 'sessions') {
    ui.activeTab = 'sessions'
  }
})

app.mount('#app')
