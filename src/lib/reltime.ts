/**
 * 相对时间格式化。全部是 `(now, ts)` 的纯函数 —— 组件不许自己取时间（tools/lint-style.mjs
 * 拦 `Date.now()`），这样「改系统时间后倒计时与『逾期 X 小时』跟着变」才是可证的：
 * 唯一的时间源是 useClock 的 tick，这里只吃它传进来的值。
 */

const MINUTE = 60_000
const HOUR = 3_600_000
const DAY = 86_400_000

const pad = (n: number) => String(n).padStart(2, '0')

/** 本地自然日整数 yyyymmdd，与 Rust 侧 `time::days::day_of_ms` 同语义 */
export function dayInt(ms: number): number {
  const d = new Date(ms)
  return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate()
}

function startOfDay(ms: number): number {
  const d = new Date(ms)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

/** 相差几个自然日（1 表示「昨天」这种），不看小时只看日期 */
export function daysBetween(now: number, ts: number): number {
  return Math.round((startOfDay(now) - startOfDay(ts)) / DAY)
}

/** 某月全部自然日的 yyyymmdd，按顺序。完成时间轴一行铺的就是它。 */
export function monthDays(day: number): number[] {
  const y = Math.floor(day / 10000)
  const m = Math.floor(day / 100) % 100
  const last = new Date(y, m, 0).getDate()
  return Array.from({ length: last }, (_, i) => y * 10000 + m * 100 + i + 1)
}

export function clockTime(ms: number): string {
  const d = new Date(ms)
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

const WEEKDAYS = ['周日', '周一', '周二', '周三', '周四', '周五', '周六']

export function clockDate(ms: number): string {
  const d = new Date(ms)
  return `${d.getMonth() + 1}月${d.getDate()}日 ${WEEKDAYS[d.getDay()]}`
}

/** 秒表式剩余时间：不足一小时不显示小时位 */
export function countdownLabel(msLeft: number): string {
  const total = Math.max(0, Math.floor(msLeft / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`
}

/** 「4 分钟前 / 3 天前 / 10月21日」—— 越远越具体，不写「很久以前」 */
export function relTime(now: number, ts: number): string {
  const diff = now - ts
  if (diff < 0) return futureLabel(now, ts)
  if (diff < MINUTE) return '刚刚'
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} 分钟前`
  if (diff < DAY && dayInt(now) === dayInt(ts)) return `${Math.floor(diff / HOUR)} 小时前`

  const days = daysBetween(now, ts)
  if (days === 1) return `昨天 ${clockTime(ts)}`
  if (days < 7) return `${days} 天前`
  const d = new Date(ts)
  const sameYear = d.getFullYear() === new Date(now).getFullYear()
  return sameYear
    ? `${d.getMonth() + 1}月${d.getDate()}日`
    : `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`
}

function futureLabel(now: number, ts: number): string {
  const diff = ts - now
  if (diff < MINUTE) return '即将'
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} 分钟后`
  if (diff < DAY && dayInt(now) === dayInt(ts)) return `${Math.floor(diff / HOUR)} 小时后`
  return dayWordLabel(now, ts)
}

/**
 * 截止时间的读法：优先「今天 18:00」这种**日词 + 绝对钟点**，而不是「3 小时后」——
 * 待办队列里要的是「今天几点之前」，相对量扫第二眼时就得重新心算（§2 原则 1）。
 */
export function dayWordLabel(now: number, ts: number): string {
  const days = daysBetween(ts, now)
  if (days === 0) return `今天 ${clockTime(ts)}`
  if (days === 1) return `明天 ${clockTime(ts)}`
  if (days === 2) return `后天 ${clockTime(ts)}`
  if (days === -1) return `昨天 ${clockTime(ts)}`
  const d = new Date(ts)
  const sameYear = d.getFullYear() === new Date(now).getFullYear()
  return sameYear
    ? `${d.getMonth() + 1}月${d.getDate()}日`
    : `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`
}

/**
 * 逾期时长。没逾期返回 null，让调用方去显示「今天 18:00」这类正常到期文案 ——
 * 状态不只靠颜色（§2 原则 3），所以逾期的表达是文字 + 图标，不是一片红。
 */
export function overdueLabel(now: number, due: number | null): string | null {
  if (due == null || now <= due) return null
  const diff = now - due
  if (diff < HOUR) return `逾期 ${Math.max(1, Math.floor(diff / MINUTE))} 分钟`
  if (diff < DAY && dayInt(now) === dayInt(due)) return `逾期 ${Math.floor(diff / HOUR)} 小时`
  const days = daysBetween(now, due)
  return `逾期 ${Math.max(1, days)} 天`
}

/** 到期文案：未逾期走「今天/明天/后天/N 月 N 日」，逾期走 overdueLabel */
export function dueLabel(now: number, due: number | null): string {
  const over = overdueLabel(now, due)
  if (over) return over
  if (due == null) return '无截止'
  return dayWordLabel(now, due)
}
