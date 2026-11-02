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
import { useLinked } from './composables/useLinked'
import { VIEW_TITLES, useShell } from './composables/useShell'
import { useTodos } from './composables/useTodos'
import { usePalette } from './composables/usePalette'
import { toggleMode, useTheme } from './composables/useTheme'
import { errText } from './lib/err'
import Backup from './views/Backup.vue'
import Blog from './views/Blog.vue'
import Calendar from './views/Calendar.vue'
import Dashboard from './views/Dashboard.vue'
import Launcher from './views/Launcher.vue'
import Medicine from './views/Medicine.vue'
import Notes from './views/Notes.vue'
import Progress from './views/Progress.vue'
import Settings from './views/Settings.vue'
import Stats from './views/Stats.vue'
import Timetable from './views/Timetable.vue'
import Todos from './views/Todos.vue'

const shell = useShell()
const palette = usePalette()
const theme = useTheme()
const todosModel = useTodos()
const linked = useLinked()
const root = useTemplateRef<HTMLDivElement>('root')
const { cols } = useLayout(() => root.value)

const title = computed(() => VIEW_TITLES[shell.view.value])
const hint = ref<string | null>(null)
let unlisten: (() => void) | null = null
let linkedTimer = 0

onMounted(async () => {
  startClock()
  // 窗口在配置里是 `visible: false`，等前端就绪再 show（免闪白底）。所以这三条里任何一条抛错
  // 都必须照样把窗口唤出来：以前 `settingsAll` 一坏就整段 onMounted 中断，表现是"双击图标没反应、
  // 进程却在托盘里跑"——最难查的那种坏法，而它恰好是 P1-3/P1-14 那一类"读不动"的故障。
  let bootError: unknown = null
  try {
    await theme.load()
    await todosModel.refresh()
    await linked.load()
  } catch (e) {
    bootError = e
  }

  window.addEventListener('keydown', onGlobalKey)

  if (!usingMock) {
    const { listen } = await import('@tauri-apps/api/event')
    const stopHint = await listen<string>('app://hint', (e) => showHint(e.payload))
    // 通知中心那条提醒点进来：切到待办、选中这一条，而不是只把窗口唤到前台
    const stopReveal = await listen<string>('app://reveal', (e) => revealTodo(e.payload))
    // 托盘与右键菜单里 Rust 不自己处理的那些项，都以 id 走这条通道过来。
    // 以前**没人听**：右键「复制选中内容」点了永远没反应，托盘「设置」只唤窗不切页。
    const stopMenu = await listen<string>('menu://click', (e) => onMenuClick(e.payload))
    unlisten = () => {
      stopHint()
      stopReveal()
      stopMenu()
    }
    await getCurrentWindow().show()
  }

  if (bootError !== null) showHint(`界面数据没读上，先按空数据开着：${errText(bootError)}`)

  // 外部源在后台刷：站点首包 8 秒，把它放进渲染路径就是让界面转圈
  void linked.refreshBlog()
  void linked.refreshMedicine()
  linkedTimer = window.setInterval(() => {
    void linked.refreshMedicine()
    void linked.refreshBlog()
  }, 10 * 60_000)
})

// 侧栏徽章只从这一份派生：视图各算各的会出现「列表 3 条、徽章 5 条」这种最难查的对不上
watch(
  () => todosModel.undone.value.length,
  (undone) => shell.setCount('todos', undone),
)
watch(
  () => linked.liveBlog.value.length,
  (n) => shell.setCount('blog', n || null),
)
watch(
  () => linked.medicine.value?.count ?? 0,
  (n) => shell.setCount('medicine', n || null),
)

onBeforeUnmount(() => {
  unlisten?.()
  stopClock()
  window.clearInterval(linkedTimer)
  window.removeEventListener('keydown', onGlobalKey)
})

function onGlobalKey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    palette.toggle()
  }
}

/** 提醒通知点进来的落点：视图切过去、这条被选中，详情面板直接是它。 */
function revealTodo(id: string) {
  shell.go('todos')
  shell.select({ kind: 'todo', id })
}

/**
 * 托盘菜单与右键菜单里，Rust 那边不自己处理的项都以 id 走 `menu://click` 过来
 * （`src-tauri/src/ui/menus.rs` 的 `other` 分支）。
 *
 * 这条通道以前没有监听方，所以两个功能一次都没生效过：右键「复制选中内容」点了没反应，
 * 托盘「设置」只把窗口唤到前台、不切页。它和 N1（`open_url` 参数名对不上）是同一类 ——
 * 跨语言的字符串协议，三层检查谁都不看，只有真点一次才暴露。现在 `npm run contract` 也比对事件名。
 */
function onMenuClick(id: string) {
  if (id === 'settings') {
    shell.go('settings')
    return
  }
  if (id !== 'copy-selection') return
  const text = window.getSelection()?.toString() ?? ''
  if (!text) {
    showHint('没有选中内容')
    return
  }
  // 不用 navigator.clipboard：应用地址是 `http://tauri.localhost`，不是可信来源，
  // 那道 API 在部分 WebView2 版本上是 undefined。execCommand 复制当前选区，不要权限。
  document.execCommand('copy')
  showHint(`已复制这 ${text.length} 个字`)
}

let hintTimer = 0
function showHint(text: string) {  hint.value = text
  window.clearTimeout(hintTimer)
  hintTimer = window.setTimeout(() => (hint.value = null), 6000)
}
</script>

<template>
  <div ref="root" class="y-win">
    <TitleBar />

    <div class="y-shell" :data-cols="cols" :data-sel="shell.selection.value ? '1' : '0'">
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

        <Dashboard v-if="shell.view.value === 'dashboard'" />
        <Progress v-else-if="shell.view.value === 'progress'" />
        <Notes v-else-if="shell.view.value === 'notes'" />
        <Stats v-else-if="shell.view.value === 'stats'" />
        <Settings v-else-if="shell.view.value === 'settings'" />
        <Todos v-else-if="shell.view.value === 'todos'" />
        <Launcher v-else-if="shell.view.value === 'launcher'" />
        <Calendar v-else-if="shell.view.value === 'calendar'" />
        <Blog v-else-if="shell.view.value === 'blog'" />
        <Medicine v-else-if="shell.view.value === 'medicine'" />
        <Timetable v-else-if="shell.view.value === 'timetable'" />
        <Backup v-else-if="shell.view.value === 'backup'" />
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
