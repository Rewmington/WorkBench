import { describe, expect, it } from 'vitest'

import {
  clockDate,
  countdownLabel,
  dayInt,
  daysBetween,
  dueLabel,
  overdueLabel,
  relTime,
} from './reltime'

// 用本地构造，不用 ISO 字符串：否则换台机器（非 UTC+8）这些断言就会漂一天。
const at = (y: number, mo: number, d: number, h = 0, mi = 0) => new Date(y, mo - 1, d, h, mi, 0).getTime()

// 2026-10-28（周三）15:00
const NOW = at(2026, 10, 28, 15, 0)

describe('relTime', () => {
  it('近处用分钟，远处换成具体日期', () => {
    expect(relTime(NOW, NOW - 20_000)).toBe('刚刚')
    expect(relTime(NOW, NOW - 4 * 60_000)).toBe('4 分钟前')
    expect(relTime(NOW, NOW - 2 * 3_600_000)).toBe('2 小时前')
    expect(relTime(NOW, at(2026, 10, 27, 22, 3))).toBe('昨天 22:03')
    expect(relTime(NOW, at(2026, 10, 25, 9, 0))).toBe('3 天前')
    expect(relTime(NOW, at(2026, 10, 21, 9, 0))).toBe('10月21日')
    expect(relTime(NOW, at(2025, 3, 1, 9, 0))).toBe('2025年3月1日')
  })

  it('跨午夜但不足 24 小时也算「昨天」，不写 3 小时前', () => {
    // 今天 15:00 减 20 小时 = 昨天 19:00
    expect(relTime(NOW, NOW - 20 * 3_600_000)).toBe('昨天 19:00')
  })

  it('时间戳落在 now 前面也只说「刚刚」，不说「即将」', () => {
    // 真机实测到的：Rust 写 fetched_at 的时钟比前端 1s tick 快几毫秒
    expect(relTime(NOW, NOW + 400)).toBe('刚刚')
    expect(relTime(NOW, NOW)).toBe('刚刚')
    // 未来的读法归 dueLabel，它才该说「今天/明天/N 小时后」
    expect(dueLabel(NOW, NOW + 3 * 3_600_000)).toBe('今天 18:00')
  })
})

describe('逾期与到期', () => {
  it('没逾期不编造逾期文案', () => {
    expect(overdueLabel(NOW, null)).toBeNull()
    expect(overdueLabel(NOW, NOW + 3_600_000)).toBeNull()
  })

  it('逾期按时/天分档，同一天内用小时', () => {
    expect(overdueLabel(NOW, NOW - 45 * 60_000)).toBe('逾期 45 分钟')
    expect(overdueLabel(NOW, at(2026, 10, 28, 12, 30))).toBe('逾期 2 小时')
    expect(overdueLabel(NOW, at(2026, 10, 19, 10, 0))).toBe('逾期 9 天')
  })

  it('将来用「今天/明天/后天/N 月 N 日」', () => {
    expect(dueLabel(NOW, at(2026, 10, 28, 18, 0))).toBe('今天 18:00')
    expect(dueLabel(NOW, at(2026, 10, 29, 9, 0))).toBe('明天 09:00')
    expect(dueLabel(NOW, at(2026, 10, 30, 9, 0))).toBe('后天 09:00')
    expect(dueLabel(NOW, at(2026, 11, 3, 20, 0))).toBe('11月3日')
  })
})

describe('倒计时', () => {
  it('不足一小时不显示小时位，负数夹到 0', () => {
    expect(countdownLabel(23 * 60_000 + 45_000)).toBe('23:45')
    expect(countdownLabel(3_600_000 + 23 * 60_000 + 45_000)).toBe('1:23:45')
    expect(countdownLabel(-5_000)).toBe('0:00')
  })
})

describe('自然日边界（PLAN §7 那个 UTC 坑的镜像）', () => {
  it('23:30 与次日 00:10 是两个不同的本地日', () => {
    const late = at(2026, 10, 28, 23, 30)
    const early = at(2026, 10, 29, 0, 10)
    expect(dayInt(late)).toBe(20261028)
    expect(dayInt(early)).toBe(20261029)
    expect(daysBetween(early, late)).toBe(1)
  })

  it('同一天内相差 23 小时仍是 0 天', () => {
    expect(daysBetween(at(2026, 10, 28, 23, 0), at(2026, 10, 28, 1, 0))).toBe(0)
  })
})

describe('时钟显示', () => {
  it('首屏的时钟行给出「10月28日 周三」这种带星期的日期', () => {
    expect(clockDate(NOW)).toBe('10月28日 周三')
  })
})
