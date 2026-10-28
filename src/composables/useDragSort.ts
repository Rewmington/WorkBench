import { ref } from 'vue'

/**
 * 指针拖拽排序。
 *
 * 不用 HTML5 DnD：窗口开了 `dragDropEnabled`（速达要从 Explorer 拖 exe 进来），
 * 两种拖拽在同一个 webview 里会互相吃掉事件 —— 这是 Tauri 的已知取舍，只能自己实现。
 * 落点只在松手时算一次，配合 4px 的位移上限，不做大位移动画（§8.6）。
 */
export interface DropTarget {
  id: string
  prevId: string | null
  nextId: string | null
}

export function useDragSort(
  getContainer: () => HTMLElement | null,
  onDrop: (target: DropTarget) => void,
) {
  const draggingId = ref<string | null>(null)
  const overId = ref<string | null>(null)

  function rows(): { id: string; el: HTMLElement }[] {
    const container = getContainer()
    if (!container) return []
    return [...container.querySelectorAll<HTMLElement>('[data-row-id]')].map((el) => ({
      id: el.dataset.rowId as string,
      el,
    }))
  }

  /** 指针落在第几行之前：拿中线当分界，比拿边界准（人眼看到的就是「插到这条前面」） */
  function indexAt(clientY: number, list: { id: string; el: HTMLElement }[]): number {
    for (let i = 0; i < list.length; i += 1) {
      const box = list[i]!.el.getBoundingClientRect()
      if (clientY < box.top + box.height / 2) return i
    }
    return list.length
  }

  function start(event: PointerEvent, id: string) {
    if (event.button !== 0) return
    event.preventDefault()
    draggingId.value = id
    const list = rows()

    // 监听挂在 window 上：指针滑出那一行的瞬间，元素级监听就会漏掉 pointerup
    const move = (ev: PointerEvent) => {
      const at = indexAt(ev.clientY, list.filter((r) => r.id !== id))
      overId.value = list[at] && list[at]!.id !== id ? list[at]!.id : null
    }

    const detach = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', cancel)
    }

    const up = (ev: PointerEvent) => {
      const without = list.filter((r) => r.id !== id)
      const at = indexAt(ev.clientY, without)
      const prevId = at > 0 ? (without[at - 1]?.id ?? null) : null
      const nextId = without[at]?.id ?? null
      detach()
      draggingId.value = null
      overId.value = null
      if (prevId !== id && nextId !== id && (prevId !== null || nextId !== null)) {
        onDrop({ id, prevId, nextId })
      }
    }

    const cancel = () => {
      detach()
      draggingId.value = null
      overId.value = null
    }

    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', cancel)
  }

  return { start, draggingId, overId }
}
