import { ref } from 'vue'

import { api } from '../api'
import type { DashSummary, Suggestion } from '../api/types'

/**
 * 驾驶舱的读模型。
 *
 * 一次 `dash_summary` 拿回 KPI + 环 + 队列 + 时间轴，而不是四个部件各查一遍 ——
 * 分开查会出现「KPI 说 3 条、队列里 4 条」这种最难查的对不上，因为它们读的是不同时刻的库。
 *
 * 1 秒 tick 只驱动相对时间显示，不回库：数字变了是因为用户做了事，不是因为秒针走了。
 */
const summary = ref<DashSummary | null>(null)
const suggestions = ref<Suggestion[]>([])
const busy = ref(false)

export function useDashboard() {
  async function refresh() {
    busy.value = true
    try {
      // 两条查询一起发：建议卡排在 KPI 后面，慢一点也无所谓，别串行等
      const [s, g] = await Promise.all([api.dashSummary(), api.dashSuggestions()])
      summary.value = s
      suggestions.value = g
    } finally {
      busy.value = false
    }
  }

  return { summary, suggestions, busy, refresh }
}
