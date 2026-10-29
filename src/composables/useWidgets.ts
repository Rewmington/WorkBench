import { ref } from 'vue'

import { api } from '../api'
import type { Countdown, CountdownMode, Sticky, SysLoad } from '../api/types'

/**
 * 「进行」视图三个部件的读模型：便签、倒计时、系统资源。
 *
 * 便签走 600 ms 防抖（PLAN §5.2），并且**只在内容真的变了时才写**：
 * 防抖会多发一次同样的落盘，而 Rust 侧对相同内容不推 rev —— 前端跟着少发一次，
 * 「最后写入时间」才不会被打字动作污染。
 */
const stickies = ref<Sticky[]>([])
const countdowns = ref<Countdown[]>([])
const load = ref<SysLoad | null>(null)

const DEBOUNCE = 600
const pending = new Map<number, string>()
const timers = new Map<number, number>()

function contentOf(slot: number): string {
  return stickies.value.find((s) => s.slot === slot)?.content ?? ''
}

export function useWidgets() {
  async function loadAll() {
    const [s, c] = await Promise.all([api.stickiesList(), api.countdownsList()])
    stickies.value = s
    countdowns.value = c
  }

  async function flushSlot(slot: number) {
    const next = pending.get(slot)
    if (next === undefined) return
    pending.delete(slot)
    const timer = timers.get(slot)
    if (timer !== undefined) {
      window.clearTimeout(timer)
      timers.delete(slot)
    }
    if (next === contentOf(slot)) return
    const saved = await api.stickySave(slot, next)
    const at = stickies.value.findIndex((s) => s.slot === slot)
    if (at >= 0) stickies.value[at] = saved
    else stickies.value = [...stickies.value, saved].sort((a, b) => a.slot - b.slot)
  }

  /** 切视图、窗口失焦、收进托盘之前都要调它：防抖窗口里的字不该丢。 */
  async function flushAll() {
    await Promise.all([...pending.keys()].map((slot) => flushSlot(slot)))
  }

  function edit(slot: number, value: string) {
    pending.set(slot, value)
    const existing = timers.get(slot)
    if (existing !== undefined) window.clearTimeout(existing)
    timers.set(
      slot,
      window.setTimeout(() => void flushSlot(slot), DEBOUNCE),
    )
  }

  /** 立刻落盘并返回该槽位当前的文本（编辑器要拿它当受控值的基线）。 */
  async function saveNow(slot: number, value: string) {
    edit(slot, value)
    await flushSlot(slot)
  }

  async function clearSlot(slot: number) {
    pending.delete(slot)
    await api.stickyClear(slot)
    stickies.value = stickies.value.filter((s) => s.slot !== slot)
  }

  // ---- 倒计时 ----

  function put(cd: Countdown) {
    const at = countdowns.value.findIndex((c) => c.id === cd.id)
    if (at >= 0) countdowns.value[at] = cd
    else countdowns.value = [...countdowns.value, cd]
  }

  async function addCountdown(label: string, mode: CountdownMode, spec: Countdown['spec']) {
    put(await api.countdownAdd(label, mode, spec))
  }
  async function start(id: string) {
    put(await api.countdownStart(id))
  }
  async function pause(id: string) {
    put(await api.countdownPause(id))
  }
  async function resume(id: string) {
    put(await api.countdownResume(id))
  }
  async function reset(id: string) {
    put(await api.countdownReset(id))
  }
  async function remove(id: string) {
    await api.countdownDelete(id)
    countdowns.value = countdowns.value.filter((c) => c.id !== id)
  }

  /** 系统资源每 1 秒跟一次：CPU 是两次采样作差，采样间隔就是它的时间分辨率。 */
  async function pollLoad() {
    load.value = await api.sysLoad()
  }

  return {
    stickies,
    countdowns,
    load,
    loadAll,
    edit,
    saveNow,
    clearSlot,
    flushAll,
    addCountdown,
    start,
    pause,
    resume,
    reset,
    remove,
    pollLoad,
  }
}
