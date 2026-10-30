import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

import { api } from '../api'
import { startClock, stopClock } from '../composables/useClock'
import Todos from './Todos.vue'
import Timetable from './Timetable.vue'

/**
 * M9c 的三条硬验收（PLAN §9）里能用单测钉住的两条：
 * 「今天」档的形状、以及那行空档与课表页是不是同一个值。
 * 剩下的（3 断点 × 2 主题、点格子出关联待办）走截图。
 */

/** 2026-10-30 是周五，mock 课表里周五只有 16:00–17:40 一节体育课 */
const FRIDAY_10_AM = new Date(2026, 9, 30, 10, 0, 0)
/** 第 1 周的周一：与 mock 的默认学期对齐，10/30 落在第 8 周 */
const WEEK1_MONDAY = '2026-09-07'

const mountOpts = { global: { stubs: { teleport: true, Transition: false } } }

beforeEach(async () => {
  vi.useFakeTimers()
  vi.setSystemTime(FRIDAY_10_AM)
  // 时钟是全局单例，startClock 里那次 tick 才是"现在"——先冻结再启，两个页面才读同一个刻
  startClock()
  await api.timetableSetWeek1(WEEK1_MONDAY)
})

afterEach(() => {
  stopClock()
  vi.useRealTimers()
})

async function mountTodos() {
  const wrapper = mount(Todos, mountOpts)
  await flushPromises()
  return wrapper
}

describe('「今天」档按时间分带', () => {
  it('有截止时间的按时间排，没有的落进「随时」，做完的排最后一条带', async () => {
    await api.todosCreate({ title: '带时间 A', zone: 'today', dueAt: FRIDAY_10_AM.getTime() + 5 * 3_600_000 })
    await api.todosCreate({ title: '带时间 B', zone: 'today', dueAt: FRIDAY_10_AM.getTime() + 2 * 3_600_000 })
    await api.todosCreate({ title: '没时间的', zone: 'today' })
    await api.todosCreate({ title: '早做完的旧账', zone: 'today', dueAt: FRIDAY_10_AM.getTime() - 90 * 86_400_000 })
    const wrapper = await mountTodos()
    const done = wrapper.findAll('.row').find((r) => r.text().includes('早做完的旧账'))
    await done!.find('.cb').trigger('click')
    await flushPromises()
    const bands = wrapper.findAll('.band').filter((b) => b.find('.bandh').exists())
    const labels = bands.map((b) => b.find('.bandh').text())
    expect(labels[0]).toContain('按截止时间')
    expect(labels[1]).toContain('随时')
    expect(labels.at(-1)).toContain('已完成')

    const timed = bands[0]!.findAll('.t').map((t) => t.text())
    expect(timed.indexOf('带时间 B')).toBeLessThan(timed.indexOf('带时间 A'))
    expect(bands[0]!.findAll('.row.done')).toHaveLength(0)
    expect(bands[1]!.findAll('.t').map((t) => t.text())).toContain('没时间的')
    // 勾掉的那条再挂"逾期 90 天"就是自相矛盾
    const doneBand = bands.at(-1)!.text()
    expect(doneBand).toContain('早做完的旧账')
    expect(bands.at(-1)!.findAll('.over-txt')).toHaveLength(0)
  })

  it('逾期那条绿线是拖动落点提示，不该被"这条逾期了"复用（同名 class 会串味）', async () => {
    await api.todosCreate({ title: '逾期但不能变绿', zone: 'today', dueAt: FRIDAY_10_AM.getTime() - 3 * 3_600_000 })
    const wrapper = await mountTodos()
    const row = wrapper.findAll('.row').find((r) => r.text().includes('逾期但不能变绿'))
    expect(row).toBeTruthy()
    expect(row!.classes()).toContain('over')
    expect(row!.classes()).not.toContain('drag-over')
  })

  it('分带时拖动把手不出现：顺序由时间决定，拖完一次 reload 就被覆盖', async () => {
    await api.todosCreate({ title: '今天不能拖', zone: 'today' })
    await api.todosCreate({ title: '收件箱照旧', zone: 'inbox' })
    const wrapper = await mountTodos()
    expect(wrapper.findAll('.grip')).toHaveLength(0)

    const inbox = wrapper.findAll('.s').find((b) => b.text().includes('收件箱'))
    expect(inbox).toBeTruthy()
    await inbox!.trigger('click')
    await flushPromises()
    // 其余档保持一条一行，把手照旧
    expect(wrapper.findAll('.bandh')).toHaveLength(0)
    expect(wrapper.findAll('.grip').length).toBeGreaterThan(0)
  })
})

describe('空档那行与课表页对齐', () => {
  it('两个页面给出同一个 HH:MM–HH:MM', async () => {
    const todos = await mountTodos()
    expect(todos.find('.gap').text()).toContain('今天 1 节课 · 最近空档 10:00–16:00')

    const tt = mount(Timetable, mountOpts)
    await flushPromises()
    expect(tt.find('.legend').text()).toContain('最近空档 10:00–16:00')
  })

  it('没设开学日时不编一句「全天有空」', async () => {
    await api.timetableSetWeek1('')
    const wrapper = await mountTodos()
    expect(wrapper.find('.gap').exists()).toBe(false)
  })

  it('一节课也不出现在待办列表里（§6.7 边界 2）', async () => {
    const titles = (await mountTodos())
      .findAll('.t')
      .map((t) => t.text())
      .filter((t) => ['编译原理', '算法设计', '线性代数', '英语读写', '体育课'].includes(t))
    expect(titles).toEqual([])
  })
})
