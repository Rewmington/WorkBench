/**
 * 详情面板上补的三个入口（待办打标签 / 速达换分区 / 速达写加密备注）。
 *
 * 这三条不是"没实现"：`todos_set_tags`、`resource_move`、`resource_set_secret`
 * 从 M3 起就在 `generate_handler!` 里，`real.ts` 也包好了，mock 甚至逐条对着真实现镜像过 ——
 * 唯独界面上没有任何地方按得到它们。2026-11-02 扫第二层（composable → 模板）时露出来的。
 * 这里钉的是：入口在、语义对（整组替换而不是追加）、点当前值不产生一次无意义的写入、
 * 失败要说话而不是显示成"没有备注"。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'

import type { Resource, Todo, Zone } from '../../api/types'

const selection = ref<{ kind: 'todo' | 'resource'; id: string } | null>(null)

const todosGet = vi.fn()
const todosSetTags = vi.fn()
const resourcesList = vi.fn()
const zonesList = vi.fn()
const resourceMove = vi.fn()
const resourceSetSecret = vi.fn()
const resourceRevealSecret = vi.fn()

vi.mock('../../api', () => ({
  api: {
    todosGet: (id: string) => todosGet(id),
    todosChildren: async () => [],
    todosSetTags: (id: string, tags: string[]) => todosSetTags(id, tags),
    resourcesList: () => resourcesList(),
    zonesList: () => zonesList(),
    resourceMove: (id: string, zoneId: string | null) => resourceMove(id, zoneId),
    resourceSetSecret: (id: string, text: string) => resourceSetSecret(id, text),
    resourceRevealSecret: (id: string) => resourceRevealSecret(id),
    resourceLaunch: async () => {},
  },
  usingMock: true,
  runningInBrowser: false,
}))

vi.mock('../../composables/useShell', () => ({
  useShell: () => ({ selection, detailOpen: ref(false) }),
}))
vi.mock('../../composables/useTodos', () => ({
  useTodos: () => ({ all: ref<Todo[]>([]) }),
}))
vi.mock('../../composables/useClock', () => ({
  useClock: () => ({ now: ref(1_700_000_000_000) }),
}))

const DetailPanel = (await import('./DetailPanel.vue')).default

const TODO: Todo = {
  id: 't-1',
  rev: 3,
  createdAt: 1,
  createdBy: 'pc',
  updatedAt: 2,
  updatedBy: 'pc',
  deletedAt: null,
  title: '写周报',
  note: '',
  zone: 'today',
  parentId: null,
  dueAt: null,
  remindAt: null,
  completed: false,
  doneAt: null,
  doneDay: null,
  priority: 2,
  pinned: false,
  sortKey: 100,
  createdDay: 20261102,
  createdHour: 9,
  tags: ['重要'],
}

const RESOURCE: Resource = {
  id: 'r-1',
  rev: 2,
  createdAt: 1,
  createdBy: 'pc',
  updatedAt: 2,
  updatedBy: 'pc',
  kind: 'app',
  name: 'Blender',
  target: 'C:\\Blender\\blender.exe',
  args: '',
  iconPath: null,
  zoneId: 'z-dev',
  useCount: 5,
  lastLaunchedAt: null,
  note: '',
}

const ZONES: Zone[] = [
  { id: 'z-dev', name: '开发', sort: 1, itemCount: 2 },
  { id: 'z-life', name: '生活', sort: 2, itemCount: 0 },
]

function staged() {
  todosGet.mockResolvedValue({ ...TODO })
  resourcesList.mockResolvedValue([{ ...RESOURCE }])
  zonesList.mockResolvedValue(ZONES.map((z) => ({ ...z })))
  todosSetTags.mockResolvedValue(undefined)
  resourceMove.mockResolvedValue(undefined)
  resourceSetSecret.mockResolvedValue(undefined)
  resourceRevealSecret.mockResolvedValue(null)
}

beforeEach(() => {
  for (const f of [todosGet, todosSetTags, resourcesList, zonesList, resourceMove, resourceSetSecret, resourceRevealSecret]) {
    f.mockReset()
  }
  staged()
  selection.value = { kind: 'todo', id: 't-1' }
})

describe('待办的标签入口', () => {
  it('现有标签一颗一颗带 ×，旁边就是加标签的输入框', async () => {
    const w = mount(DetailPanel)
    await flushPromises()
    const chip = w.findAll('.y-chip').find((c) => c.text() === '重要 ×')
    expect(chip).toBeTruthy()
    expect(w.find('input[aria-label="给这条待办加标签"]').exists()).toBe(true)
  })

  it('回车是整组替换：已有的和新增的合起来、去重，不是只把新那一个发过去', async () => {
    const w = mount(DetailPanel)
    await flushPromises()
    await w.find('input[aria-label="给这条待办加标签"]').setValue('急， 重要 再一个')
    await w.find('input[aria-label="给这条待办加标签"]').trigger('keydown.enter')
    await flushPromises()
    expect(todosSetTags).toHaveBeenCalledWith('t-1', ['重要', '急', '再一个'])
  })

  it('点 × 去掉那一个，别的都留着', async () => {
    todosGet.mockResolvedValue({ ...TODO, tags: ['重要', '私人'] })
    const w = mount(DetailPanel)
    await flushPromises()
    const kill = w.findAll('.x').find((b) => b.attributes('title') === '去掉标签 私人')
    await kill!.trigger('click')
    await flushPromises()
    expect(todosSetTags).toHaveBeenCalledWith('t-1', ['重要'])
  })
})

describe('速达的分区入口', () => {
  it('列出「未分区」与每个真分区，当前所在那颗标出来', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    const w = mount(DetailPanel)
    await flushPromises()
    const picks = w.findAll('.zpicks .y-chip')
    expect(picks.map((p) => p.text())).toEqual(['未分区', '开发', '生活'])
    expect(picks[1].classes()).toContain('on')
  })

  it('点另一个分区才写：resourceMove 收到目标 id', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    const w = mount(DetailPanel)
    await flushPromises()
    await w.findAll('.zpicks .y-chip')[2].trigger('click')
    await flushPromises()
    expect(resourceMove).toHaveBeenCalledWith('r-1', 'z-life')
  })

  it('点自己那一颗不产生一次无意义的写入（rev 白涨一条）', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    const w = mount(DetailPanel)
    await flushPromises()
    await w.findAll('.zpicks .y-chip')[1].trigger('click')
    await flushPromises()
    expect(resourceMove).not.toHaveBeenCalled()
  })

  it('挪进「未分区」发的是 null，不是空字符串', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    const w = mount(DetailPanel)
    await flushPromises()
    await w.findAll('.zpicks .y-chip')[0].trigger('click')
    await flushPromises()
    expect(resourceMove).toHaveBeenCalledWith('r-1', null)
  })
})

describe('速达的加密备注入口', () => {
  it('点开才有输入框，保存把文本交给 resource_set_secret', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    const w = mount(DetailPanel)
    await flushPromises()
    expect(w.find('textarea').exists()).toBe(false)
    const open = w.findAll('button').find((b) => b.text() === '写 / 改加密备注')
    await open!.trigger('click')
    await w.find('textarea').setValue('授权码在抽屉第二层')
    const save = w.findAll('button').find((b) => b.text() === '保存')
    await save!.trigger('click')
    await flushPromises()
    expect(resourceSetSecret).toHaveBeenCalledWith('r-1', '授权码在抽屉第二层')
  })

  it('写的时候本机密钥坏了要说出来，不能显示成「没有备注」', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    resourceSetSecret.mockRejectedValue(new Error('本机密钥读不出来：请检查 Windows 凭据管理器'))
    const w = mount(DetailPanel)
    await flushPromises()
    const open = w.findAll('button').find((b) => b.text() === '写 / 改加密备注')
    await open!.trigger('click')
    await w.find('textarea').setValue('x')
    const save = w.findAll('button').find((b) => b.text() === '保存')
    await save!.trigger('click')
    await flushPromises()
    expect(w.text()).toContain('本机密钥读不出来')
  })

  it('全是空白的备注存下去就是清除；发的是原文，trim 不 trim 由后端判（与真实现同口径）', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    const w = mount(DetailPanel)
    await flushPromises()
    const open = w.findAll('button').find((b) => b.text() === '写 / 改加密备注')
    await open!.trigger('click')
    await w.find('textarea').setValue('   ')
    const save = w.findAll('button').find((b) => b.text() === '保存')
    await save!.trigger('click')
    await flushPromises()
    expect(resourceSetSecret).toHaveBeenCalledWith('r-1', '   ')
  })

  it('清除之后再查看，显示「没有备注」而不是空白一行（解出来是空串，不是 null）', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    resourceRevealSecret.mockResolvedValue('')
    const w = mount(DetailPanel)
    await flushPromises()
    const reveal = w.findAll('button').find((b) => b.text() === '查看加密备注')
    await reveal!.trigger('click')
    await flushPromises()
    expect(w.text()).toContain('没有备注')
  })

  it('分区清单读不出来时：详情照常显示，并把这一句说出来（真窗口撞见过：库里 sort 列类型不对）', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    zonesList.mockRejectedValue(new Error('数据库错误：Invalid column type Text at index: 2, name: sort'))
    const w = mount(DetailPanel)
    await flushPromises()
    expect(w.find('.dt').text()).toBe('Blender')
    expect(w.find('.zerr').text()).toContain('分区清单读不出来')
    expect(w.find('.zerr').text()).toContain('Invalid column type')
    // 「未分区」这一颗还在：挪出分区不依赖清单
    expect(w.findAll('.zpicks .y-chip').map((c) => c.text())).toEqual(['未分区'])
  })

  it('网页类条目不给写备注的口子（与「查看加密备注」同一条件）', async () => {
    selection.value = { kind: 'resource', id: 'r-1' }
    resourcesList.mockResolvedValue([{ ...RESOURCE, kind: 'web' }])
    const w = mount(DetailPanel)
    await flushPromises()
    expect(w.text()).not.toContain('写 / 改加密备注')
    expect(w.text()).not.toContain('查看加密备注')
  })
})
