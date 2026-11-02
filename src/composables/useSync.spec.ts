/**
 * 「清理我读不懂的对端文件」这一条动作的读模型（批次 9 的 P1-9 步骤 3）。
 *
 * 界面那一步是两步确认（与速达磁贴、笔记彻底删除同一条节奏，那边已有测试），
 * 这里钉的是确认之后发生的事：**结果取自 Rust 的回报，不取本地推测**，
 * 而且删完要重读一次状态 —— 名单还挂着已经不存在的文件，界面就会说过期结论。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { CleanedPeers, SyncStatus } from '../api/types'

const syncCleanPeers = vi.fn()
const syncStatus = vi.fn()

vi.mock('../api', () => ({
  api: { syncStatus, syncCleanPeers },
  usingMock: true,
  runningInBrowser: false,
}))

function status(over: Partial<SyncStatus> = {}): SyncStatus {
  return {
    configured: true,
    myDevice: 'pc-me',
    lastSyncAt: 1,
    lastError: null,
    lastErrorAt: null,
    pending: 0,
    peers: ['phone-c'],
    unreadablePeers: ['phone-c'],
    ...over,
  }
}

beforeEach(() => {
  vi.resetModules()
  syncCleanPeers.mockReset()
  syncStatus.mockReset()
  syncStatus.mockResolvedValue(status())
})

describe('useSync.cleanPeers', () => {
  it('把远端的回报原样交给界面，并重读一次状态', async () => {
    const { useSync } = await import('./useSync')
    const deleted: CleanedPeers = { deleted: ['device-phone-d.enc'], failed: [], skipped: [] }
    syncCleanPeers.mockResolvedValue(deleted)
    syncStatus.mockResolvedValue(status({ peers: [], unreadablePeers: [] }))

    const sync = useSync()
    await sync.cleanPeers()

    expect(sync.cleaned.value).toEqual(deleted)
    expect(sync.status.value?.unreadablePeers, '删完还报「这几台读不懂」就是过期结论').toEqual([])
    expect(sync.error.value).toBeNull()
    expect(sync.cleaning.value, '按钮不许卡在「清理中」').toBe(false)
  })

  it('一份都没删成时说的是失败的那几份和原因，不是「清理完成」', async () => {
    const { useSync } = await import('./useSync')
    syncCleanPeers.mockResolvedValue({
      deleted: [],
      failed: [{ name: 'device-phone-c.enc', why: '坚果云拒绝了凭据' }],
      skipped: [],
    } satisfies CleanedPeers)

    const sync = useSync()
    await sync.cleanPeers()

    expect(sync.cleaned.value?.deleted).toEqual([])
    expect(sync.cleaned.value?.failed[0].why).toContain('拒绝了凭据')
  })

  /// 「不该删」与「没删成」是两句话：前者是判据在保护另一台设备的历史，
  /// 后者是操作失败。混成一句，用户会以为再点一次就能把它删掉。
  it('故意没删的那几份带着理由单独占一格', async () => {
    const { useSync } = await import('./useSync')
    syncCleanPeers.mockResolvedValue({
      deleted: [],
      failed: [],
      skipped: [{ name: 'device-phone-c.enc', why: '这台机器解不开它（可能是别的凭据或账号写的），不删' }],
    } satisfies CleanedPeers)

    const sync = useSync()
    await sync.cleanPeers()

    expect(sync.cleaned.value?.skipped).toHaveLength(1)
    expect(sync.cleaned.value?.failed, '没删成是 0 件，不该被算成失败').toEqual([])
    expect(sync.error.value, '这不是错误，不该亮红').toBeNull()
  })

  it('命令本身抛错（凭据没配）时落到 error，不假装删过', async () => {
    const { useSync } = await import('./useSync')
    syncCleanPeers.mockRejectedValue(new Error('先在备份页填好坚果云邮箱与应用密码'))

    const sync = useSync()
    await sync.cleanPeers()

    expect(sync.error.value).toContain('备份页')
    expect(sync.cleaned.value, '没有发生过的删除不该留下一句"删掉了"').toBeNull()
    expect(syncStatus, '抛错时不该顺手重读状态').not.toHaveBeenCalled()
  })
})
