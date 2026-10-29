import { ref } from 'vue'

import { api } from '../api'
import type { StatsReport } from '../api/types'

/**
 * 统计页的读模型。
 *
 * 一次 `statsReport()` 拿全五张图 —— 分五个命令各自取时间，会出现
 * 「热力图有今天、柱状图没有」这种只有刷新一次才对得上的假象。
 */
const report = ref<StatsReport | null>(null)
const error = ref<string | null>(null)

export function useStats() {
  async function load() {
    try {
      report.value = await api.statsReport()
      error.value = null
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
    }
  }

  return { report, error, load }
}

/** `yyyymmdd` → `10/28`。图上只放得下这个。 */
export function dayShort(day: number): string {
  const s = String(day)
  return `${Number(s.slice(4, 6))}/${Number(s.slice(6, 8))}`
}

/** `yyyymmdd` → `2026 年 10 月`，给热力图的月份刻度。 */
export function monthLabel(day: number): string {
  const s = String(day)
  return `${s.slice(0, 4)} 年 ${Number(s.slice(4, 6))} 月`
}

/** 该日所在月的第一天，用来判断某一列是不是跨月了。 */
export function isFirstOfMonth(day: number): boolean {
  return day % 100 === 1
}
