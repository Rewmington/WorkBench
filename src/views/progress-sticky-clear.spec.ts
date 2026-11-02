/**
 * 便签槽位的「清空」入口（`sticky_clear`）。
 *
 * 命令、Api、composable 三层都在，缺的还是那第四层：界面上只有输入框。
 * 于是"这一行我不要了"只能手动把字删光 —— 那只是存了一个空串，槽位记录还在，
 * 与 `sticky_clear` 语义（整条撤掉）不是一回事。
 * 与速达磁贴、分区删除同一个两步口径，包括 5 秒后自动解除待确认状态。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

import type { Sticky } from '../api/types'

const stickyClear = vi.fn()
const stickiesList = vi.fn()
const stickySave = vi.fn()

vi.mock('../api', () => ({
  api: {
    stickiesList: () => stickiesList(),
    stickySave: (slot: number, content: string) => stickySave(slot, content),
    stickyClear: (slot: number) => stickyClear(slot),
    countdownsList: async () => [],
    sysLoad: async () => ({ cpuPercent: null, memTotal: 0, memAvail: 0 }),
    resourcesList: async () => [],
    settingsGet: async () => null,
    settingsSet: async () => {},
  },
  usingMock: true,
  runningInBrowser: false,
}))

const Progress = (await import('./Progress.vue')).default

const STICKY: Sticky = {
  id: 's-1',
  rev: 1,
  createdAt: 1,
  createdBy: 'pc',
  updatedAt: 1,
  updatedBy: 'pc',
  deletedAt: null,
  slot: 1,
  content: '买猫粮',
}

beforeEach(() => {
  stickyClear.mockReset()
  stickiesList.mockReset()
  stickySave.mockReset()
  stickiesList.mockResolvedValue([{ ...STICKY }])
  stickyClear.mockResolvedValue(true)
  stickySave.mockResolvedValue({ ...STICKY })
  vi.useRealTimers()
})

async function opened() {
  const w = mount(Progress)
  await flushPromises()
  return w
}

describe('便签槽位清空', () => {
  it('每个槽位都有一颗「清空」', async () => {
    const w = await opened()
    expect(w.findAll('.ycl').length).toBe(2)
  })

  it('第一下只是把它变成确认，不调命令', async () => {
    const w = await opened()
    await w.findAll('.ycl')[0].trigger('click')
    await flushPromises()
    expect(stickyClear).not.toHaveBeenCalled()
    expect(w.findAll('.ycl')[0].text()).toBe('确认清空')
    expect(w.findAll('.ycl')[0].classes()).toContain('armed')
  })

  it('第二下才真的清：发的是这一槽的槽号，不是别的', async () => {
    const w = await opened()
    await w.findAll('.ycl')[1].trigger('click')
    await w.findAll('.ycl')[1].trigger('click')
    await flushPromises()
    expect(stickyClear).toHaveBeenCalledTimes(1)
    expect(stickyClear).toHaveBeenCalledWith(2)
  })

  it('清完那一格的字也没了（本地草稿与库要同一口径）', async () => {
    const w = await opened()
    expect((w.findAll('textarea')[0].element as HTMLTextAreaElement).value).toBe('买猫粮')
    await w.findAll('.ycl')[0].trigger('click')
    await w.findAll('.ycl')[0].trigger('click')
    await flushPromises()
    expect((w.findAll('textarea')[0].element as HTMLTextAreaElement).value).toBe('')
  })

  it('5 秒没按第二下就自动解除，回来还是「清空」', async () => {
    const w = await opened()
    vi.useFakeTimers()
    await w.findAll('.ycl')[0].trigger('click')
    expect(w.findAll('.ycl')[0].text()).toBe('确认清空')
    vi.advanceTimersByTime(6000)
    vi.useRealTimers()
    await flushPromises()
    expect(w.findAll('.ycl')[0].text()).toBe('清空')
    expect(stickyClear).not.toHaveBeenCalled()
  })

  it('同时只可能有一颗在待确认：按了 2 槽，1 槽就退回「清空」', async () => {
    const w = await opened()
    await w.findAll('.ycl')[0].trigger('click')
    await w.findAll('.ycl')[1].trigger('click')
    const texts = w.findAll('.ycl').map((b) => b.text())
    expect(texts).toEqual(['清空', '确认清空'])
  })
})
