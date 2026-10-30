import { describe, expect, it } from 'vitest'

import type { ClassRow } from '../api/types'
import { isoToDay, MIN_GAP_MINUTES, nextFreeGap, weekOf } from './timetable'

const at = (startMin: number, endMin: number): ClassRow => ({
  name: '课',
  teacher: '',
  room: '',
  day: 1,
  startMin,
  endMin,
  period: '',
  weeks: '1-16',
})

describe('nextFreeGap', () => {
  const day = [at(480, 580), at(600, 700), at(710, 800)]

  it('第一节之前那段是真空档', () => {
    expect(nextFreeGap(day, 400)).toEqual([400, 480])
  })

  it('正在上课与不足门槛的间隔都不算', () => {
    expect(nextFreeGap(day, 600)).toBeNull()
    expect(nextFreeGap(day, 700)).toBeNull()
    expect(nextFreeGap([at(480, 500), at(520, 700)], 500)).toBeNull()
    expect(MIN_GAP_MINUTES).toBe(30)
  })

  it('最后一节之后不凭空造一段空档', () => {
    expect(nextFreeGap(day, 800)).toBeNull()
  })

  it('刚好卡在门槛上算有空', () => {
    expect(nextFreeGap([at(480, 500), at(530, 700)], 500)).toEqual([500, 530])
  })

  it('重叠的课合成一段，否则会造出不存在的空档', () => {
    // 调课常见：两节时间重叠。游标必须取 max(end)
    expect(nextFreeGap([at(480, 700), at(500, 800)], 480)).toBeNull()
    expect(nextFreeGap([at(480, 500), at(490, 800)], 480)).toBeNull()
    expect(nextFreeGap([at(480, 500), at(490, 800), at(900, 960)], 480)).toEqual([800, 900])
  })
})

describe('weekOf', () => {
  // 2026-10-26 是周一
  const monday = '2026-10-26'
  const ms = (y: number, m: number, d: number, h = 10) => new Date(y, m - 1, d, h).getTime()

  it('第 1 周覆盖开学周一往后的七天', () => {
    expect(weekOf(monday, ms(2026, 10, 26), 19)).toBe(1)
    // 周日还在第 1 周：toBe 不吃第二个参数，说明写在注释里
    expect(weekOf(monday, ms(2026, 11, 1), 19)).toBe(1)
    expect(weekOf(monday, ms(2026, 11, 2), 19)).toBe(2)
  })

  it('跨月不能整数相减：10/26 → 11/02 是 7 天', () => {
    expect(isoToDay('2026-10-26')).toBe(20261026)
    expect(weekOf('2026-09-07', ms(2026, 10, 30), 19)).toBe(8)
  })

  it('没设开学日 / 还没开学 / 学期结束都回 null，而不是 0', () => {
    expect(weekOf(null, ms(2026, 10, 30), 19)).toBeNull()
    expect(weekOf(monday, ms(2026, 10, 25), 19)).toBeNull()
    // 10/26 → 03/01 是 126 天 = 第 19 周（正好在学期内），03/08 才是第 20 周
    expect(weekOf(monday, ms(2027, 3, 1), 19)).toBe(19)
    expect(weekOf(monday, ms(2027, 3, 8), 19)).toBeNull()
    expect(weekOf(monday, ms(2026, 10, 30), null)).toBeNull()
  })
})
