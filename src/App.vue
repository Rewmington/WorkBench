<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Moon, Search, Sun } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue'

import { runningInBrowser, usingMock } from './api'
import CommandPalette from './components/palette/CommandPalette.vue'
import DetailPanel from './components/shell/DetailPanel.vue'
import IconRail from './components/shell/IconRail.vue'
import SideNav from './components/shell/SideNav.vue'
import TitleBar from './components/shell/TitleBar.vue'
import { startClock, stopClock } from './composables/useClock'
import { useLayout } from './composables/useLayout'
import { VIEW_TITLES, useShell } from './composables/useShell'
import { useTodos } from './composables/useTodos'
import { usePalette } from './composables/usePalette'
import { toggleMode, useTheme } from './composables/useTheme'
import Calendar from './views/Calendar.vue'
import Launcher from './views/Launcher.vue'
import Todos from './views/Todos.vue'

const shell = useShell()
const palette = usePalette()
const theme = useTheme()
const todosModel = useTodos()
const root = useTemplateRef<HTMLDivElement>('root')
const { cols } = useLayout(() => root.value)

const title = computed(() => VIEW_TITLES[shell.view.value])
const hint = ref<string | null>(null)
let unlisten: (() => void) | null = null

onMounted(async () => {
  startClock()
  await theme.load()
  await todosModel.refresh()

  window.addEventListener('keydown', onGlobalKey)

  if (!usingMock) {
    const { listen } = await import('@tauri-apps/api/event')
    const stop = await listen<string>('app://hint', (e) => showHint(e.payload))
    unlisten = () => stop()
    await getCurrentWindow().show()
  }
})

// 侧栏徽章只从这一份派生：视图各算各的会出现「列表 3 条、徽章 5 条」这种最难查的对不上
watch(
  () => todosModel.undone.value.length,
  (undone) => shell.setCount('todos', undone),
)

onBeforeUnmount(() => {
  unlisten?.()
  stopClock()
  window.removeEventListener('keydown', onGlobalKey)
})

function onGlobalKey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    palette.toggle()
  }
}

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
          <button type="button" class="y-search" @click="palette.show()">
            <Search :size="13" />
            <span>搜索待办、速达、动作…</span>
            <kbd>Ctrl K</kbd>
          </button>
          <div class="y-acts">
            <button
              type="button"
              class="iconbtn"
              :title="theme.resolved.value === 'dark' ? '切到亮色' : '切到暗色'"
              @click="toggleMode()"
            >
              <Sun v-if="theme.resolved.value === 'dark'" :size="15" />
              <Moon v-else :size="15" />
            </button>
          </div>
        </header>

        <p v-if="hint" class="y-hint flash">{{ hint }}</p>

        <Todos v-if="shell.view.value === 'todos'" />
        <Launcher v-else-if="shell.view.value === 'launcher'" />
        <Calendar v-else-if="shell.view.value === 'calendar'" />
        <div v-else class="y-empty">
          {{ runningInBrowser ? '浏览器预览（UI 验证用）· ' : '' }}「{{ title }}」要在后续里程碑接上；待办、速达、日历现在就能用，
          <button type="button" class="inline-link" @click="palette.show()">命令面板</button>也已经通了。
        </div>
      </main>

      <DetailPanel v-if="cols === 'wide'" />
      <DetailPanel v-else-if="shell.detailOpen.value" class="y-drawer" />
    </div>

    <CommandPalette />
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
.inline-link {
  color: var(--accent);
  font-weight: 700;
  text-decoration: underline;
  text-underline-offset: 2px;
}
</style>
