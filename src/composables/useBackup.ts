import { ref } from 'vue'

import { api } from '../api'
import type { BackupOutcome, BackupStatus, RemoteFile } from '../api/types'

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
      error.value = String(e)
      remote.value = []
    } finally {
      busy.value = { ...busy.value, remote: false }
    }
  }

  async function runNow() {
    busy.value = { ...busy.value, run: true }
    error.value = null
    notice.value = null
    try {
      result.value = await api.backupNow()
      if (!result.value.ok) error.value = result.value.error
      await load()
    } catch (e) {
      error.value = String(e)
    } finally {
      busy.value = { ...busy.value, run: false }
    }
  }

  async function testConnection() {
    busy.value = { ...busy.value, test: true }
    error.value = null
    notice.value = null
    try {
      const n = await api.backupTestConnection()
      notice.value = `凭据可用，远端目录里列出 ${n} 份快照（只列目录，没写任何东西）`
      await refreshRemote()
    } catch (e) {
      error.value = String(e)
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
      error.value = String(e)
      return false
    }
  }

  async function clearCredentials() {
    error.value = null
    status.value = await api.backupClearCredentials()
    remote.value = []
    result.value = null
  }

  async function restore(name: string) {
    busy.value = { ...busy.value, restore: true }
    error.value = null
    try {
      result.value = await api.backupRestore(name)
      return result.value.ok
    } catch (e) {
      error.value = String(e)
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
      error.value = String(e)
      return false
    }
  }

  return {
    status,
    remote,
    busy,
    result,
    error,
    notice,
    load,
    refreshRemote,
    runNow,
    testConnection,
    saveCredentials,
    clearCredentials,
    restore,
    selfTest,
  }
}
