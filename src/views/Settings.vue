<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

import { api } from '../api'
import type { PathsView, ThemeAccent, ThemeMode, ThemePreset } from '../api/types'
import Segmented from '../components/ui/Segmented.vue'
import { useHotkeys } from '../composables/useHotkeys'
import { useTheme } from '../composables/useTheme'

/**
 * 设置页 v1 只放**已经在生效**的开关：主题三轴、降低动效、快捷键、开机自启，
 * 外加两个"说清楚"的块：「关于与隐私」与「数据与诊断」（PLAN §8.9 —— 解释性的话
 * 从各屏搬到这里，操作屏每屏最多留一句能用的提示）。
 */
const theme = useTheme()
const hk = useHotkeys()
const paths = ref<PathsView | null>(null)

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
  void api.settingsAll().then((s) => {
    paths.value = s.paths
  })
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
      <p class="note">三轴独立，每个预设都有对应的亮色版本。</p>
    </div>

    <div class="blk">
      <h3>动效</h3>
      <label class="row chk">
        <input v-model="theme.reducedMotion.value" type="checkbox" />
        <span class="k">降低动效</span>
        <span class="v">卡片环停止巡航</span>
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
        <span class="v">写入系统启动项</span>
      </label>
      <p v-if="hk.autostart.value === true" class="note">自启记的是当前 exe 路径，装正式版后再开。</p>
    </div>

    <div class="blk">
      <h3>关于与隐私</h3>
      <dl class="y-dl">
        <dt>数据在哪</dt>
        <dd>只在本机这一个文件里，见下面「数据与诊断」。坚果云上只有加密快照。</dd>
        <dt>备份会上传</dt>
        <dd>待办、笔记、速达条目、便签、倒计时、课表、主题与界面设置。整库 gzip 后 AES-256-GCM，密钥由「邮箱 + 应用密码」派生 —— 坚果云只见密文。</dd>
        <dt>备份不上传</dt>
        <dd>应用密码（只在本机 Windows 凭据管理器）、药箱的库存与健康数据、绝对路径与邮箱、备份流水。药箱的 <code>device-*.json</code> 在另一个目录，永不读写。</dd>
        <dt>手机同步</dt>
        <dd>手机能改的只有待办：新建、勾选、删除；课表在手机上只读。两边同改一条时整行按「最后写入的那台」走，不做字段拼接。笔记、速达、便签、倒计时都不进同步目录。云上每设备一份 <code>device-*.enc</code>，与备份共用同一组凭据、同一个信封，但独立目录 <code>workbench-sync/</code>，坚果云只见密文。</dd>
        <dt>应用密码</dt>
        <dd>坚果云的应用密码是<b>账号级</b>的：备份与读药箱这两把都能读整个网盘，分开是为了能单独吊销、以及从名字认人，不构成权限隔离。写入的边界靠的是只读客户端 —— 那个类型没有 PUT / MKCOL / DELETE。</dd>
        <dt>密码框为何不回显</dt>
        <dd>它从不进库，所以没有东西可以回显。框里空着 = 沿用已存那条，不是丢了。</dd>
        <dt>课表与教务</dt>
        <dd>导入进来的只读副本。工作台不连教务网站、不碰教务密码；要改课，回教务导出再导一次。</dd>
        <dt>药箱</dt>
        <dd>只读源：把它的库复制一份再只读打开，用完就删；不写它的库，也不写它的 <code>medicine-sync/</code> 目录，更不读它的凭据文件。</dd>
        <dt>遥测</dt>
        <dd>没有。不统计、不上报，联网只发生在你点备份、读药箱云端、刷新博客这三处。</dd>
      </dl>
    </div>

    <div class="blk">
      <h3>数据与诊断</h3>
      <dl v-if="paths" class="y-dl">
        <dt>库文件</dt>
        <dd><code>{{ paths.db }}</code></dd>
        <dt>备份快照</dt>
        <dd><code>{{ paths.backups }}</code></dd>
        <dt>日志</dt>
        <dd><code>{{ paths.logs }}</code></dd>
        <dt>设备 id</dt>
        <dd><code>{{ paths.deviceId }}</code></dd>
        <dt>SQLite 模式</dt>
        <dd>{{ paths.journalMode }}</dd>
      </dl>
      <div class="row">
        <button type="button" class="mini" @click="api.openDataDir()">打开数据目录</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* 滚动契约在全局 `.y-view`（src/style.css）：这一页的内部间距靠各块自己的 margin 撑，
   所以这里不再重复声明 flex/overflow。 */
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
  word-break: break-all;
}
/* 「关于与隐私」与「数据与诊断」共用的一张问答表 */
.y-dl {
  display: grid;
  grid-template-columns: 84px minmax(0, 1fr);
  gap: 8px 10px;
  margin: 0;
}
.y-dl dt {
  font-size: 11px;
  color: var(--t3);
}
.y-dl dd {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--t2);
}
.y-dl dd b {
  color: var(--t1);
}
.y-dl code {
  font: 500 11px/1.5 ui-monospace, Consolas, monospace;
  color: var(--t1);
  word-break: break-all;
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
