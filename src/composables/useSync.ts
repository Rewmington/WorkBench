import { ref } from 'vue'

import { api } from '../api'
import type { CleanedPeers, SyncStatus } from '../api/types'
import { errText } from '../lib/err'

/**
 * 手机同步的读模型 + 一个动作（M11b）。
 *
 * 状态永远来自 SQLite + settings 里那份 sync.state（Rust 的 `sync::relay::record` 写的），
 * 不是内存：开机同步在后台线程跑完，界面下次进这一页要看到的是它，而不是「上次我点过什么」
 * —— 与 useBackup 的读模型是同一条纪律。
 *
 * 这里**没有** busy 之外的进度状态：同步不转圈、不弹窗（PLAN-MOBILE §5.5），
 * running 只用来禁用按钮那一下。
 */
const status = ref<SyncStatus | null>(null)
const running = ref(false)
const error = ref<string | null>(null)
const cleaning = ref(false)
/** 上一次清理真正删掉了什么。界面用它说话，不复述"应该已经删了" */
const cleaned = ref<CleanedPeers | null>(null)

export function useSync() {
  async function load() {
    status.value = await api.syncStatus()
  }

  /**
   * 手动「立即同步」。失败不抛给弹窗：Rust 把它折进 sync.state，这里重读状态，
   * 界面就多那一行状态。唯一会 throw 的是「凭据没配」那句可执行的话。
   */
  async function runNow() {
    running.value = true
    error.value = null
    try {
      status.value = await api.syncNow()
    } catch (e) {
      error.value = errText(e)
      await load()
    } finally {
      running.value = false
    }
  }

  /**
   * 「清理我读不懂的对端文件」。删的是别人设备上已发布的数据，跨设备不可逆 ——
   * 所以调用方（备份页那一颗按钮）必须先把它列出的是哪几份说清楚、由人点第二下确认。
   * 这里只负责"做完之后重读一遍状态"，界面上那句结果来自 Rust 的回报，不是本地推测。
   */
  async function cleanPeers() {
    cleaning.value = true
    error.value = null
    try {
      cleaned.value = await api.syncCleanPeers()
      await load()
    } catch (e) {
      error.value = errText(e)
    } finally {
      cleaning.value = false
    }
  }

  return { status, running, error, load, runNow, cleaning, cleaned, cleanPeers }
}
