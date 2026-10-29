// 逐图核对（§9 M7 的验收：「造 90 天种子数据，逐图核对聚合 SQL 与列表一致」）。
//
// 为什么不让 stats.rs 自己的 SQL 再跑一遍：那叫自证。这里每条对照都**故意换一种写法** ——
// 逐日参数化 count 代替 GROUP BY、JS 的 Date 算本地日代替 chrono、LEFT JOIN … IS NULL
// 代替 NOT EXISTS、应用自己的列表命令代替聚合命令。两套独立实现给出同一个数，
// 才说明那个数是真的。
//
// 要一个开着远程调试、且用 scratch 数据目录的实例（别碰真库）：
//   LOCALAPPDATA=<repo>\src-tauri\target\scratch \
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223 --remote-allow-origins=*" \
//     npm run tauri:dev -- -- --seed 90
//   node tools/verify.mjs --attach=9223 --db="<repo>\src-tauri\target\scratch\dev.rewmington.anvil\app.db"
import { DatabaseSync } from 'node:sqlite'

import { openCdp, pollPage } from './cdp.mjs'

const args = process.argv.slice(2)
const opt = (name, dflt) => {
  const hit = args.find((a) => a.startsWith(`--${name}=`))
  return hit ? hit.slice(name.length + 3) : dflt
}
const PORT = Number(opt('attach', '9223'))
const DB = opt('db', '')
if (!DB) {
  console.error('用法：node tools/verify.mjs --attach=9223 --db="<路径>/app.db"')
  process.exit(2)
}

const DAY = 86_400_000
const HEATMAP_WEEKS = 15
const BAR_DAYS = 30
const TAG_DAYS = 90

const problems = []
const checks = []
function check(name, ok, detail = '') {
  checks.push({ name, ok })
  if (!ok) problems.push(`${name}${detail ? ` —— ${detail}` : ''}`)
}
function eq(name, a, b) {
  check(name, JSON.stringify(a) === JSON.stringify(b), `期望 ${JSON.stringify(b)}，拿到 ${JSON.stringify(a)}`)
}

const dayIntOf = (ms) => {
  const d = new Date(ms)
  return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate()
}
const parts = (day) => [Math.floor(day / 10000), Math.floor(day / 100) % 100, day % 100]
const shiftDay = (day, by) => {
  const [y, m, d] = parts(day)
  return dayIntOf(new Date(y, m - 1, d + by).getTime())
}
const weekdayOf = (day) => {
  const [y, m, d] = parts(day)
  const g = new Date(y, m - 1, d).getDay()
  return g === 0 ? 7 : g
}
const daysBetween = (from, to) => {
  const a = parts(from)
  const b = parts(to)
  return Math.round((Date.UTC(b[0], b[1] - 1, b[2]) - Date.UTC(a[0], a[1] - 1, a[2])) / DAY)
}

const db = new DatabaseSync(DB, { readOnly: true })
const one = (sql, ...p) => db.prepare(sql).get(...p)
const all = (sql, ...p) => db.prepare(sql).all(...p)

const wsUrl = await pollPage(PORT, { urlIncludes: ':1420', tries: 8 })
const cdp = await openCdp(wsUrl, 'ws 握手超时：原生要带 --remote-allow-origins=*')
const invoke = async (cmd, arg) =>
  JSON.parse(
    await cdp.eval(
      `__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(arg ?? {})}).then(v=>JSON.stringify(v))`,
      true,
    ),
  )

const report = await invoke('stats_report')
const dash = await invoke('dash_summary')
const listAll = await invoke('todos_all')
cdp.close()

// ---------- 1. 热力图 ----------
eq('热力图格数 = 15×7', report.heatmap.cells.length, HEATMAP_WEEKS * 7)
check('热力图从周一起', weekdayOf(report.heatmap.startDay) === 1, `startDay=${report.heatmap.startDay}`)
check(
  '热力图最后一列含今天',
  report.heatmap.endDay >= report.todayDay &&
    daysBetween(report.todayDay, report.heatmap.endDay) < 7,
  `end=${report.heatmap.endDay} today=${report.todayDay}`,
)
{
  // 独立算法：一格一条参数化 count，不用 GROUP BY
  let mismatch = 0
  for (const cell of report.heatmap.cells) {
    const n = one(
      'SELECT count(*) c FROM todos WHERE deleted_at IS NULL AND completed = 1 AND done_day = ?',
      BigInt(cell.day),
    ).c
    if (Number(n) !== cell.done) {
      if (mismatch < 3) console.log(`   格 ${cell.day}：图上 ${cell.done}，库里 ${n}`)
      mismatch += 1
    }
  }
  check('热力图每格与库一致', mismatch === 0, `${mismatch} 格对不上`)
  eq(
    '热力图活跃天数',
    report.heatmap.activeDays,
    report.heatmap.cells.filter((c) => c.done > 0).length,
  )
}
{
  // 连续天数：从最近一个有记录的那天往回数，用 JS 独立走一遍
  const days = all(
    'SELECT DISTINCT done_day d FROM todos WHERE deleted_at IS NULL AND completed = 1 AND done_day IS NOT NULL',
  )
    .map((r) => Number(r.d))
    .filter((d) => d <= report.todayDay)
    .sort((a, b) => a - b)
  const set = new Set(days)
  let streak = 0
  let through = null
  if (days.length) {
    through = days.at(-1)
    streak = 1
    let cursor = through
    while (set.has(shiftDay(cursor, -1))) {
      cursor = shiftDay(cursor, -1)
      streak += 1
    }
  }
  eq('连续天数', report.heatmap.streak, streak)
  eq('连续止于哪天', report.heatmap.streakThrough, through)
}

// ---------- 2. 每日柱状图 ----------
{
  eq('柱状图根数 = 30', report.bars.length, BAR_DAYS)
  const first = report.bars[0]?.day
  const last = report.bars.at(-1)?.day
  eq('柱状图窗口右端是今天', last, report.todayDay)
  eq('柱状图窗口左端', first, shiftDay(report.todayDay, -(BAR_DAYS - 1)))

  // 独立算法：一次性把行取回来，用 JS 的 Date 折本地日（Rust 侧是 chrono）
  const rows = all(
    'SELECT done_day d, due_at due FROM todos WHERE deleted_at IS NULL AND completed = 1',
  )
  const mine = new Map(report.bars.map((b) => [b.day, { onTime: 0, late: 0, noDue: 0 }]))
  for (const r of rows) {
    const day = Number(r.d)
    const slot = mine.get(day)
    if (!slot) continue
    if (r.due == null) slot.noDue += 1
    else if (day <= dayIntOf(Number(r.due))) slot.onTime += 1
    else slot.late += 1
  }
  const diff = report.bars.filter((b) => {
    const m = mine.get(b.day)
    return !m || m.onTime !== b.onTime || m.late !== b.late || m.noDue !== b.noDue
  })
  check('每根柱的三档都与库一致', diff.length === 0, `${diff.length} 根对不上`)

  // 与**列表**一致：走的是另一条命令（todos_all），不是聚合命令
  const listed = listAll.filter((t) => t.completed && t.doneDay != null && t.doneDay >= first)
  const listedTotal = listed.length
  const chartTotal = report.bars.reduce((a, b) => a + b.onTime + b.late + b.noDue, 0)
  eq('柱状图总和 = 待办列表里数出来的条数', chartTotal, listedTotal)
}

// ---------- 3. 今日环 ----------
{
  eq(
    '环的「还剩」= 驾驶舱 TODAY 卡',
    report.gauge.open,
    dash.today.value,
  )
  eq('环的「已清」= 驾驶舱 DONE 卡', report.gauge.done, dash.done.value)

  const todayStart = new Date().setHours(0, 0, 0, 0)
  const todayEnd = new Date().setHours(23, 59, 59, 999)
  const open = Number(
    one(
      'SELECT count(*) c FROM todos WHERE deleted_at IS NULL AND completed = 0 AND ' +
        '(due_at BETWEEN ? AND ? OR (zone = ? AND due_at IS NULL))',
      BigInt(todayStart),
      BigInt(todayEnd),
      'today',
    ).c,
  )
  eq('环的「还剩」与独立 SQL 一致', report.gauge.open, open)
  const done = Number(
    one(
      'SELECT count(*) c FROM todos WHERE deleted_at IS NULL AND completed = 1 AND done_day = ?',
      BigInt(report.todayDay),
    ).c,
  )
  eq('环的「已清」与独立 SQL 一致', report.gauge.done, done)
}

// ---------- 4. 标签横条 ----------
{
  const from = shiftDay(report.todayDay, -(TAG_DAYS - 1))
  // 独立写法：LEFT JOIN … IS NULL 代替 NOT EXISTS
  const untagged = Number(
    one(
      `SELECT count(*) c FROM todos d
       LEFT JOIN todo_tag_links l ON l.todo_id = d.id AND l.deleted_at IS NULL
       LEFT JOIN tags t ON t.id = l.tag_id AND t.deleted_at IS NULL
       WHERE d.deleted_at IS NULL AND d.completed = 1 AND d.done_day >= ? AND t.id IS NULL`,
      BigInt(from),
    ).c,
  )
  const gotUntagged = report.tags.find((t) => t.tag === '无标签')?.done ?? 0
  eq('「无标签」那一档与库一致', gotUntagged, untagged)

  const perTag = all(
    `SELECT t.name n, count(*) c FROM todos d
     JOIN todo_tag_links l ON l.todo_id = d.id AND l.deleted_at IS NULL
     JOIN tags t ON t.id = l.tag_id AND t.deleted_at IS NULL
     WHERE d.deleted_at IS NULL AND d.completed = 1 AND d.done_day >= ?
     GROUP BY t.name ORDER BY c DESC, t.name ASC LIMIT ?`,
    BigInt(from),
    BigInt(8),
  ).map((r) => ({ tag: r.n, done: Number(r.c) }))
  eq(
    '前 8 个标签的条数与库一致',
    report.tags.filter((t) => t.tag !== '无标签'),
    perTag,
  )

  const total = Number(
    one(
      'SELECT count(*) c FROM todos WHERE deleted_at IS NULL AND completed = 1 AND done_day >= ?',
      BigInt(from),
    ).c,
  )
  eq('近 90 天完成总数与库一致', report.tagsTotal, total)
  const listed = listAll.filter((t) => t.completed && t.doneDay != null && t.doneDay >= from)
  eq('近 90 天总数 = 列表里数出来的条数', report.tagsTotal, listed.length)
}

// ---------- 5. 时段直方图 ----------
{
  eq('直方图 24 根', report.hours.length, 24)
  let mismatch = 0
  for (const h of report.hours) {
    const n = Number(
      one(
        'SELECT count(*) c FROM todos WHERE deleted_at IS NULL AND created_hour = ?',
        BigInt(h.hour),
      ).c,
    )
    if (n !== h.created) {
      if (mismatch < 3) console.log(`   ${h.hour}:00 图上 ${h.created}，库里 ${n}`)
      mismatch += 1
    }
  }
  check('每根时段柱与库一致', mismatch === 0, `${mismatch} 根对不上`)

  const firstDay = one(
    'SELECT min(created_day) d FROM todos WHERE deleted_at IS NULL',
  ).d
  const history = firstDay == null ? 0 : daysBetween(Number(firstDay), report.todayDay) + 1
  eq('记了多少天', report.historyDays, history)
  eq(
    '时段图的门槛判定',
    report.hoursEnough,
    history >= report.hourMinDays,
    `${history} 天 / 门槛 ${report.hourMinDays}`,
  )
}

// ---------- 6. 完成记录存在性 ----------
{
  const anyDone = Number(
    one('SELECT count(*) c FROM todos WHERE deleted_at IS NULL AND completed = 1').c,
  )
  eq('有没有完成记录', report.hasActivity, anyDone > 0)
}

const passed = checks.filter((c) => c.ok).length
db.close()
console.log(`\n逐图核对：${passed}/${checks.length} 项一致`)
if (problems.length) {
  console.error(`\n对不上的 ${problems.length} 项：\n  ` + problems.join('\n  '))
  process.exit(1)
}
console.log('统计页上的每个数字都能被一条写法不同的查询复算出来。')
