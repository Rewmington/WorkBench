/**
 * 备份动作的重复触发保护（2026-11-02 扫命令面板那一路查出来的）。
 *
 * 备份页那棵按钮自己有 `:disabled="busy.run"`，所以这条路一直是安全的；
 * 但命令面板里「立即备份到坚果云」调的是同一个 composable 函数，不经过按钮 ——
 * 连按两下就会排起两份并发任务，各自把整库导出、加密、PUT 到同一个日期键上：
 * 传上去的内容是同一份，但配额是双份，而且两条流水会互相盖掉状态。
 * 这里钉的是：跑着的时候第二次调用直接返回，跑完之后还能再跑。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { BackupOutcome, BackupStatus } from '../api/types'

const backupNow = vi.fn()
const backupStatus = vi.fn()
const backupTestConnection = vi.fn()
const backupListRemote = vi.fn()

vi.mock('../api', () => ({
  api: {
    backupStatus,
    backupNow,
    backupTestConnection,
    backupListRemote,
    backupRestore: async () => ({}),
    backupClearLog: async () => {},
    backupSaveCredentials: async () => {},
    backupClearCredentials: async () => {},
    backupSelfTest: async () => {},
    settingsGet: async () => null,
    settingsSet: async () => {},
  },
  usingMock: true,
  runningInBrowser: false,
}))

const { useBackup } = await import('../composables/useBackup')

function outcome(over: Partial<BackupOutcome> = {}): BackupOutcome {
  return { at: 1, ok: true, bytes: 1200, remotePath: 'anvil-2026-11-02.enc', error: null, skipped: false, ...over }
}

function status(over: Partial<BackupStatus> = {}): BackupStatus {
  return {
    configured: true,
    passwordStored: true,
    email: 'a@b.c',
    lastSuccessAt: 1,
    lastError: null,
    lastErrorAt: null,
    log: [],
    remote: [],
    paths: {} as BackupStatus['paths'],
    journalMode: 'wal',
    ...over,
  } as BackupStatus
}

let deferred: ((v: BackupOutcome) => void)[] = []

beforeEach(() => {
  for (const f of [backupNow, backupStatus, backupTestConnection, backupListRemote]) f.mockReset()
  deferred = []
  backupStatus.mockResolvedValue(status())
  backupListRemote.mockResolvedValue([])
  backupTestConnection.mockResolvedValue(0)
  // 手动可控的 promise：这样能真实地"在上一轮还没回来的时候再按一次"
  backupNow.mockImplementation(
    () => new Promise<BackupOutcome>((res) => deferred.push(res)),
  )
})

describe('立即备份不许排队', () => {
  it('上一轮还没结束时的第二次调用直接返回，不会排起第二份上传', async () => {
    const b = useBackup()
    const first = b.runNow()
    await Promise.resolve()
    expect(b.busy.value.run, '第一下就该把 busy.run 立起来').toBe(true)

    await b.runNow()
    expect(backupNow, '第二下不该再打一次命令').toHaveBeenCalledTimes(1)

    deferred[0](outcome())
    await first
    expect(b.busy.value.run).toBe(false)
  })

  it('跑完一轮之后还能再跑（守卫不能变成一次性的）', async () => {
    const b = useBackup()
    const first = b.runNow()
    await Promise.resolve()
    deferred[0](outcome())
    await first

    const second = b.runNow()
    await Promise.resolve()
    expect(backupNow).toHaveBeenCalledTimes(2)
    deferred[1](outcome())
    await second
  })

  it('测连接也守同一条：上一轮没回来时第二次不打命令', async () => {
    let release: ((n: number) => void) | null = null
    backupTestConnection.mockImplementation(() => new Promise<number>((res) => (release = res)))
    const b = useBackup()
    const first = b.testConnection()
    await Promise.resolve()
    expect(b.busy.value.test, '第一下就该把 busy.test 立起来').toBe(true)

    await b.testConnection()
    expect(backupTestConnection, '第二下不该再打一次 PROPFIND').toHaveBeenCalledTimes(1)

    release!(3)
    await first
    expect(b.busy.value.test).toBe(false)
    expect(b.notice.value).toContain('3 份快照')
  })
})
