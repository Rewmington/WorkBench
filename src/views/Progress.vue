<script setup lang="ts">
import { Play, Plus, RotateCcw, Zap } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { api } from '../api'
import type { Countdown, Resource } from '../api/types'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import WidgetCard from '../components/widget/WidgetCard.vue'
import { useClock } from '../composables/useClock'
import { useWidgets } from '../composables/useWidgets'
import { countdownLabel } from '../lib/reltime'

/**
 * 「进行」视图（PLAN §4）：倒计时 / 便签 ×2 / 常用速达 / 系统资源。
 *
 * 这四个是从首屏降级下来的部件 —— 候选一的版式里没有 Bento 的位置，
 * 所以它们住在这里，但仍然是「打开就想看一眼」的东西。
 */
const { now } = useClock()
const w = useWidgets()

/** 键名沿用 settings.rs 里已定义的 `ui.widgetOrder`，不再另起一个同义键。 */
const ORDER_KEY = 'ui.widgetOrder'
type WidgetKey = 'countdowns' | 'stickies' | 'pins' | 'system'
const LABELS: Record<WidgetKey, string> = {
  countdowns: '倒计时',
  stickies: '便签',
  pins: '常用速达',
  system: '系统资源',
}
const DEFAULTS: WidgetKey[] = ['countdowns', 'stickies', 'pins', 'system']

const order = ref<WidgetKey[]>([...DEFAULTS])
const hidden = ref<WidgetKey[]>([])

const resources = ref<Resource[]>([])
const pins = computed(() =>
  [...resources.value].sort((a, b) => b.useCount - a.useCount).slice(0, 4),
)

/** 部件顺序与显隐是本机 UX，跟着 settings 走（settings 不参与同步，见 0001 的豁免注释）。 */
async function loadOrder() {
  const saved = await api.settingsGet(ORDER_KEY)
  if (Array.isArray(saved)) {
    const list = saved as { key: WidgetKey; hidden?: boolean }[]
    order.value = DEFAULTS.filter((k) => list.some((v) => v.key === k))
    for (const k of DEFAULTS) if (!order.value.includes(k)) order.value.push(k)
    hidden.value = list.filter((v) => v.hidden).map((v) => v.key)
  }
}

function persist() {
  void api.settingsSet(
    ORDER_KEY,
    order.value.map((key) => ({ key, hidden: hidden.value.includes(key) })),
  )
}

function move(key: WidgetKey, delta: number) {
  const i = order.value.indexOf(key)
  const j = i + delta
  if (i < 0 || j < 0 || j >= order.value.length) return
  const next = [...order.value]
  ;[next[i], next[j]] = [next[j], next[i]]
  order.value = next
  persist()
}

function hide(key: WidgetKey) {
  hidden.value = [...hidden.value, key]
  persist()
}

function restore(key: WidgetKey) {
  hidden.value = hidden.value.filter((k) => k !== key)
  persist()
}

function visible(k: WidgetKey): boolean {
  return order.value.includes(k) && !hidden.value.includes(k)
}

// ---- 倒计时 ----

const draftLabel = ref('')
const draftMinutes = ref('25')
const showDraft = ref(false)

function remaining(c: Countdown): number | null {
  if (c.state === 'idle') return null
  if (c.paused) return c.remainingMs
  return c.endsAt == null ? null : c.endsAt - now.value
}

function stateLabel(c: Countdown): string {
  const left = remaining(c)
  if (c.state === 'idle') return '没开始'
  if (c.state === 'finished') return '时间到'
  if (left == null) return ''
  if (left <= 0) return '时间到'
  return c.paused ? `冻在 ${countdownLabel(left)}` : `剩 ${countdownLabel(left)}`
}

async function addCountdown() {
  const label = draftLabel.value.trim()
  const minutes = Number(draftMinutes.value)
  if (!label || !Number.isFinite(minutes) || minutes <= 0) return
  await w.addCountdown(label, 'duration', { minutes })
  await w.start(w.countdowns.value[w.countdowns.value.length - 1].id)
  draftLabel.value = ''
  showDraft.value = false
}

// ---- 便签 ----

const drafts = ref<Record<number, string>>({})
watch(
  () => w.stickies.value,
  (list) => {
    for (const s of list) if (drafts.value[s.slot] === undefined) drafts.value[s.slot] = s.content
  },
  { immediate: true },
)

function onType(slot: number, el: Event) {
  const value = (el.target as HTMLTextAreaElement).value
  drafts.value = { ...drafts.value, [slot]: value }
  w.edit(slot, value)
}

const slots = [1, 2]

// ---- 系统资源 ----

const memUsedPct = computed(() => {
  const l = w.load.value
  if (!l || !l.memTotal) return null
  return Math.round(((l.memTotal - l.memAvail) / l.memTotal) * 100)
})
const gib = (n: number) => `${(n / 1024 ** 3).toFixed(1)} GB`

let loadTimer = 0
let flushBlur: (() => void) | null = null
let flushVis: (() => void) | null = null

onMounted(async () => {
  await Promise.all([w.loadAll(), api.resourcesList().then((r) => (resources.value = r)), loadOrder()])
  void w.pollLoad()
  loadTimer = window.setInterval(() => void w.pollLoad(), 1000)

  // 防抖窗口里的字不能因为切视图/失焦/收进托盘而丢
  flushBlur = () => void w.flushAll()
  flushVis = () => {
    if (document.visibilityState === 'hidden') void w.flushAll()
  }
  window.addEventListener('blur', flushBlur)
  document.addEventListener('visibilitychange', flushVis)
})

onBeforeUnmount(() => {
  window.clearInterval(loadTimer)
  if (flushBlur) window.removeEventListener('blur', flushBlur)
  if (flushVis) document.removeEventListener('visibilitychange', flushVis)
  void w.flushAll()
})
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>进行 <span class="sub">正在跑的东西 · 部件可折叠与排序</span></h2>
    </div>

    <template v-for="k in order" :key="k">
      <!-- 倒计时 -->
      <WidgetCard
        v-if="k === 'countdowns' && visible('countdowns')"
        title="倒计时"
        :count="w.countdowns.value.length"
        :first="order[0] === 'countdowns'"
        :last="order[order.length - 1] === 'countdowns'"
        @up="move('countdowns', -1)"
        @down="move('countdowns', 1)"
        @hide="hide('countdowns')"
      >
        <template #head>
          <button type="button" class="ib" title="新建倒计时" @click="showDraft = !showDraft">
            <Plus :size="13" />
          </button>
        </template>

        <div v-if="showDraft" class="draft">
          <input v-model="draftLabel" class="txt" placeholder="叫什么" @keydown.enter.prevent="addCountdown" />
          <input v-model="draftMinutes" class="txt min" type="number" min="1" placeholder="分钟" />
          <button type="button" class="y-ghost go" @click="addCountdown">加上</button>
        </div>

        <CollapsedRow v-if="!w.countdowns.value.length" label="倒计时 · 没在跑的东西" detail="点右上角 + 起一个，比如 25 分钟番茄钟" />
        <ul v-else class="list">
          <li v-for="c in w.countdowns.value" :key="c.id" class="it">
            <span class="nm">{{ c.label }}</span>
            <span class="st" :class="{ hot: c.state === 'running' && (remaining(c) ?? 1) > 0, done: c.state === 'finished' }">
              {{ stateLabel(c) }}
            </span>
            <button v-if="c.state === 'idle' || c.state === 'finished'" type="button" class="ib" title="开始" @click="w.start(c.id)">
              <Play :size="12" />
            </button>
            <button v-else-if="c.paused" type="button" class="ib" title="继续" @click="w.resume(c.id)">
              <Play :size="12" />
            </button>
            <button v-else type="button" class="ib" title="暂停（会冻住剩余时间）" @click="w.pause(c.id)">❙❙</button>
            <button type="button" class="ib" title="归零" @click="w.reset(c.id)"><RotateCcw :size="12" /></button>
            <button type="button" class="ib del" title="删除" @click="w.remove(c.id)">×</button>
          </li>
        </ul>
      </WidgetCard>

      <!-- 便签 -->
      <WidgetCard
        v-if="k === 'stickies' && visible('stickies')"
        title="便签"
        :count="w.stickies.value.filter((s) => s.content.trim()).length"
        :first="order[0] === 'stickies'"
        :last="order[order.length - 1] === 'stickies'"
        @up="move('stickies', -1)"
        @down="move('stickies', 1)"
        @hide="hide('stickies')"
      >
        <div class="notes">
          <label v-for="s in slots" :key="s" class="note">
            <span class="nh">槽位 {{ s }}</span>
            <textarea
              class="ta"
              rows="4"
              :value="drafts[s] ?? ''"
              :placeholder="`便签 ${s} · 打字后 0.6 秒自动存`"
              @input="onType(s, $event)"
            />
          </label>
        </div>
      </WidgetCard>

      <!-- 常用速达 -->
      <WidgetCard
        v-if="k === 'pins' && visible('pins')"
        title="常用速达"
        :count="pins.length"
        :first="order[0] === 'pins'"
        :last="order[order.length - 1] === 'pins'"
        @up="move('pins', -1)"
        @down="move('pins', 1)"
        @hide="hide('pins')"
      >
        <CollapsedRow v-if="!pins.length" label="常用速达 · 还没加过条目" detail="去速达视图，把 exe 直接拖进窗口" />
        <div v-else class="tiles">
          <button v-for="r in pins" :key="r.id" type="button" class="tile" @click="api.resourceLaunch(r.id)">
            <Zap :size="13" />
            <span class="tn">{{ r.name }}</span>
            <span class="tu">用过 {{ r.useCount }} 次</span>
          </button>
        </div>
      </WidgetCard>

      <!-- 系统资源 -->
      <WidgetCard
        v-if="k === 'system' && visible('system')"
        title="系统资源"
        :first="order[0] === 'system'"
        :last="order[order.length - 1] === 'system'"
        @up="move('system', -1)"
        @down="move('system', 1)"
        @hide="hide('system')"
      >
        <div class="res">
          <div class="row">
            <span class="k">CPU</span>
            <div class="track"><i :style="{ width: (w.load.value?.cpuPercent ?? 0) + '%' }"></i></div>
            <span class="v">{{ w.load.value?.cpuPercent == null ? '采样中' : w.load.value.cpuPercent.toFixed(0) + '%' }}</span>
          </div>
          <div class="row">
            <span class="k">内存</span>
            <div class="track"><i :style="{ width: (memUsedPct ?? 0) + '%' }"></i></div>
            <span class="v">{{ memUsedPct == null ? '—' : memUsedPct + '%' }}</span>
          </div>
          <p class="foot">
            {{ w.load.value ? `物理内存 ${gib(w.load.value.memTotal - w.load.value.memAvail)} / ${gib(w.load.value.memTotal)} 已用` : '第一次采样要等一秒' }}
          </p>
        </div>
      </WidgetCard>
    </template>

    <CollapsedRow
      v-if="hidden.length"
      :label="`已隐藏的部件 · ${hidden.map((k) => LABELS[k]).join(' · ')}`"
      detail="只是收起来了，数据都还在"
      :retryable="false"
    >
      <template #action>
        <button v-for="k in hidden" :key="k" type="button" class="y-ghost tiny" @click="restore(k)">
          显示 {{ LABELS[k] }}
        </button>
      </template>
    </CollapsedRow>
  </section>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 2px;
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
.list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
}
.it {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 0;
  border-top: 1px solid var(--line);
  font-size: 12.5px;
}
.it:first-child {
  border-top: 0;
}
.it .nm {
  font-weight: 650;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.it .st {
  font-size: 11px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.it .st.hot {
  color: var(--accent);
  font-weight: 700;
}
.it .st.done {
  color: var(--amber);
  font-weight: 700;
}
.ib {
  width: 22px;
  height: 22px;
  border-radius: 6px;
  display: grid;
  place-items: center;
  color: var(--t3);
  font-size: 10px;
  flex: none;
}
.ib:hover {
  background: var(--card-2);
  color: var(--t1);
}
.ib.del:hover {
  color: var(--red);
}
.draft {
  display: flex;
  gap: 6px;
  margin-bottom: 8px;
}
.txt {
  height: 28px;
  padding: 0 9px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line-2);
  background: var(--card-2);
  color: var(--t1);
  font-size: 12px;
  user-select: text;
  min-width: 0;
  flex: 1;
}
.txt.min {
  flex: 0 0 68px;
}
.txt:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}
.go {
  height: 28px;
  width: auto;
  padding: 0 11px;
  font-size: 12px;
}
.notes {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.note {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.nh {
  font-size: 10px;
  color: var(--t3);
  font-weight: 800;
  letter-spacing: 0.06em;
}
.ta {
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--card-2);
  color: var(--t1);
  font: inherit;
  font-size: 12.5px;
  line-height: 1.6;
  padding: 8px 9px;
  resize: vertical;
  user-select: text;
}
.ta:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
}
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 8px;
}
.tile {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
  padding: 9px 10px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  background: var(--card-2);
  color: var(--accent);
  text-align: left;
}
.tile .tn {
  font-size: 12px;
  font-weight: 700;
  color: var(--t1);
}
.tile .tu {
  font-size: 10.5px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.res {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.row {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 11.5px;
}
.row .k {
  width: 30px;
  color: var(--t3);
  font-weight: 700;
}
.row .v {
  width: 46px;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--t2);
}
.track {
  flex: 1;
  height: 5px;
  border-radius: 3px;
  background: rgba(128, 140, 135, 0.2);
  overflow: hidden;
}
.track i {
  display: block;
  height: 100%;
  background: var(--accent);
  transition: width 160ms linear;
}
.foot {
  margin: 0;
  font-size: 10.5px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.y-ghost.tiny {
  height: 24px;
  width: auto;
  padding: 0 9px;
  font-size: 11px;
}
@media (max-width: 720px) {
  .notes {
    grid-template-columns: 1fr;
  }
}
</style>
