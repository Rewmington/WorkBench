import { describe, expect, it } from 'vitest'

import type { SyncStatus } from '../api/types'
import { clockOf, currentFailure, syncSentence } from './sync'

/**
 * PLAN-MOBILE §5.5 / §6 M11b 的文案钉子在前端这一侧的落点：
 * 状态行只写可验证的真事实，且表里「删掉的那一列」出现的频率为 0。
 */

const base = (part: Partial<SyncStatus>): SyncStatus => ({
  configured: true,
  myDevice: 'pc-0196e4b3',
  lastSyncAt: null,
  lastError: null,
  lastErrorAt: null,
  pending: 0,
  peers: [],
  ...part,
})

const at = (y: number, mo: number, d: number, h = 0, mi = 0) => new Date(y, mo - 1, d, h, mi, 0).getTime()

describe('syncSentence', () => {
  it('钉住 §5.5 留下的那句：上次同步于 12:03 · 2 条待上传', () => {
    const today = new Date()
    const s = base({ lastSyncAt: at(today.getFullYear(), today.getMonth() + 1, today.getDate(), 12, 3), pending: 2 })
    expect(syncSentence(s)).toBe('上次同步于 12:03 · 2 条待上传')
  })

  it('从没同步过就明说，不伪造时间（与「今天没有待办」是两种状态两句话）', () => {
    expect(syncSentence(base({ lastSyncAt: null, pending: 5 }))).toBe('还没同步过 · 5 条待上传')
    expect(syncSentence(base({ lastSyncAt: null, pending: 0 }))).toBe('还没同步过 · 没有待上传的')
  })

  it('零欠账时不许出现「已同步」这个词', () => {
    const s = base({ lastSyncAt: at(2026, 10, 27, 9, 0), pending: 0 })
    const line = syncSentence(s)
    expect(line).not.toContain('已同步')
    expect(line).toBe('上次同步于 10月27日 09:00 · 没有待上传的')
  })

  it('整族输出永远没有「同步中」与「已同步」', () => {
    const cases = [
      base({}),
      base({ pending: 3 }),
      base({ lastSyncAt: Date.now(), pending: 0 }),
      base({ lastSyncAt: Date.now(), lastError: '没连上', lastErrorAt: Date.now() }),
    ]
    for (const c of cases) {
      expect(syncSentence(c)).not.toMatch(/已同步|同步中|转圈/)
    }
  })
})

describe('currentFailure', () => {
  it('最近一次失败比成功新才显示那行', () => {
    const s = base({ lastSyncAt: at(2026, 10, 27, 9), lastError: '坚果云拒绝了凭据', lastErrorAt: at(2026, 10, 28, 9) })
    expect(currentFailure(s)).toBe('坚果云拒绝了凭据')
  })

  it('已被成功覆盖的旧失败不常驻红色警报（备份页 staleFailure 同判据）', () => {
    const s = base({ lastSyncAt: at(2026, 10, 28, 12), lastError: '坚果云拒绝了凭据', lastErrorAt: at(2026, 10, 27, 9) })
    expect(currentFailure(s)).toBeNull()
  })

  it('从没成功过且报过错：那句要能看见', () => {
    const s = base({ lastSyncAt: null, lastError: '坚果云请求失败：dns error', lastErrorAt: at(2026, 10, 28, 9) })
    expect(currentFailure(s)).toBe('坚果云请求失败：dns error')
  })
})

describe('clockOf', () => {
  it('跨了自然日就补上日期，别让人拿 08:10 猜哪天', () => {
    expect(clockOf(at(2026, 10, 27, 8, 10))).toBe('10月27日 08:10')
  })
})
