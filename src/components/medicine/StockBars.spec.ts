import { mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import StockBars from './StockBars.vue'
import type { MedicineTodayItem } from '../../api/types'

function item(over: Partial<MedicineTodayItem>): MedicineTodayItem {
  return {
    id: 'm',
    name: '药',
    unit: '片',
    remaining: 10,
    boxes: 1,
    dailyUse: 1,
    daysLeft: 10,
    lowDays: 7,
    isLow: false,
    isOut: false,
    slots: [],
    taken: 0,
    owed: 0,
    pending: 0,
    skipped: 0,
    ...over,
  }
}

const rows = (w: VueWrapper) => w.findAll('.row')

describe('各药还能吃几天', () => {
  it('照传进来的顺序画，不自己重排（顺序是后端那一条口径）', () => {
    const w = mount(StockBars, {
      props: { items: [item({ id: 'a', name: '二甲双胍', daysLeft: 3 }), item({ id: 'b', name: '钙片', daysLeft: 15 })] },
    })
    expect(rows(w).map((r) => r.find('.nm').text())).toEqual(['二甲双胍', '钙片'])
  })

  it('条长按这一列里最久的那支折算', () => {
    const w = mount(StockBars, {
      props: { items: [item({ daysLeft: 3 }), item({ daysLeft: 15 })] },
    })
    const widths = rows(w).map((r) => r.find('.fill').attributes('style') ?? '')
    expect(widths[1]).toContain('width: 100%')
    expect(widths[0]).toContain('width: 20%')
  })

  it('用完的是空条 + 红字，未设用量的不冒充「0 天」也不顶刻度', () => {
    const w = mount(StockBars, {
      props: {
        items: [
          item({ id: 'x', daysLeft: 0, remaining: 0, isOut: true, isLow: true }),
          item({ id: 'y', daysLeft: null, dailyUse: 0 }),
          item({ id: 'z', daysLeft: 40 }),
        ],
      },
    })
    const labels = rows(w).map((r) => r.find('.n').text())
    expect(labels).toEqual(['已用完', '未设用量', '剩 40 天'])
    expect(rows(w)[0].find('.fill').attributes('style')).toContain('width: 0%')
    expect(rows(w)[1].find('.n').classes()).toContain('unknown')
    // 最久的那支就是满格：条长是相对它折算的，不是绝对刻度
    expect(rows(w)[2].find('.fill').attributes('style')).toContain('width: 100%')
  })
})
