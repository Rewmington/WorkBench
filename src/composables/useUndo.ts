import { ref } from 'vue'

/**
 * 删除可撤销（PLAN §4）。撤销条 8 秒后自己收，不打断手上的活；
 * Enter 绑定在条上，所以手不用碰鼠标（§8.6 最后一条）。
 */
export interface UndoEntry {
  label: string
  run: () => Promise<void> | void
}

const entry = ref<UndoEntry | null>(null)
let timer = 0

export function useUndo() {
  function offer(next: UndoEntry) {
    entry.value = next
    window.clearTimeout(timer)
    timer = window.setTimeout(() => (entry.value = null), 8000)
  }

  async function trigger() {
    const current = entry.value
    entry.value = null
    window.clearTimeout(timer)
    await current?.run()
  }

  function dismiss() {
    entry.value = null
    window.clearTimeout(timer)
  }

  return { entry, offer, trigger, dismiss }
}
