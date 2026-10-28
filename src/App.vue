<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Moon, Sun } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef } from 'vue'

import { runningInBrowser, usingMock } from './api'
import DetailPanel from './components/shell/DetailPanel.vue'
import IconRail from './components/shell/IconRail.vue'
import SideNav from './components/shell/SideNav.vue'
import TitleBar from './components/shell/TitleBar.vue'
import { useLayout } from './composables/useLayout'
import { VIEW_TITLES, useShell } from './composables/useShell'
import { useTheme } from './composables/useTheme'

const { view } = useShell()
const theme = useTheme()
const root = useTemplateRef<HTMLDivElement>('root')
const { cols } = useLayout(() => root.value)

const title = computed(() => VIEW_TITLES[view.value])
const hint = ref<string | null>(null)
let unlisten: (() => void) | null = null

onMounted(async () => {
  await theme.load()

  if (!usingMock) {
    const { listen } = await import('@tauri-apps/api/event')
    const stop = await listen<string>('app://hint', (e) => showHint(e.payload))
    unlisten = () => stop()
  }

  // 窗口先以 visible:false 创建，首帧后才亮出来 —— 白屏一闪是「看起来像网页」的头号来源（§8.6）
  if (!usingMock) await getCurrentWindow().show()
})

onBeforeUnmount(() => unlisten?.())

let hintTimer = 0
function showHint(text: string) {
  hint.value = text
  window.clearTimeout(hintTimer)
  hintTimer = window.setTimeout(() => (hint.value = null), 6000)
}
</script>

<template>
  <div ref="root" class="y-win">
    <TitleBar />

    <div class="y-shell" :data-cols="cols">
      <SideNav v-if="cols !== 'narrow'" />
      <IconRail v-else />

      <main class="y-ctr">
        <header class="y-head">
          <h1>{{ title }}</h1>
          <div class="y-acts">
            <button
              type="button"
              class="iconbtn"
              :title="theme.resolved.value === 'dark' ? '切到亮色' : '切到暗色'"
              @click="theme.toggleMode()"
            >
              <Sun v-if="theme.resolved.value === 'dark'" :size="15" />
              <Moon v-else :size="15" />
            </button>
          </div>
        </header>

        <p v-if="hint" class="y-hint flash">{{ hint }}</p>

        <div class="y-empty">
          {{ runningInBrowser ? '浏览器预览（UI 验证用）· ' : '' }}骨架已就位：窗口、托盘、数据目录与迁移都通了，视图内容按里程碑逐个上。
        </div>
      </main>

      <DetailPanel v-if="cols === 'wide'" />
      <DetailPanel v-else-if="cols === 'mid'" class="y-drawer" />
    </div>
  </div>
</template>

<style scoped>
.y-hint {
  padding: 9px 12px;
  border-radius: var(--r-sm);
  background: var(--card);
  border: 1px solid var(--line-2);
  color: var(--t2);
  font-size: 12px;
  flex: none;
}
</style>
