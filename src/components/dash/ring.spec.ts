import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import FocusRing from './FocusRing.vue'
import type { Todo } from '../../api/types'

vi.mock('../../api', () => ({
  usingMock: true,
  api: {
    settingsAll: async () => ({ values: {}, paths: {}, deviceId: 'pc-test' }),
    settingsSet: async () => undefined,
  },
}))

/** 造 N 张焦点卡。环只看数量与 id，字段给全是为了别改天加断言时又要重写夹具。 */
function cards(n: number): Todo[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `t${i}`,
    rev: 1,
    createdAt: 1,
    createdBy: 'pc-test',
    updatedAt: 1,
    updatedBy: 'pc-test',
    deletedAt: null,
    title: `第 ${i} 项`,
    note: '',
    doneAt: null,
    doneDay: null,
    completed: false,
    dueAt: 2,
    remindAt: null,
    priority: 2,
    pinned: false,
    parentId: null,
    sortKey: 1024 * (i + 1),
    zone: 'today',
    createdDay: 20261028,
    createdHour: 12,
    tags: [],
  }))
}

function mountRing(n: number) {
  return mount(FocusRing, { props: { cards: cards(n) } })
}

describe('卡片环的降级（§8.8.1 硬验收项）', () => {
  beforeEach(() => {
    document.documentElement.dataset.mode = 'dark'
  })

  it('0 面时既不画环也不画列表 —— 让父级折叠成一行虚线条', () => {
    const w = mountRing(0)
    expect(w.find('.y-ring').exists()).toBe(false)
    expect(w.find('.y-strip').exists()).toBe(false)
  })

  it('1 到 4 面退化成横向列表：面数太少时同时只有一张正对镜头，那不叫环', () => {
    for (const n of [1, 2, 3, 4]) {
      const w = mountRing(n)
      expect(w.find('.y-ring').exists(), `${n} 面不该画环`).toBe(false)
      expect(w.find('.y-strip').exists(), `${n} 面该画列表`).toBe(true)
      expect(w.findAll('.y-card')).toHaveLength(n)
    }
  })

  it('5 面起才画环，且每张卡按 360/n 铺开', () => {
    const w = mountRing(6)
    expect(w.find('.y-ring').exists()).toBe(true)
    const transforms = w.findAll('.y-c').map((c) => c.attributes('style') ?? '')
    expect(transforms[0]).toContain('rotateY(0deg)')
    expect(transforms[1]).toContain('rotateY(60deg)')
    expect(transforms[3]).toContain('rotateY(180deg)')
    // 6 面时 (cardW/2)/tan(π/6) ≈ 111 < 210，所以下限 210 生效
    expect(transforms[0]).toContain('translateZ(210px)')
  })

  it('最多的那一圈里卡片不会互相穿过', () => {
    // 环上半径由 max(210, (cardW/2)/tan(π/n)) 给出；128px 宽的卡在 n ≤ 8 时
    // 永远取到 210 的下限。真正要保证的是周长装得下所有卡：2πr ≥ n × cardW。
    const w = mountRing(8)
    const style = w.find('.y-c').attributes('style') ?? ''
    const r = Number(/translateZ\((\d+)px\)/.exec(style)?.[1] ?? 0)
    expect(2 * Math.PI * r).toBeGreaterThanOrEqual(8 * 128)
  })

  it('任何时刻只有一张卡是 accent 实心（§8.8.6 一屏一个重心）', () => {
    const w = mountRing(6)
    expect(w.findAll('.y-c.hot')).toHaveLength(1)
  })
})
