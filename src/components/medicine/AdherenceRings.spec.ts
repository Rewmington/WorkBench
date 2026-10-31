import { mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import AdherenceRings from './AdherenceRings.vue'

const props = (over: Record<string, unknown> = {}) => ({
  todayTaken: 3,
  todayOwed: 8,
  weekRate: 0.92,
  hasWeekPlan: true,
  streakDays: 5,
  ...over,
})

const values = (w: VueWrapper) => w.findAll('.legend .n').map((n) => n.text())

/** 从 stroke-dasharray 反解出弧长占比 —— 图上的形状与那句数字得是同一件事。 */
function frac(arc: { attributes(name: string): string | undefined }) {
  const [len, circ] = arc.attributes('stroke-dasharray')!.split(' ').map(Number)
  return len / circ
}

describe('用药三环', () => {
  it('三个数各占一环，弧长就是它自己的比例', () => {
    const w = mount(AdherenceRings, { props: props() })
    expect(values(w)).toEqual(['3 / 8', '92%', '5 天'])
    const arcs = w.findAll('.arc')
    expect(arcs).toHaveLength(3)
    expect(frac(arcs[0])).toBeCloseTo(3 / 8, 3)
    expect(frac(arcs[1])).toBeCloseTo(0.92, 3)
    expect(frac(arcs[2])).toBeCloseTo(5 / 7, 3)
  })

  it('连续打卡封顶在一周，不画出一圈多出来的那截', () => {
    const w = mount(AdherenceRings, { props: props({ streakDays: 11 }) })
    expect(frac(w.findAll('.arc')[2])).toBeCloseTo(1, 3)
  })

  it('近 7 天没有安排时不说 0%，也不画那根空弧', () => {
    const w = mount(AdherenceRings, { props: props({ weekRate: 0, hasWeekPlan: false }) })
    expect(values(w)[1]).toBe('没有安排')
    expect(w.findAll('.arc')).toHaveLength(2)
  })

  it('今天没有格子时今日服药不写成 0 / 0', () => {
    const w = mount(AdherenceRings, { props: props({ todayTaken: 0, todayOwed: 0 }) })
    expect(values(w)[0]).toBe('没有安排')
  })
})
