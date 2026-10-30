/**
 * 课表的显示侧计算。
 *
 * 「最近的空档」只有一个实现（PLAN §8.8 第 8 条）：课表页与待办「今天」档顶部那行
 * 都调这里的 `nextFreeGap`。Rust 那边只留 `weeks_include`（它要在 SQL 里过滤行），
 * 两边各写一遍迟早给出两个答案 —— 所以 Rust 那份空档计算被删了，不是没写。
 */

import type { ClassRow } from '../api/types'

/** 空档至少这么长才算"有空"。界面上要把这个数写出来（§2 原则 5：不藏着门槛）。 */
export const MIN_GAP_MINUTES = 30

const DAY_MS = 86_400_000

/** 本地自然日整数 yyyymmdd，与 Rust 的 `days::day_of_ms` 同语义 */
export function dayInt(ms: number): number {
  const d = new Date(ms)
  return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate()
}

/** 两个 yyyymmdd 之间的自然日数。**不能整数相减**：20261029 - 20260930 = 99，真实是 29。 */
export function daysBetween(fromDay: number, toDay: number): number {
  const parts = (n: number) => [Math.floor(n / 10000), Math.floor(n / 100) % 100, n % 100] as const
  const [ay, am, ad] = parts(fromDay)
  const [by, bm, bd] = parts(toDay)
  return Math.round((Date.UTC(by, bm - 1, bd) - Date.UTC(ay, am - 1, ad)) / DAY_MS)
}

/** `yyyy-mm-dd` → yyyymmdd。 */
export function isoToDay(iso: string): number {
  const [y, m, d] = iso.split('-').map(Number)
  return y * 10000 + m * 100 + d
}

/**
 * 今天在第几周。开学日没设 / 还没开学 / 学期已结束 → null，
 * 而不是 0 或者硬猜 —— 猜错会把一整个学期的课表显示成另一周的样子。
 */
export function weekOf(week1Monday: string | null, todayMs: number, totalWeeks: number | null): number | null {
  if (!week1Monday || totalWeeks == null) return null
  const elapsed = daysBetween(isoToDay(week1Monday), dayInt(todayMs))
  if (elapsed < 0) return null
  const week = Math.floor(elapsed / 7) + 1
  return week <= totalWeeks ? week : null
}

/** 分钟数 → `08:30`。 */
export function hhmm(min: number): string {
  const h = Math.floor(min / 60)
  const m = min % 60
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`
}

/**
 * 当前时间之后、长度 ≥ `MIN_GAP_MINUTES` 的第一段空档。
 *
 * 三件事都得对：已经上完的课不能把游标推回去；**重叠的课要合成一段**（调课很常见，
 * 不合并就会凭空造出一段不存在的空档）；最后一节之后不算空档 —— 那是放学，不是"有空"。
 */
export function nextFreeGap(rows: ClassRow[], nowMin: number): [number, number] | null {
  const spans = rows.map((r) => [r.startMin, r.endMin]).sort((a, b) => a[0] - b[0])
  let cursor = nowMin
  for (const [start, end] of spans) {
    if (end <= cursor) continue
    if (start - cursor >= MIN_GAP_MINUTES) return [cursor, start]
    cursor = Math.max(cursor, end)
  }
  return null
}

/** 一分钟的时刻（`Date.now()` 换算），给 nextFreeGap 当 nowMin 用 */
export function minutesOfDay(ms: number): number {
  const d = new Date(ms)
  return d.getHours() * 60 + d.getMinutes()
}
