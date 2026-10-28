import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

import Medicine from './Medicine.vue'
import Blog from './Blog.vue'

const linkedState = vi.hoisted(() => ({
  value: {
    paths: {
      blogRepo: String.raw`D:\nope`,
      blogRepoExists: false,
      medicineDb: String.raw`C:\nope\medicine_tracker.db`,
      medicineDbExists: false,
      onlineCheckEnabled: true,
    },
    blog: [] as unknown[],
    medicine: null as unknown,
  },
}))

vi.mock('../api', () => ({
  api: {
    linkedState: async () => linkedState.value,
    linkedRefreshBlog: async () => [],
    linkedRefreshMedicine: async () => linkedState.value.medicine,
    linkedCheckOnline: async () => [],
    linkedRestockTodo: async () => null,
    linkedSetBlogDir: async () => linkedState.value.paths,
    todosDelete: async () => [],
    todosRestore: async () => undefined,
  },
  usingMock: true,
  runningInBrowser: true,
}))

const mountOpts = {
  global: { stubs: { teleport: true, Transition: false } },
}

beforeEach(() => {
  linkedState.value = {
    ...linkedState.value,
    blog: [],
    medicine: null,
  }
})

/**
 * M3 验收里最要紧的一条：外部程序被卸载 / 仓库被改名时，界面折叠成一行虚线条，
 * 不弹错、不显示假「已同步」。这里断言的是「只有那一行，没有别的报错面」。
 */
describe('外部源不可用时的降级', () => {
  it('药箱读不到 → 一行虚线条 + 原因，没有弹窗也没有假数据', async () => {
    linkedState.value.medicine = {
      source: 'medicine',
      kind: 'today',
      fetchedAt: Date.now() - 60_000,
      sig: '',
      status: 'missing',
      detail: '没找到药箱的库，可能已卸载或换过位置',
      payloadJson: '{"items":[],"count":0,"meta":{}}',
    }

    const wrapper = mount(Medicine, mountOpts)
    await flushPromises()

    const collapsed = wrapper.find('.y-empty.collapsed')
    expect(collapsed.exists()).toBe(true)
    expect(collapsed.text()).toContain('没找到药箱的库')
    expect(collapsed.text()).toContain('最后成功于')
    expect(wrapper.find('.med').exists()).toBe(false)
    expect(wrapper.find('.sum').exists()).toBe(false)
    // 不弹框：整棵树里不该出现 dialog / alert 角色
    expect(wrapper.find('[role="alertdialog"], [role="dialog"]').exists()).toBe(false)
  })

  it('博客仓库目录不在 → 折叠行说明是目录不可用，而不是「0 条」', async () => {
    linkedState.value.blog = [
      {
        source: 'blog',
        kind: 'projects',
        fetchedAt: Date.now(),
        sig: '',
        status: 'missing',
        detail: '本地仓库目录不可用（projects.json）',
        payloadJson: '{"items":[],"count":0,"meta":{}}',
      },
    ]

    const wrapper = mount(Blog, mountOpts)
    await flushPromises()

    const collapsed = wrapper.find('.y-empty.collapsed')
    expect(collapsed.exists()).toBe(true)
    expect(collapsed.text()).toContain('本地仓库里没有可显示的清单')
    expect(wrapper.find('.blk').exists()).toBe(false)
  })
})
