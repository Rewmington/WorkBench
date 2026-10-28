import { computed, ref } from 'vue'

/**
 * 全应用唯一的时间源：1 秒 tick 同时驱动倒计时、"逾期 2 小时"、服药是否过期、
 * 以及所有"最后成功于 X 分钟前"（PLAN §4）。
 *
 * 组件不许自己 `Date.now()`（lint-style 拦得住），只能把这里的 now 传进 lib/reltime.ts。
 * 这也是「改系统时间后一切跟着变」能成立的原因：每秒重读系统钟，中间没有任何缓存。
 */
const nowMs = ref(Date.now())
let interval = 0

function tick() {
  nowMs.value = Date.now()
}

function onVisible() {
  // 窗口从托盘/最小化回来时先补一帧，否则用户会看到停在旧秒数上
  if (!document.hidden) tick()
}

export function startClock() {
  if (interval) return
  tick()
  interval = window.setInterval(tick, 1000)
  document.addEventListener('visibilitychange', onVisible)
  window.addEventListener('focus', tick)
}

export function stopClock() {
  if (!interval) return
  window.clearInterval(interval)
  interval = 0
  document.removeEventListener('visibilitychange', onVisible)
  window.removeEventListener('focus', tick)
}

export function useClock() {
  return { now: computed(() => nowMs.value) }
}
