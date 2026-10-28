import { describe, expect, it } from 'vitest'

import { NAV, VIEW_TITLES } from './useShell'

/**
 * 侧栏条目由功能清单决定，不由风格决定（PLAN §8.7）。
 * 这条测试守的是「别偷偷加没有对应功能的导航」—— 参考图里那些智能分析/知识图谱
 * 一类条目就是硬凑会做出「点进去是空页」的东西。
 */
describe('侧栏条目（§8.7）', () => {
  const items = NAV.flatMap((g) => g.items)

  it('v1 共 12 项，key 不重复', () => {
    expect(items).toHaveLength(12)
    expect(new Set(items.map((i) => i.key)).size).toBe(12)
  })

  it('分组标题按 进行 / 回顾 / 外部 / 系统 排，第一组无标题', () => {
    expect(NAV.map((g) => g.title ?? null)).toEqual([null, '进行', '回顾', '外部', '系统'])
    expect(NAV[0].items).toHaveLength(4)
  })

  it('标签用用户看得懂的话，且每项都能落到一个真实视图', () => {
    expect(items.map((i) => i.label)).toEqual([
      '今日驾驶舱',
      '待办',
      '速达',
      '笔记',
      '倒计时',
      '便签',
      '统计',
      '日历',
      '博客',
      '药箱',
      '备份',
      '设置',
    ])
    for (const item of items) {
      expect(VIEW_TITLES[item.view]).toBeTruthy()
    }
  })

  it('「进行」视图里的两个条目带落点，导航过去要能滚到对应部件', () => {
    const progress = items.filter((i) => i.view === 'progress')
    expect(progress.map((i) => i.focus)).toEqual(['countdowns', 'stickies'])
  })
})
