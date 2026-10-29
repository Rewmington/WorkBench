<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'

import type { ThemeAccent, ThemeMode, ThemePreset } from '../api/types'
import Segmented from '../components/ui/Segmented.vue'
import { useHotkeys } from '../composables/useHotkeys'
import { useTheme } from '../composables/useTheme'

/**
 * 设置页 v1 只放**已经在生效**的开关：主题三轴、降低动效、快捷键、开机自启、数据位置说明。
 *
 * 不放没实现的东西（自动更新、扩展市场这些 PLAN §4 明确不做），
 * 也不放「同步」—— 一台电脑一个写端，v1 没有同步可设（PLAN §10）。
 */
const theme = useTheme()
const hk = useHotkeys()

const chipClass = computed(() => ({ cap: hk.capturing.value, warn: hk.idle.value }))
const chipText = computed(() =>
  hk.capturing.value ? '按下新组合键 · Esc 取消' : hk.display.value || '—',
)
function toggleCapture() {
  if (hk.capturing.value) hk.stopCapture()
  else hk.startCapture()
}

const MODES: { value: ThemeMode; label: string }[] = [
  { value: 'auto', label: '跟随系统' },
  { value: 'dark', label: '暗色' },
  { value: 'light', label: '亮色' },
]
const PRESETS: { value: ThemePreset; label: string }[] = [
  { value: 'graphite', label: '石墨' },
  { value: 'moss', label: '苔绿' },
  { value: 'ink', label: '墨黑' },
]
const ACCENTS: { value: ThemeAccent; label: string }[] = [
  { value: 'green', label: '绿' },
  { value: 'blue', label: '蓝' },
  { value: 'amber', label: '琥珀' },
]

// 挂在 document 的捕获阶段：App.vue 在 window 上冒泡听 Ctrl+K，而键盘事件的路径是
// document → 目标元素 → window，只有先跑到的这一层能拦住它，
// 否则「正在录快捷键」的同时命令面板也会跟着弹出来。
function onKeydown(e: KeyboardEvent) {
  void hk.onKey(e)
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown, true)
  void hk.load()
})
onUnmounted(() => document.removeEventListener('keydown', onKeydown, true))
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>外观与动效 <span class="sub">三轴独立 · 当前解析为 {{ theme.resolved.value }}</span></h2>
    </div>

    <div class="blk">
      <h3>外观</h3>
      <div class="row">
        <span class="k">模式</span>
        <Segmented :options="MODES" :model-value="theme.mode.value" @update:model-value="theme.mode.value = $event" />
      </div>
      <div class="row">
        <span class="k">明度预设</span>
        <Segmented :options="PRESETS" :model-value="theme.preset.value" @update:model-value="theme.preset.value = $event" />
      </div>
      <div class="row">
        <span class="k">强调色</span>
        <Segmented :options="ACCENTS" :model-value="theme.accent.value" @update:model-value="theme.accent.value = $event" />
      </div>
      <p class="note">
        三轴各自独立：预设改的是底与卡的明度阶梯，强调色只改 <code>--accent</code> 一族。
        亮色下强调色自动降到 <code>#059669</code>，绿底白字那种对比度不够的写法不会出现。
      </p>
    </div>

    <div class="blk">
      <h3>动效</h3>
      <label class="row chk">
        <input v-model="theme.reducedMotion.value" type="checkbox" />
        <span class="k">降低动效</span>
        <span class="v">卡片环立刻停止巡航（不是只暂停 CSS 动画）</span>
      </label>
    </div>

    <div class="blk">
      <h3>快捷键与启动</h3>
      <div class="row">
        <span class="k">唤起窗口</span>
        <button class="combo" :class="chipClass" :disabled="hk.busy.value" type="button" @click="toggleCapture">
          {{ chipText }}
        </button>
        <button class="mini" :disabled="hk.busy.value" type="button" @click="hk.reset()">恢复默认</button>
        <span v-if="hk.idle.value" class="v warn">! 未挂上</span>
      </div>
      <p v-if="hk.idle.value" class="note warn">
        <span class="mark">!</span>
        「{{ hk.combo.value }}」这条绑定存着但没生效 —— 十有八九是被别的程序先占了。
        换个组合键就能抢回来，旧的那条不会丢。
      </p>
      <p v-if="hk.notice.value" class="note" :class="hk.notice.value.tone">
        <span class="mark">{{ hk.notice.value.tone === 'bad' ? '✕' : '!' }}</span>
        {{ hk.notice.value.text }}
      </p>
      <label class="row chk">
        <input
          type="checkbox"
          :checked="hk.autostart.value === true"
          :disabled="hk.busy.value || hk.autostart.value === null"
          @change="hk.setAutostart(($event.target as HTMLInputElement).checked)"
        />
        <span class="k">开机自启</span>
        <span class="v">写进系统启动项，只动这台电脑</span>
      </label>
      <p class="note">
        自启记的是当前可执行文件的路径。开发模式下那是 <code>src-tauri\target\debug\anvil.exe</code>，
        所以这条建议等装上正式版之后再打开。
      </p>
    </div>

    <div class="blk">
      <h3>数据位置</h3>
      <p class="note">
        库文件、图标缓存与备份快照的绝对路径原样写在备份页与侧栏底部 ——
        「数据只在这台机器上」这句话，要能被看见才算数。
      </p>
    </div>
  </section>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
h2 {
  font-size: 13.5px;
  font-weight: 800;
}
.sub {
  font-size: 11px;
  font-weight: 500;
  color: var(--t3);
}
.blk {
  display: flex;
  flex-direction: column;
  gap: 9px;
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: var(--r-sm);
  padding: 11px 12px;
}
.blk h3 {
  margin: 0;
  font-size: 12px;
  font-weight: 750;
  color: var(--t2);
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.row .k {
  font-size: 11.5px;
  color: var(--t3);
  width: 74px;
  flex: none;
}
.row .v {
  font-size: 11px;
  color: var(--t3);
}
.chk {
  cursor: pointer;
}
.chk input {
  accent-color: var(--accent);
}
.note {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.65;
  color: var(--t3);
}
.note code {
  font: 500 11px/1.5 ui-monospace, Consolas, monospace;
  color: var(--t2);
}
/* 组合键芯片：等宽、有边框、不带 accent —— 它显示的是一个值，不是一个动作 */
.combo {
  min-width: 132px;
  padding: 5px 9px;
  border-radius: var(--r-xs);
  border: 1px solid var(--line-2);
  background: var(--card-2);
  color: var(--t1);
  font: 600 11.5px/1.3 ui-monospace, Consolas, monospace;
  letter-spacing: 0.02em;
  text-align: left;
}
.combo.cap {
  border-color: var(--amber);
  border-style: dashed;
  color: var(--t2);
}
.combo.warn {
  border-color: var(--amber);
  color: var(--amber);
}
.combo:disabled {
  opacity: 0.6;
  cursor: default;
}
.mini {
  padding: 5px 8px;
  border-radius: var(--r-xs);
  border: 1px solid var(--line);
  color: var(--t3);
  font-size: 11px;
  font-weight: 600;
}
.mini:hover {
  border-color: var(--line-2);
  color: var(--t2);
}
/* 状态不只靠颜色（§2 原则 3）：✕ / ! 前缀 + 边框 + 文字三处一起说同一件事 */
.v.warn {
  color: var(--amber);
  font-weight: 700;
}
.note.warn {
  color: var(--t2);
}
.note.bad {
  color: var(--red);
  font-weight: 650;
}
.mark {
  font-weight: 800;
  color: var(--amber);
  margin-right: 3px;
}
.note.bad .mark {
  color: var(--red);
}
</style>
