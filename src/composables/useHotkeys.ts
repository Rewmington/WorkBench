import { computed, ref } from 'vue'

import { api } from '../api'
import type { HotkeyItem } from '../api/types'
import { captureCombo, clashWithInApp, displayCombo } from '../lib/combo'

/**
 * 全局快捷键与开机自启的读模型。
 *
 * 「改绑」这件事的真相只有内核知道：Windows 没有查询「这个组合键归谁」的 API，
 * 所以判据是注册成没成功。前端因此不自己维护一份乐观状态 ——
 * 失败时 Rust 侧已经把旧绑定挂回去了，这里重新读一遍就是对的。
 */
const ACTION = 'show'

const items = ref<HotkeyItem[]>([])
const autostart = ref<boolean | null>(null)
const capturing = ref(false)
const busy = ref(false)
/**
 * 一行内联提示。不弹窗：§2 原则 5，说清楚就够了。
 * `bad` = 这件事没成（注册失败、自启写不进去）；`warn` = 成了但有个要注意的。
 */
const notice = ref<{ text: string; tone: 'bad' | 'warn' } | null>(null)

function fail(e: unknown) {
  notice.value = { text: e instanceof Error ? e.message : String(e), tone: 'bad' }
}

export function useHotkeys() {
  const show = computed(() => items.value.find((i) => i.action === ACTION) ?? null)
  const combo = computed(() => show.value?.combo ?? '')
  const display = computed(() => (combo.value ? displayCombo(combo.value) : ''))
  /** 存了但没挂上：多半是被别的程序抢了，这一格必须看得出来。 */
  const idle = computed(() => show.value !== null && !show.value.registered)

  async function load() {
    const [list, start] = await Promise.all([api.hotkeysList(), api.autostartGet()])
    items.value = list
    autostart.value = start
  }

  async function apply(combo: string) {
    busy.value = true
    try {
      const clash = clashWithInApp(combo)
      const item = await api.hotkeysSet(ACTION, combo)
      items.value = items.value.map((i) => (i.action === ACTION ? item : i))
      notice.value = clash
        ? { text: `已绑上，但它同时也是${clash}的快捷键，两个会一起触发。`, tone: 'warn' }
        : null
    } catch (e) {
      // 旧绑定还在，所以只说这一句，不去动 items
      fail(e)
      await load().catch(() => null)
    } finally {
      busy.value = false
    }
  }

  /** 捕获一次按键。返回 true 表示这一下被当成正经绑定消费掉了。 */
  async function onKey(e: KeyboardEvent): Promise<boolean> {
    if (!capturing.value) return false
    e.preventDefault()
    e.stopPropagation()
    if (e.code === 'Escape') {
      capturing.value = false
      notice.value = null
      return true
    }
    const next = captureCombo(e)
    if (!next) return true
    capturing.value = false
    if (next === combo.value) {
      notice.value = null
      return true
    }
    await apply(next)
    return true
  }

  function startCapture() {
    capturing.value = true
    notice.value = null
  }

  function stopCapture() {
    capturing.value = false
  }

  async function reset() {
    busy.value = true
    try {
      const item = await api.hotkeysReset(ACTION)
      items.value = items.value.map((i) => (i.action === ACTION ? item : i))
      notice.value = null
    } catch (e) {
      fail(e)
    } finally {
      busy.value = false
    }
  }

  async function setAutostart(enabled: boolean) {
    busy.value = true
    notice.value = null
    try {
      autostart.value = await api.autostartSet(enabled)
    } catch (e) {
      fail(e)
      await load().catch(() => null)
    } finally {
      busy.value = false
    }
  }

  return {
    items,
    show,
    combo,
    display,
    idle,
    capturing,
    busy,
    notice,
    autostart,
    load,
    onKey,
    startCapture,
    stopCapture,
    reset,
    setAutostart,
  }
}
