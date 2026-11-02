/**
 * 备份页「手机同步」那一块的口径（批次 10）。
 *
 * 台账里挂着一条"退出时同步没跑完就关窗"，读码读到最后剩下的其实不是数据问题：
 * `relay::run_with` 是先 PUT 再 `clear_dirty`，而 PC 那一侧只要本地有数据就每轮发全量，
 * 所以被打断的那一轮在下一轮一定会被带出去 —— 自愈路径是代码排出来的，不是祈愿。
 * 真正缺的是这句话没人说：界面上只有一棵「立即同步」，用户没有地方知道
 * "这台电脑不会自己同步"和"中途退出不会丢"。手机端同一个口径当初是写进设置页的，
 * 桌面端一直缺这一句。这里把它钉住，免得下次改文案时又顺手删掉。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'

import type { SyncStatus } from '../api/types'

const STATUS: SyncStatus = {
  configured: true,
  myDevice: 'pc-aaaa',
  lastSyncAt: Date.now() - 60_000,
  lastError: null,
  lastErrorAt: null,
  pending: 3,
  peers: ['phone-b'],
  unreadablePeers: [],
}

const syncStatus = ref<SyncStatus>({ ...STATUS })
const runNow = vi.fn(async () => STATUS)

vi.mock('../api', () => ({ usingMock: true, runningInBrowser: false }))
vi.mock('../composables/useBackup', () => ({
  useBackup: () => ({
    status: ref(null),
    result: ref(null),
    busy: ref(false),
    error: ref(null),
    notice: ref(null),
    remote: ref(null),
    restoreError: ref(null),
    load: async () => {},
    runNow: async () => {},
    saveCredentials: async () => {},
    clearCredentials: async () => {},
    testConnection: async () => {},
    selfTest: async () => {},
    refreshRemote: async () => {},
    restore: async () => {},
    clearLog: async () => {},
  }),
}))
vi.mock('../composables/useSync', () => ({
  useSync: () => ({
    status: syncStatus,
    running: ref(false),
    error: ref(null),
    cleaning: ref(false),
    cleaned: ref(null),
    load: async () => {},
    runNow,
    cleanPeers: async () => {},
  }),
}))
vi.mock('../composables/useShell', () => ({
  useShell: () => ({ select: () => {}, setCount: () => {}, selection: ref(null) }),
}))
vi.mock('../composables/useClock', () => ({ useClock: () => ({ now: ref(Date.now()) }) }))

const Backup = (await import('./Backup.vue')).default

beforeEach(() => {
  syncStatus.value = { ...STATUS }
  runNow.mockClear()
})

describe('同步节奏那句话', () => {
  it('说清"只有开机那一次自动同步、之后没有定时器"和"中途退出不会丢"', async () => {
    const w = mount(Backup)
    await flushPromises()
    const text = w.text()
    expect(text).toContain('开机后 60 秒会自动同步这一次')
    expect(text).toContain('之后没有定时器')
    expect(text).toContain('下一轮照常带上')
    expect(text, '不能顺手写成"完全不自动同步"（schedule.rs 那一次是真的）').not.toContain('不会自己定时同步')
  })

  it('那句话是常态说明，不跟着配置状态消失（没配凭据时也要看得见口径）', async () => {
    syncStatus.value = { ...STATUS, configured: false, peers: [], unreadablePeers: [] }
    const w = mount(Backup)
    await flushPromises()
    expect(w.text()).toContain('开机后 60 秒会自动同步这一次')
    expect(w.text()).toContain('先在上面的「凭据」里填好坚果云邮箱与应用密码')
  })

  it('按钮真的把那轮同步发出去：点一下 = 一次 runNow（不是只刷新状态）', async () => {
    const w = mount(Backup)
    await flushPromises()
    const btn = w.findAll('button').find((b) => b.text() === '立即同步')
    await btn!.trigger('click')
    await flushPromises()
    expect(runNow).toHaveBeenCalledTimes(1)
  })
})
