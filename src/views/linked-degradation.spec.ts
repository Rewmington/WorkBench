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
      medicineDav: 'https://dav.jianguoyun.com/dav/medicine-sync/',
      medicineCloudReady: false,
      onlineCheckEnabled: true,
    },
    blog: [] as unknown[],
    medicine: null as unknown,
  },
}))

/** 保存云端凭据的次数：这一屏「存完立刻读一次」是刻意的，得能数出来。 */
const calls = vi.hoisted(() => ({ setPassword: 0, refresh: 0 }))

vi.mock('../api', () => ({
  api: {
    linkedState: async () => linkedState.value,
    linkedRefreshBlog: async () => [],
    linkedRefreshMedicine: async () => {
      calls.refresh += 1
      return linkedState.value.medicine
    },
    linkedCheckOnline: async () => [],
    linkedRestockTodo: async () => null,
    linkedSetBlogDir: async () => linkedState.value.paths,
    linkedSetMedicinePassword: async () => {
      calls.setPassword += 1
      return true
    },
    linkedClearMedicinePassword: async () => false,
    linkedSetMedicineDir: async (dir: string) => dir || 'https://dav.jianguoyun.com/dav/medicine-sync/',
    settingsGet: async () => null,
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
  linkedState.value.paths.medicineCloudReady = false
  calls.setPassword = 0
  calls.refresh = 0
})

function medicineSnap(detail: string) {
  return {
    source: 'medicine',
    kind: 'today',
    fetchedAt: Date.now() - 60_000,
    sig: '',
    status: 'ok',
    detail,
    payloadJson:
      '{"items":[{"id":"m1","name":"二甲双胍","unit":"片","remaining":6,"boxes":0.2,"dailyUse":2,"daysLeft":3,"lowDays":7,"isLow":true,"isOut":false,"slots":[],"taken":1,"owed":2,"pending":0,"skipped":0}],"count":1,"meta":{"todayTaken":1,"todayOwed":2,"lowCount":1,"nowMinutes":900}}',
  }
}

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

/**
 * M8：这一屏的数据可能是本机副本，也可能是手机上发的云端快照。
 * 用了哪一份必须看得见 —— 云端那份可能是几小时前的，不写就是拿旧数据冒充实时。
 */
describe('药箱数据源的标注', () => {
  it('没配云端时只说本机，不提云端', async () => {
    linkedState.value.medicine = medicineSnap('')
    const wrapper = mount(Medicine, mountOpts)
    await flushPromises()

    const src = wrapper.find('.src')
    expect(src.text()).toContain('本机副本')
    expect(src.text()).not.toContain('云端')
    expect(wrapper.find('.src').classes()).not.toContain('alt')
    // 底部那句必须指向它真正读的那个文件
    expect(wrapper.find('.note').text()).toContain('medicine_tracker.db')
  })

  it('用了云端那份时，顶上与底部都说它是云端', async () => {
    linkedState.value.medicine = medicineSnap('云端 · phone 发布于 3 分钟前')
    const wrapper = mount(Medicine, mountOpts)
    await flushPromises()

    const src = wrapper.find('.src')
    expect(src.text()).toContain('云端 · phone 发布于 3 分钟前')
    expect(src.classes()).toContain('alt')
    const note = wrapper.find('.note').text()
    expect(note).toContain('medicine-sync')
    // 这时候再写「来自 medicine_tracker.db 的副本」就是假话
    expect(note).not.toContain('medicine_tracker.db')
  })

  it('本机赢但云端也读上了，那句照抄，不简化成「实时」', async () => {
    linkedState.value.medicine = medicineSnap('本机 · 云端那份是 phone 3 天前发布')
    const wrapper = mount(Medicine, mountOpts)
    await flushPromises()

    expect(wrapper.find('.src').text()).toContain('云端那份是 phone 3 天前发布')
    expect(wrapper.find('.src').classes()).not.toContain('alt')
  })

  it('保存云端凭据后立刻读一次，不等下次进这一页', async () => {
    linkedState.value.medicine = medicineSnap('')
    const wrapper = mount(Medicine, mountOpts)
    await flushPromises()
    const afterMount = calls.refresh

    await wrapper.find('input[type="email"]').setValue('me@example.com')
    await wrapper.find('input[type="password"]').setValue('app-password')
    const before = calls.refresh
    const save = wrapper
      .findAll('.blk button')
      .find((b) => b.text().includes('保存并立即读一次'))
    expect(save).toBeTruthy()
    await save!.trigger('click')
    await flushPromises()

    expect(calls.setPassword).toBe(1)
    expect(calls.refresh).toBeGreaterThan(Math.max(afterMount, before))
  })

  it('填错目录时把后端的原话显示出来，不自造一句', async () => {
    linkedState.value.medicine = medicineSnap('')
    const wrapper = mount(Medicine, mountOpts)
    await flushPromises()

    const dir = wrapper.findAll('input').find((i) => i.attributes('spellcheck') === 'false')
    expect(dir).toBeTruthy()
    await dir!.setValue('https://dav.jianguoyun.com/dav/workbench-backup/')
    const saveDir = wrapper
      .findAll('.blk button')
      .find((b) => b.text() === '保存')
    await saveDir!.trigger('click')
    await flushPromises()

    expect(wrapper.find('.bad').exists()).toBe(false)
    expect(dir!.element.value).toContain('workbench-backup')
  })
})
