/**
 * 磁贴上那个 17px 的 × 要两步才删（issue #1 的 P1-5，选的是"加一次确认"，不动后端）。
 *
 * 为什么确认不能省：数据层是软删（`resources.deleted_at` 墓碑），**但界面上没有回收站**，
 * 所以误点一次的效果对用户就是"东西没了"。节奏与笔记页「彻底删除」同一条（5 秒内不点第二下就撤回），
 * 两处动作不可逆程度相似，就不该有两个不同的确认时长。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'

import Tile from './Tile.vue'
import type { Resource } from '../../api/types'

function res(over: Partial<Resource> = {}): Resource {
  return {
    id: 'r1',
    rev: 1,
    createdAt: 0,
    createdBy: 'pc',
    updatedAt: 0,
    updatedBy: 'pc',
    kind: 'app',
    name: '编辑器',
    target: 'C:/x/editor.exe',
    args: '',
    iconPath: null,
    zoneId: null,
    useCount: 3,
    lastLaunchedAt: null,
    note: '',
    ...over,
  }
}

/** 工具条里第二个按钮就是 ×（第一个是改名） */
const xBtn = (w: VueWrapper) => w.findAll('.t')[1]

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('移除磁贴的两步确认', () => {
  it('第一下只把 × 点亮，不发删除', async () => {
    const w = mount(Tile, { props: { resource: res() } })
    await xBtn(w).trigger('click')

    expect(w.emitted('remove'), '第一下就该把这次点击吃掉').toBeUndefined()
    expect(xBtn(w).classes()).toContain('armed')
    expect(xBtn(w).attributes('title')).toBe('再点一次真的移除')
  })

  it('第二下才真发，且只发一次', async () => {
    const w = mount(Tile, { props: { resource: res() } })
    await xBtn(w).trigger('click')
    await xBtn(w).trigger('click')

    expect(w.emitted('remove')).toHaveLength(1)
    expect(xBtn(w).classes()).not.toContain('armed')
  })

  it('5 秒不点第二下就撤回：回到第一步，不是"下一次点就删"', async () => {
    const w = mount(Tile, { props: { resource: res() } })
    await xBtn(w).trigger('click')
    vi.advanceTimersByTime(5000)
    await w.vm.$nextTick()

    expect(xBtn(w).classes()).not.toContain('armed')
    await xBtn(w).trigger('click')
    expect(w.emitted('remove'), '过期的 armed 不许留着偷偷完成删除').toBeUndefined()
  })

  it('确认中的那一下，× 的样式与「tools」一起留在屏幕上', async () => {
    // 这一组按钮平时只在 hover 时出现；如果 armed 那一下仍靠 hover 才可见，
    // 指针移开后就看不见"再点一次"的提示，回来直接点就等于删了
    const w = mount(Tile, { props: { resource: res() } })
    expect(w.find('.tools').classes()).not.toContain('shown')
    await xBtn(w).trigger('click')
    expect(w.find('.tools').classes()).toContain('shown')
  })

  it('卸载时撤掉那个定时器', async () => {
    const w = mount(Tile, { props: { resource: res() } })
    await xBtn(w).trigger('click')
    const cleared = vi.spyOn(window, 'clearTimeout')
    w.unmount()
    expect(cleared, '定时器留在组件外面继续跑，是在给已卸载的实例改状态').toHaveBeenCalled()
  })
})
