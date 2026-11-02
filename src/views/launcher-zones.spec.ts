/**
 * 分区的删除入口（2026-11-02 用户报的：「速达创建分区了不能删除」）。
 *
 * 账目上这不是"没实现"：`zone_delete` 命令、`api.zoneDelete`、mock 全都在，
 * 唯独界面上没有入口 —— 有命令没入口，和有能力没命令是同一类缺陷，
 * 而且更难发现，因为契约检查（命令 ↔ 调用点）只看得到 `real.ts` 那一层。
 * 这里钉的是：入口存在、只对真分区存在、两步确认、删完不停在已经不存在的筛选上。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

import type { Zone } from '../api/types'

const zoneDelete = vi.fn()
const zonesList = vi.fn()
const zones: Zone[] = [
  { id: 'z-dev', name: '开发', sort: 1, itemCount: 2 },
  { id: 'z-life', name: '生活', sort: 2, itemCount: 0 },
]

vi.mock('../api', () => ({
  api: {
    zonesList: () => zonesList(),
    resourcesList: async () => [],
    zoneAdd: async (name: string) => ({ id: 'z-new', name, sort: 9, itemCount: 0 }),
    zoneDelete,
  },
  usingMock: true,
  runningInBrowser: false,
}))

const Launcher = (await import('./Launcher.vue')).default

function chips(w: ReturnType<typeof mount>) {
  return w.findAll('.chipwrap')
}

beforeEach(() => {
  zoneDelete.mockReset()
  zonesList.mockReset()
  zonesList.mockResolvedValue(zones.map((z) => ({ ...z })))
})

describe('分区胶囊上的删除入口', () => {
  it('只有真分区带 ×，「全部」和「未分区」不带', async () => {
    const w = mount(Launcher)
    await flushPromises()
    const wraps = chips(w)
    // 全部 + 两个真分区 + 未分区
    expect(wraps).toHaveLength(4)
    expect(w.findAll('.kill')).toHaveLength(2)
    expect(wraps[0].find('.kill').exists(), '「全部」不该能被删').toBe(false)
    expect(wraps[3].find('.kill').exists(), '「未分区」是兜底格，不该能被删').toBe(false)
  })

  it('第一下只把 × 点亮，不发删除', async () => {
    const w = mount(Launcher)
    await flushPromises()
    await w.findAll('.kill')[0].trigger('click')
    expect(zoneDelete).not.toHaveBeenCalled()
    expect(w.findAll('.kill')[0].classes()).toContain('armed')
  })

  it('第二下才删，删的是那一个分区', async () => {
    const w = mount(Launcher)
    await flushPromises()
    const kill = w.findAll('.kill')[0]
    await kill.trigger('click')
    await kill.trigger('click')
    expect(zoneDelete).toHaveBeenCalledTimes(1)
    expect(zoneDelete).toHaveBeenCalledWith('z-dev')
  })

  it('正在看这个分区时，删完回到「全部」而不是停在一个已经不存在的筛选上', async () => {
    const w = mount(Launcher)
    await flushPromises()
    // 先切到「开发」这一格
    await w.findAll('.chip')[1].trigger('click')
    expect((w.vm as unknown as { activeZone: string }).activeZone).toBe('z-dev')

    const kill = w.findAll('.kill')[0]
    await kill.trigger('click')
    await kill.trigger('click')
    await flushPromises()
    expect((w.vm as unknown as { activeZone: string }).activeZone).toBe('all')
  })

  it('Esc 撤回待确认，不需要等 5 秒', async () => {
    const w = mount(Launcher)
    await flushPromises()
    const kill = w.findAll('.kill')[0]
    await kill.trigger('click')
    expect(kill.classes()).toContain('armed')
    await kill.trigger('keydown.esc')
    expect(w.findAll('.kill')[0].classes()).not.toContain('armed')
    await w.findAll('.kill')[0].trigger('click')
    expect(zoneDelete, '撤回之后第一下不该直接完成删除').not.toHaveBeenCalled()
  })

  it('分区清单读不出来时：那一句要说出来，剩下「全部/未分区」两颗还能用', async () => {
    zonesList.mockRejectedValue(new Error('数据库错误：Invalid column type Text at index: 2, name: sort'))
    const w = mount(Launcher)
    await flushPromises()
    expect(w.find('.err').text()).toContain('Invalid column type')
    const 胶囊 = w.findAll('.chip').filter((c) => !c.classes().includes('add'))
    expect(胶囊.map((c) => c.text()).join('/'), '真分区没了，但两颗固定的筛选还在').toBe('全部/未分区')
    expect(w.findAll('.chip.add').length, '新建分区的口子不能被一起带走').toBe(1)
  })
})
