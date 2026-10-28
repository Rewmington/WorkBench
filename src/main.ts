import { createApp } from 'vue'

// Inter 随包（本地 woff2 子集）：走 CDN 会离线即破，也会多一条 CSP 例外（§8.6）
import '@fontsource-variable/inter/standard.css'
import './style.css'

import App from './App.vue'
import { api } from './api'

// 右键一律走 Win32 原生菜单，浏览器菜单不能出现（§8.6）
document.addEventListener('contextmenu', (event) => {
  event.preventDefault()
  const target = event.target as HTMLElement | null
  const inField = !!target?.closest('input, textarea, [contenteditable="true"], .md-preview')
  void api.contextMenu(inField ? 'input' : 'shell')
})

createApp(App).mount('#app')
