import type { SyncStatus } from '../api/types'

/**
 * 同步状态行的措辞（PLAN-MOBILE §5.5 的两条文案纪律在这里落地）：
 *
 * - 只写**可验证的真事实**：时间来自最后一次成功同步，条数来自 `merge::pending`（Rust 侧）。
 * - 表里删掉的那一列在这里同样不许出现：没有「已同步」，没有「同步中…（转圈）」。
 *   `syncSentence` 的输出永远不含这两个词 —— `sync.spec.ts` 钉着。
 *
 * 它是纯函数：`now` 不参与（显示的是发生过的时刻本身，不是相对时间），
 * 所以这条句子与「改系统时间」那条验收互不干扰。
 */

/** 12:03 那种本地时刻；跨了自然日就补上「日」，别让人拿 08:10 猜是哪天。 */
export function clockOf(ms: number): string {
  const d = new Date(ms)
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
  const today = new Date()
  const sameDay =
    d.getFullYear() === today.getFullYear() &&
    d.getMonth() === today.getMonth() &&
    d.getDate() === today.getDate()
  return sameDay ? hm : `${d.getMonth() + 1}月${d.getDate()}日 ${hm}`
}

/** 「上次同步于 12:03 · 2 条待上传」；从没同步过就明说，不伪造时间。 */
export function syncSentence(s: SyncStatus): string {
  const when = s.lastSyncAt == null ? '还没同步过' : `上次同步于 ${clockOf(s.lastSyncAt)}`
  const debt = s.pending > 0 ? `${s.pending} 条待上传` : '没有待上传的'
  return `${when} · ${debt}`
}

/**
 * 失败那行只在「最近一次还没缓过来」时出现 —— 与备份页 staleFailure 同一判据：
 * 已被成功覆盖的旧失败，流水里那一行就够了，不该常驻一条红色警报。
 */
export function currentFailure(s: SyncStatus): string | null {
  if (!s.lastError) return null
  return s.lastSyncAt == null || (s.lastErrorAt ?? 0) > s.lastSyncAt ? s.lastError : null
}
