import { computed, ref } from 'vue'

import { api } from '../api'
import type { Todo, TodoZone } from '../api/types'
import { useClock } from './useClock'

/**
 * 待办的共享读模型。
 *
 * 待办清单、日历、驾驶舱要看的是同一批数据：各拉一遍会出现「列表说 3 条，
 * 徽章说 5 条」这种最难查的对不上，所以只留一份 all，别人都从它派生。
 */
const all = ref<Todo[]>([])
const loading = ref(false)
const error = ref<string | null>(null)

const startOfDay = (ms: number) => {
  const d = new Date(ms)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

export function useTodos() {
  const { now } = useClock()

  async function refresh() {
    loading.value = true
    try {
      all.value = await api.todosAll()
      error.value = null
    } catch (e) {
      // 读失败就留着上一次的结果，只把原因写在界面上，不清空也不弹框
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  const undone = computed(() => all.value.filter((t) => !t.doneAt))

  const inZone = (zone: TodoZone) =>
    computed(() =>
      undone
        .value.filter((t) => t.zone === zone)
        .sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.sortKey - b.sortKey),
    )

  // 这些都跟着 1 秒 tick 重算：逾期从「59 分钟」变「1 小时」不该等到下次刷新
  const overdue = computed(() =>
    undone.value.filter((t) => t.dueAt != null && t.dueAt < now.value),
  )

  const dueToday = computed(() => {
    const end = startOfDay(now.value) + 86_400_000
    return undone.value.filter((t) => t.dueAt != null && t.dueAt >= now.value && t.dueAt < end)
  })

  const doneToday = computed(() => {
    const floor = startOfDay(now.value)
    return all.value.filter((t) => t.doneAt != null && t.doneAt >= floor)
  })

  const byId = (id: string) => all.value.find((t) => t.id === id) ?? null

  const childrenOf = (id: string) => all.value.filter((t) => t.parentId === id)

  return {
    all,
    undone,
    loading,
    error,
    refresh,
    overdue,
    dueToday,
    doneToday,
    inbox: inZone('inbox'),
    today: inZone('today'),
    later: inZone('later'),
    someday: inZone('someday'),
    byId,
    childrenOf,
  }
}
