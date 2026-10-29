<script setup lang="ts">
import { Maximize } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { usingMock } from '../../api'
import type { Todo } from '../../api/types'
import { useClock } from '../../composables/useClock'
import { useShell } from '../../composables/useShell'
import { useTheme } from '../../composables/useTheme'
import { dueLabel } from '../../lib/reltime'
import RingStrip from './RingStrip.vue'

/**
 * 3D 卡片环（PLAN §8.8.1）。参考图那一版用 CSS keyframes 转，这里故意不照抄：
 *
 * keyframe 占着 `transform` 就没法在拖动时改角度，而 §8.8.1 明说「CSS 暂停不算真停」。
 * 所以角度归 rAF 所有，`live` 一落假就 `cancelAnimationFrame` —— 这是失焦/最小化/
 * 降低动效三种情况巡航真的停下来（GPU 曲线为证）的唯一可靠写法。
 *
 * 动画只碰 `transform`：环与其卡上不许出现 filter / backdrop-filter / box-shadow 过渡，
 * 那三样都触发重绘，正是参考项目把 GPU 吃到 26% 的来源。
 */
const props = defineProps<{ cards: Todo[] }>()

const shell = useShell()
const theme = useTheme()
const { now } = useClock()

/** 一圈最多 8 张，超过就挤成一叠，不如退化成列表（降级方向同 §8.8.1）。 */
const CARD_W = 128
const CRUISE_MS = 26_000
const SNAP_MS = 160
/**
 * 参考图是 6 面。少于 5 面时相邻卡的角度差 ≥ 72°，配上 backface-visibility
 * 后同时只剩一张正对镜头 —— 那不叫环，正是 §8.8.1 要避免的「空转的圈」，
 * 只是它发生在 3–4 面这一档，所以把降级门槛从 §8.8.1 的 3 提到 5。
 */
const MIN_FACES = 5

const ring = ref<HTMLElement | null>(null)
const box = ref<HTMLElement | null>(null)
const angle = ref(0)
const dragging = ref(false)
const windowFocused = ref(true)
const pageVisible = ref(true)

let frame = 0
let lastTick = 0
let snapFrame = 0
let unlistenFocus: (() => void) | null = null

const n = computed(() => props.cards.length)
const step = computed(() => (n.value > 0 ? 360 / n.value : 0))
/**
 * 半径按面数算，保证相邻卡片边缘刚好不相撞：
 * tz = (cardW / 2) / tan(π / n)，再夹一个下限，面数少的时候不至于贴到圆心。
 */
const radius = computed(() => {
  if (n.value <= 1) return 210
  const need = CARD_W / 2 / Math.tan(Math.PI / n.value)
  return Math.max(210, Math.ceil(need))
})

const live = computed(
  () =>
    n.value >= MIN_FACES &&
    pageVisible.value &&
    windowFocused.value &&
    !theme.reducedMotion.value &&
    !dragging.value,
)

/** 正对镜头的那一面。环的 `.hot` 是这一屏唯一的 accent 实心，所以只允许一张。 */
const front = computed(() => {
  if (n.value === 0) return -1
  const raw = Math.round(-mod(angle.value, 360) / step.value)
  return mod(raw, n.value)
})

function mod(x: number, m: number): number {
  return ((x % m) + m) % m
}

function paint() {
  if (ring.value) ring.value.style.transform = `rotateY(${angle.value}deg)`
}

function loop(ts: number) {
  if (!live.value) {
    frame = 0
    lastTick = 0
    return
  }
  if (lastTick) angle.value -= ((ts - lastTick) / CRUISE_MS) * 360
  lastTick = ts
  paint()
  frame = requestAnimationFrame(loop)
}

function sync() {
  const want = live.value
  if (want && !frame) {
    lastTick = 0
    frame = requestAnimationFrame(loop)
  } else if (!want && frame) {
    cancelAnimationFrame(frame)
    frame = 0
    lastTick = 0
  }
}

watch(live, sync)
watch(live, (v) => box.value?.setAttribute('data-live', String(v)))

// 面数变了要重画：卡片的静态 transform 依赖半径，角度也该收敛到合法范围
watch([n, radius], () => {
  angle.value = mod(angle.value, 360)
  paint()
})

function onPointerDown(event: PointerEvent) {
  if (n.value < MIN_FACES) return
  dragging.value = true
  box.value?.setPointerCapture(event.pointerId)
  let last = event.clientX
  const move = (e: PointerEvent) => {
    angle.value += (e.clientX - last) * 0.4
    last = e.clientX
    paint()
  }
  const up = (e: PointerEvent) => {
    box.value?.removeEventListener('pointermove', move)
    box.value?.removeEventListener('pointerup', up)
    box.value?.removeEventListener('pointercancel', up)
    snap(e.clientX)
  }
  box.value?.addEventListener('pointermove', move)
  box.value?.addEventListener('pointerup', up)
  box.value?.addEventListener('pointercancel', up)
}

/** 松手 160 ms 吸附到最近的一面。不做惯性、不做弹性 —— 那是不值钱的那 200 行。 */
function snap(_fromX: number) {
  const target = Math.round(angle.value / step.value) * step.value
  const from = angle.value
  const delta = target - from
  const t0 = performance.now()
  const ease = (k: number) => 1 - (1 - k) ** 3

  const run = (ts: number) => {
    const k = Math.min(1, (ts - t0) / SNAP_MS)
    angle.value = from + delta * ease(k)
    paint()
    if (k < 1) snapFrame = requestAnimationFrame(run)
    else {
      snapFrame = 0
      dragging.value = false
    }
  }
  snapFrame = requestAnimationFrame(run)
}

function resetView() {
  if (snapFrame) cancelAnimationFrame(snapFrame)
  dragging.value = false
  angle.value = 0
  paint()
}

function pick(i: number) {
  const card = props.cards[i]
  if (!card) return
  shell.select({ kind: 'todo', id: card.id })
}

function onVisibility() {
  pageVisible.value = document.visibilityState !== 'hidden'
}

onMounted(async () => {
  paint()
  box.value?.setAttribute('data-live', String(live.value))
  document.addEventListener('visibilitychange', onVisibility)

  if (!usingMock) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const stop = await getCurrentWindow().onFocusChanged(({ payload }) => {
      windowFocused.value = payload
    })
    unlistenFocus = () => stop()
  }
  sync()
})

onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibility)
  unlistenFocus?.()
  if (frame) cancelAnimationFrame(frame)
  if (snapFrame) cancelAnimationFrame(snapFrame)
})

function metaOf(t: Todo): string {
  if (t.dueAt != null) return dueLabel(now.value, t.dueAt)
  return t.zone === 'inbox' ? '收件箱' : t.zone
}
</script>

<template>
  <!-- 0 面：既不画环也不画列表，由父级折叠成一行虚线条（§8.3.3 / §8.8.1） -->
  <RingStrip v-if="n > 0 && n < MIN_FACES" :cards="cards" @pick="pick" />

  <div v-else-if="n >= MIN_FACES" ref="box" class="y-ringwrap" @pointerdown="onPointerDown">
    <div class="y-tools">
      <button type="button" class="y-tool" @click.stop="resetView">
        <Maximize :size="12" /> 复位视角
      </button>
      <span class="y-live" :class="{ off: !live }">
        <i></i>{{ live ? '巡航中' : '已停' }}
      </span>
    </div>

    <div class="y-ringbox">
      <!-- 仪表卡只浮在环上：降级成列表后它压住的就是列表第一张卡 -->
      <slot name="overlay" />
      <div ref="ring" class="y-ring">
        <button
          v-for="(c, i) in cards"
          :key="c.id"
          type="button"
          class="y-c"
          :class="{ hot: i === front, on: shell.selection.value?.kind === 'todo' && shell.selection.value.id === c.id }"
          :style="{ transform: `rotateY(${i * step}deg) translateZ(${radius}px)` }"
          @click.stop="pick(i)"
        >
          <span class="id">TODO-{{ c.id.slice(-4).toUpperCase() }}</span>
          <span class="ti">{{ c.title }}</span>
          <span class="ln"></span>
          <span class="mt">{{ metaOf(c) }}</span>
        </button>
      </div>
    </div>

    <p class="y-drag">按住<b>左右拖动</b>换面 · 点卡片在右栏看全貌 · 一圈 <b>{{ n }}</b> 面</p>
  </div>
</template>

<style scoped>
/* 外框与底色归父级的 .y-stage（照参考图的分工），这里只管环本身 */
.y-ringwrap {
  min-width: 0;
  touch-action: none;
  user-select: none;
}
.y-ringbox {
  position: relative;
  height: 268px;
  perspective: 1100px;
  display: grid;
  place-items: center;
  overflow: hidden;
}
.y-ring {
  position: relative;
  width: 128px;
  height: 168px;
  transform-style: preserve-3d;
}
.y-c {
  position: absolute;
  inset: 0;
  border-radius: 11px;
  padding: 11px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  text-align: left;
  background: linear-gradient(165deg, #f7faf8, #cfd8d3);
  color: #16211c;
  /* 静态投影：§8.8.1 禁的是环与其卡上的 box-shadow *过渡*（触发重绘），
     不是投影本身 —— 没有它，亮色下这几张白卡会直接融进背景里。 */
  box-shadow: 0 12px 26px rgba(0, 0, 0, 0.4);
  transform: rotateY(0) translateZ(210px);
  backface-visibility: hidden;
}
.y-c .id {
  font: 700 9px/1 ui-monospace, Consolas, monospace;
  letter-spacing: 0.1em;
  opacity: 0.55;
}
.y-c .ti {
  font-size: 11.5px;
  font-weight: 800;
  margin-top: 12px;
  line-height: 1.35;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
}
.y-c .ln {
  height: 5px;
  border-radius: 3px;
  background: rgba(20, 30, 26, 0.14);
  margin-top: 7px;
  width: 62%;
}
.y-c .mt {
  font-size: 10px;
  opacity: 0.6;
  margin-top: 5px;
}
.y-c.hot {
  background: linear-gradient(165deg, var(--accent), var(--accent-2));
  color: var(--accent-ink);
}
.y-c.hot .ln {
  background: rgba(4, 36, 15, 0.2);
}
.y-c.on {
  outline: 2px solid var(--blue);
  outline-offset: 2px;
}
.y-tools {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  margin-bottom: 4px;
}
.y-tool {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  font-weight: 700;
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid var(--line-2);
  background: var(--card-2);
  color: var(--t2);
}
.y-live {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 10.5px;
  font-weight: 750;
  color: var(--accent);
}
.y-live i {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}
.y-live.off {
  color: var(--amber);
}
.y-drag {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 6px 0 0;
  padding: 8px 10px;
  border-radius: 9px;
  background: var(--card-2);
  border: 1px solid var(--line);
  font-size: 11px;
  color: var(--t3);
}
.y-drag b {
  color: var(--t2);
  font-weight: 700;
}
</style>
