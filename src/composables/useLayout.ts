import { onBeforeUnmount, onMounted, ref } from 'vue'

/**
 * §8.4 的断点是按**内容区宽度**定的，不是窗口宽度 —— 侧栏与详情面板本身占掉 510px，
 * 所以必须测 #app 的主区，否则窗口拉到 1080 时内容其实只有 570。
 */
export type Cols = 'wide' | 'mid' | 'narrow'

const cols = ref<Cols>('wide')
let target: HTMLElement | null = null
let ro: ResizeObserver | null = null

function measure(width: number): Cols {
  if (width >= 1080) return 'wide'
  if (width >= 720) return 'mid'
  return 'narrow'
}

export function useLayout(root: () => HTMLElement | null) {
  onMounted(() => {
    target = root()
    if (!target) return
    cols.value = measure(target.clientWidth)
    ro = new ResizeObserver((entries) => {
      for (const e of entries) cols.value = measure(e.contentRect.width)
    })
    ro.observe(target)
  })

  onBeforeUnmount(() => {
    ro?.disconnect()
    ro = null
  })

  return { cols, detailInline: () => cols.value === 'wide' }
}
