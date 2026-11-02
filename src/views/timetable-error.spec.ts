import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

import Timetable from './Timetable.vue'
import type { TimetableState } from '../api/types'

/**
 * 课表页的「读不出来」与「还没导入」必须是两屏（P1-3 / 批次 2 的那一形状）。
 *
 * 空态那一行会支着用户去找一份并不存在的 CSV，而真该查的是这台机器上这个库读不读得动 ——
 * 所以这条测试钉的不是「有没有字出现」，是**两种口径不同时出现**。
 */

const fail = vi.hoisted(() => ({ on: false }))

const emptyState = (): TimetableState => ({
  term: '2026-2027-1',
  week1Monday: null,
  currentWeek: null,
  totalWeeks: null,
  rows: 0,
  courses: 0,
  periods: [{ startMin: 480, label: '08:00' }],
})

vi.mock('../api', () => ({
  api: {
    timetableState: async () => {
      if (fail.on) throw new Error('no such table: classes')
      return emptyState()
    },
    timetableWeek: async () => {
      if (fail.on) throw new Error('no such table: classes')
      return []
    },
  },
}))

async function render() {
  const w = mount(Timetable)
  await flushPromises()
  await flushPromises()
  return w
}

describe('课表页：错误与空态分得开', () => {
  beforeEach(() => {
    fail.on = false
  })

  it('后端读不动时说的是那句错，不再催我去导入 CSV', async () => {
    fail.on = true
    const w = await render()
    const bad = w.find('p.bad')
    expect(bad.exists()).toBe(true)
    expect(bad.text()).toContain('no such table')
    // 空态那一行（CollapsedRow）必须不出现：它教的动作此时是错的
    expect(w.text()).not.toContain('还没有导入过')
  })

  it('真的还没导入过时，才说那句可执行的话，且不红', async () => {
    const w = await render()
    expect(w.find('p.bad').exists()).toBe(false)
    expect(w.text()).toContain('还没有导入过')
  })
})
