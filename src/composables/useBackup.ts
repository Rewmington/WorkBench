import { ref } from 'vue'

import { api } from '../api'
import type { BackupOutcome, BackupStatus, RemoteFile } from '../api/types'
import { errText } from '../lib/err'

/**
 * 备份的读模型 + 四个动作。
 *
 * 状态（`status`）永远来自 SQLite 的 backup_log，不是来自内存：托盘驻留时
 * 每日备份在后台线程跑完，界面下次进这一页要看到的是那条流水，而不是「上次我点过什么」。
 */
const status = ref<BackupStatus | null>(null)
const remote = ref<RemoteFile[]>([])
const busy = ref<{ run: boolean; remote: boolean; test: boolean; restore: boolean }>({
  run: false,
  remote: false,
  test: false,
  restore: false,
})
const result = ref<BackupOutcome | null>(null)
const error = ref<string | null>(null)
/**
 * 还原专用的那一行。故意不复用 `error`：`error` 渲染在凭据那一块，而还原按钮在
 * 远端快照的列表里 —— 隔着一整页，失败原因看不见就会让人再点一次「确认还原」。
 */
const restoreError = ref<string | null>(null)
/** 「测试连接」的回话不是备份结果，别混进 result 里假装成 Outcome */
const notice = ref<string | null>(null)

export function useBackup() {
  async function load() {
    status.value = await api.backupStatus()
  }

  async function refreshRemote() {
    busy.value = { ...busy.value, remote: true }
    error.value = null
    try {
      remote.value = await api.backupListRemote()
    } catch (e) {
      error.value = errText(e)
      remote.value = []
    } finally {
      busy.value = { ...busy.value, remote: false }
    }
  }

  async function runNow() {
    // 命令面板那条路（Ctrl+K → 立即备份）不经过按钮，按钮上的 :disabled 拦不到它：
    // 连按两下就会排起两份同时加密上传同一天的快照，白烧一倍配额。
    if (busy.value.run) return
    busy.value = { ...busy.value, run: true }
    error.value = null
    notice.value = null
    try {
      result.value = await api.backupNow()
      if (!result.value.ok) error.value = result.value.error
      await load()
    } catch (e) {
      error.value = errText(e)
    } finally {
      busy.value = { ...busy.value, run: false }
    }
  }

  async function testConnection() {
    if (busy.value.test) return
    busy.value = { ...busy.value, test: true }
    error.value = null
    notice.value = null
    try {
      const n = await api.backupTestConnection()
      notice.value = `凭据可用，远端目录里列出 ${n} 份快照（只列目录，没写任何东西）`
      await refreshRemote()
    } catch (e) {
      error.value = errText(e)
    } finally {
      busy.value = { ...busy.value, test: false }
    }
  }

  async function saveCredentials(email: string, password: string) {
    error.value = null
    try {
      status.value = await api.backupSaveCredentials(email, password)
      await refreshRemote()
      return true
    } catch (e) {
      error.value = errText(e)
      return false
    }
  }

  async function clearCredentials() {
    error.value = null
    status.value = await api.backupClearCredentials()
    remote.value = []
    result.value = null
  }

  /** 清的是这张列表，不是备份：远端文件与本地暂存副本一个都不动 */
  async function clearLog() {
    error.value = null
    try {
      status.value = await api.backupClearLog()
    } catch (e) {
      error.value = errText(e)
    }
  }

  async function restore(name: string) {
    busy.value = { ...busy.value, restore: true }
    restoreError.value = null
    try {
      const outcome = await api.backupRestore(name)
      if (outcome.ok) return true
      // 还原失败有两种形状：抛出来的，和折进 Outcome 的（Rust 侧把远端取不回、
      // 完整性检查不过都折成 ok=false）。之前后者只会写进 `result`，于是渲染在
      // 「动作」那块 —— 那块讲的是备份，用户点的却是快照列表里那一行。
      // 这里也不再写 `result`：让「这次没成」继续只表示备份的结果。
      restoreError.value = outcome.error ?? '还原没成，当前这个库没被动过'
      return false
    } catch (e) {
      restoreError.value = errText(e)
      return false
    } finally {
      busy.value = { ...busy.value, restore: false }
    }
  }

  /** 不联网也能自证的「换机解得开」：加解密往返走一遍本机 KDF。 */
  async function selfTest(email: string, password: string) {
    error.value = null
    try {
      return await api.backupSelfTest(email, password)
    } catch (e) {
      error.value = errText(e)
      return false
    }
  }

  return {
    status,
    remote,
    busy,
    result,
    error,
    restoreError,
    notice,
    load,
    refreshRemote,
    runNow,
    testConnection,
    saveCredentials,
    clearCredentials,
    clearLog,
    restore,
    selfTest,
  }
}
