import type {
  Api,
  BackupLogRow,
  BackupOutcome,
  BackupStatus,
  ClassRow,
  Countdown,
  CountdownMode,
  DashSummary,
  HotkeyItem,
  MedicineTodayPayload,
  Note,
  NoteFolder,
  PaletteHit,
  PathsView,
  QueueGroup,
  RemoteFile,
  Resource,
  SettingsView,
  SnapshotView,
  SourcePaths,
  StatsReport,
  Sticky,
  Suggestion,
  SysLoad,
  Todo,
  TodoDraft,
  TodoPatch,
  TodoZone,
  TimetableImportReport,
  TimetableState,
  Zone,
} from './types'
import { DEFAULT_COMBO, sameCombo } from '../lib/combo'
import { weekOf } from '../lib/timetable'

/**
 * 只在浏览器里跑（`npm run dev` + `?mock=1`）时用的实现。
 *
 * 它存在的唯一理由：M2/M5/M7 的验收是纯视觉判据（断点、选中态对比度、同屏 accent 数），
 * 需要能任意改视口尺寸反复截图。行为正确性由 cargo test 负责，不用这里证明。
 *
 * 但语义要跟 Rust 一致，否则「看着对」是假的：rev 每次写入 +1、取消勾选只翻 completed
 * 而不清 done_at、删除是打墓碑、排序取邻居中点。这些规则在这里各写一遍，是因为它们同时也是渲染前提。
 * 两边对不上时以 Rust 为准改这里。
 */

const paths: PathsView = {
  root: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil`,
  db: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\app.db`,
  icons: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\icons`,
  logs: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\logs`,
  backups: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\backups`,
  deviceId: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\device.id`,
  journalMode: 'wal',
}

const DAY = 86_400_000
const HOUR = 3_600_000
const MINUTE = 60_000
const GAP = 1024

let counter = 0
const uid = () => `mock-${(counter += 1).toString().padStart(4, '0')}`

const now = () => Date.now()

// `paths.deviceId` 是**文件路径**（原则 4 要让人看见它在哪）；同步列里的来源设备是 id 本身。
const DEVICE_ID = 'pc-0196e4b3a1c27d80a9b0c1d2e3f40000'

function dayInt(ms: number): number {
  const d = new Date(ms)
  return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate()
}

function makeTodo(part: Partial<Todo>): Todo {
  const created = part.createdAt ?? now()
  return {
    id: part.id ?? uid(),
    rev: part.rev ?? 1,
    createdAt: created,
    createdBy: DEVICE_ID,
    updatedAt: part.updatedAt ?? created,
    updatedBy: DEVICE_ID,
    deletedAt: null,
    title: part.title ?? '',
    note: part.note ?? '',
    doneAt: part.doneAt ?? null,
    doneDay: part.doneDay ?? null,
    // 种子数据里凡是带完成时间的就是「当前仍完成」；取消勾选的例子由 todosSetDone 演
    completed: part.completed ?? part.doneAt != null,
    dueAt: part.dueAt ?? null,
    remindAt: part.remindAt ?? null,
    priority: part.priority ?? 2,
    pinned: part.pinned ?? false,
    parentId: part.parentId ?? null,
    sortKey: part.sortKey ?? GAP,
    zone: part.zone ?? 'inbox',
    createdDay: dayInt(created),
    createdHour: new Date(created).getHours(),
    tags: part.tags ?? [],
  }
}

const seedTodos = (): Todo[] => {
  const t = now()
  return [
    makeTodo({ title: '把 M2 的验收跑完', zone: 'today', priority: 1, dueAt: t + 2 * HOUR, tags: ['工作'] }),
    makeTodo({ title: '回复房东的微信', zone: 'today', dueAt: t - 3 * HOUR, tags: ['家里'] }),
    makeTodo({ title: '预约牙医', zone: 'inbox', dueAt: t - 2 * DAY, priority: 0, note: '下周三之前' }),
    makeTodo({ title: '整理备份目录', zone: 'later', pinned: true, sortKey: GAP }),
    makeTodo({ title: '买猫粮（大袋）', zone: 'today', doneAt: t - 4 * HOUR, doneDay: dayInt(t - 4 * HOUR) }),
    makeTodo({ title: '写周报', zone: 'inbox' }),
    // 环要 ≥ 5 面才看得出是环（见 FocusRing 的 MIN_FACES），所以种子数据必须铺够焦点项，
    // 否则每次截图验到的都是降级形态，环本体等于没验。
    makeTodo({ title: '给 x-hub 提同步 issue', zone: 'today', priority: 2, dueAt: t + 5 * HOUR }),
    makeTodo({ title: '把药箱的补买提醒接上', zone: 'today', priority: 1, dueAt: t - 26 * HOUR }),
    makeTodo({ title: '重写数据位置这块文案', zone: 'today', dueAt: t + 90 * MINUTE }),
    makeTodo({ title: '统计视图的图先定选型', zone: 'later', priority: 1, dueAt: t - 4 * DAY, tags: ['工作'] }),
    ...seedHistory(t),
  ]
}

/**
 * 过去 90 天的完成记录：让时间轴与统计页有真实形状可看。
 * 天数刻意跨过 60 天门槛，这样「不足 60 天不画趋势线」那条降级也能反过来验到。
 *
 * 三种形状都要有 —— 按时、补做（截止在前一天）、压根没设截止，外加约三分之一挂标签。
 * 否则柱状图的三档堆叠与标签条的「无标签」档只有一档有东西，截图看着满、其实什么也没测到。
 */
const HISTORY_TAGS = ['工作', '家里', '跑腿', '电脑']

function seedHistory(t: number): Todo[] {
  const out: Todo[] = []
  for (let i = 1; i <= 90; i++) {
    // 周末做得少，工作日做得多：柱状图与热力图才不至于是一片均匀的噪声
    const dow = new Date(t - i * DAY).getDay()
    const n = dow === 0 || dow === 6 ? (i % 4 === 0 ? 1 : 0) : i % 3 === 0 ? 3 : i % 5 === 0 ? 0 : 2
    for (let k = 0; k < n; k++) {
      const at = t - i * DAY + (9 + k * 4) * HOUR
      const midnight = new Date(at)
      midnight.setHours(0, 0, 0, 0)
      const shape = (i + k) % 5
      const dueAt = shape === 4 ? null : shape === 3 ? at - DAY : at + 6 * HOUR
      const createdAt = midnight.getTime() - 2 * DAY + (7 + ((i * 3) % 14)) * HOUR
      out.push(
        makeTodo({
          title: `完成于 ${String(dayInt(at)).slice(4, 6)}/${String(dayInt(at)).slice(6, 8)} 的第 ${k + 1} 项`,
          zone: 'today',
          createdAt,
          doneAt: at,
          doneDay: dayInt(at),
          dueAt,
          tags: (i + k) % 3 === 0 ? [HISTORY_TAGS[(i + k) % HISTORY_TAGS.length]] : [],
        }),
      )
    }
  }
  return out
}

const seedZones = (): Zone[] => [
  { id: 'z-dev', name: '开发', sort: GAP, itemCount: 2 },
  { id: 'z-life', name: '生活', sort: GAP * 2, itemCount: 1 },
]

const seedResources = (): Resource[] => {
  const t = now()
  const base = (part: Partial<Resource>): Resource => ({
    id: part.id ?? uid(),
    rev: 1,
    createdAt: t - 5 * DAY,
    createdBy: DEVICE_ID,
    updatedAt: t,
    updatedBy: DEVICE_ID,
    kind: 'app',
    name: '',
    target: '',
    args: '',
    iconPath: null,
    zoneId: null,
    useCount: 0,
    lastLaunchedAt: null,
    note: '',
    ...part,
  })
  return [
    base({ name: 'VS Code', target: 'C:\\Tools\\Code.exe', zoneId: 'z-dev', useCount: 41, lastLaunchedAt: t - HOUR }),
    base({ name: '药箱', target: 'C:\\Tools\\medicine.exe', zoneId: 'z-life', useCount: 12, lastLaunchedAt: t - DAY }),
    base({ name: '博客仓库', target: 'D:\\ADevelop\\Repositories\\Mine\\Rewmington-Blog-main', kind: 'file', useCount: 3 }),
    base({ name: 'wmddd.online', target: 'https://wmddd.online', kind: 'web', zoneId: 'z-dev', useCount: 7 }),
  ]
}

interface Store {
  todos: Todo[]
  resources: Resource[]
  zones: Zone[]
  secrets: Record<string, string>
  settings: Record<string, unknown>
  backup: BackupSeed
  medicine: MedicineSeed
  stickies: Sticky[]
  countdowns: Countdown[]
  timetable: TimetableSeed
  notes: Note[]
  folders: NoteFolder[]
}

/** 备份在浏览器里只演「状态机」：配没配凭据、流水、远端清单。加解密链路归 cargo test。 */
interface BackupSeed {
  email: string | null
  hasPass: boolean
  log: BackupLogRow[]
  remote: RemoteFile[]
}

/**
 * 药箱云端备用源同样只演状态：邮箱、有没有应用密码、目录地址。
 * 真的去 GET 坚果云由 cargo test 负责，这里连不上也不该装出连上了。
 */
interface MedicineSeed {
  email: string | null
  hasPass: boolean
  davDir: string
  /** 药箱库在这台机器上的位置；null = 用默认路径 */
  dbPath: string | null
}

const MEDICINE_DAV_DEFAULT = 'https://dav.jianguoyun.com/dav/medicine-sync/'
const MEDICINE_EMAIL_KEY = 'linked.medicine.email'
const MEDICINE_DB_DEFAULT = String.raw`C:\Users\hp\AppData\Roaming\dev.mine\medicine_tracker\medicine_tracker.db`

function freshMedicine(): MedicineSeed {
  return { email: null, hasPass: false, davDir: MEDICINE_DAV_DEFAULT, dbPath: null }
}

function freshBackup(): BackupSeed {
  const t = Date.now()
  return {
    email: 'me@wmddd.online',
    hasPass: true,
    remote: [
      { name: abackupName(new Date(t - 4 * 60_000), 'a1b2c3d4'), size: 138_112, readable: true },
      { name: abackupName(new Date(t - 26 * 3_600_000), '9f8e7d6c'), size: 137_024, readable: true },
      { name: abackupName(new Date(t - 50 * 3_600_000), '5e4d3c2b'), size: 132_096, readable: true },
      // 药箱在同一个账号里的文件：real 侧永远列不出来，这里也不列
    ],
    log: [
      { at: t - 26 * 3_600_000, ok: false, bytes: null, remotePath: null, error: '坚果云请求失败：dns error: 没有达到要求的名称服务器', kind: 'daily' },
      { at: t - 26 * 3_600_000 + 61_000, ok: true, bytes: 137_024, remotePath: abackupName(new Date(t - 26 * 3_600_000), '9f8e7d6c'), error: null, kind: 'daily' },
      { at: t - 4 * 60_000, ok: true, bytes: 138_112, remotePath: abackupName(new Date(t - 4 * 60_000), 'a1b2c3d4'), error: null, kind: 'manual' },
    ],
  }
}

// 与 Rust 的 file_name() 同格式：anvil-<UTC 时间戳>-<8 hex>.abackup
function abackupName(d: Date, tag: string): string {
  return `anvil-${d.toISOString().replace(/[-:]/g, '').replace(/\.\d+Z$/, 'Z')}-${tag}.abackup`
}

function seedNotes(): Pick<Store, 'notes' | 'folders'> {
  const t = now()
  const folders: NoteFolder[] = [
    {
      id: 'f-work', rev: 1, createdAt: t - 20 * DAY, createdBy: DEVICE_ID, updatedAt: t - 20 * DAY,
      updatedBy: DEVICE_ID, deletedAt: null, name: '工程', parentId: null, sort: GAP, noteCount: 2,
    },
    {
      id: 'f-life', rev: 1, createdAt: t - 18 * DAY, createdBy: DEVICE_ID, updatedAt: t - 18 * DAY,
      updatedBy: DEVICE_ID, deletedAt: null, name: '生活', parentId: null, sort: GAP * 2, noteCount: 1,
    },
    {
      id: 'f-anvil', rev: 1, createdAt: t - 17 * DAY, createdBy: DEVICE_ID, updatedAt: t - 17 * DAY,
      updatedBy: DEVICE_ID, deletedAt: null, name: 'Anvil', parentId: 'f-work', sort: GAP, noteCount: 1,
    },
  ]
  const make = (part: Partial<Note> & { id: string; title: string; bodyMd: string }): Note => ({
    rev: 1,
    createdAt: t - 3 * DAY,
    createdBy: DEVICE_ID,
    updatedAt: t - 2 * HOUR,
    updatedBy: DEVICE_ID,
    deletedAt: null,
    folderId: null,
    pinned: false,
    trashedAt: null,
    createdDay: dayInt(t - 3 * DAY),
    tags: [],
    ...part,
  })
  const notes: Note[] = [
    make({
      id: 'n-1',
      title: '候选一的实施要点',
      folderId: 'f-anvil',
      pinned: true,
      tags: ['设计', 'v1'],
      updatedAt: t - 30 * 60_000,
      bodyMd: [
        '## 卡片环的三条降级',
        '',
        '- 面数 < 5 → 横向 scroll-snap 列表',
        '- 0 面 → 折叠成一行虚线条',
        '- 失焦 / 最小化 / 降低动效 → `cancelAnimationFrame`',
        '',
        '> 动画只碰 `transform`。环与其卡上不许出现滤镜与模糊类属性，也不许给阴影加过渡',
        '> —— 那正是参考项目把 GPU 吃到 26% 的来源。',
        '',
        '```ts',
        'const live = faces >= 5 && visible && focused && !reduced',
        '```',
        '',
        '| 情形 | 该不该转 |',
        '| ---- | -------- |',
        '| 前台 | 转 |',
        '| 收进托盘 | 不转 |',
      ].join('\n'),
    }),
    make({
      id: 'n-2',
      title: '同步字段的取舍',
      folderId: 'f-work',
      tags: ['数据层'],
      updatedAt: t - 5 * HOUR,
      bodyMd: [
        '两台设备、编辑面基本不相交，**行级 LWW 够了**，不引 CRDT。',
        '',
        '1. `rev` 每次写入 +1',
        '2. 平票比 `updated_by` 字典序',
        '3. 手机只做追加与勾选，不做段落级并发改写',
        '',
        '链接测试：[官网](https://wmddd.online) 与一个不该放的 [坑](javascript:alert(1))。',
      ].join('\n'),
    }),
    make({
      id: 'n-3',
      title: '猫粮清单',
      folderId: 'f-life',
      updatedAt: t - 8 * HOUR,
      bodyMd: '- [x] 问店家有没有冻干\n- [ ] 大袋趁活动囤两包\n',
    }),
    make({
      id: 'n-4',
      title: '写废的一稿',
      updatedAt: t - 9 * DAY,
      trashedAt: t - 2 * DAY,
      bodyMd: '正文留着，还原后一字不差。\n',
    }),
    make({
      id: 'n-5',
      title: '没有文件夹的散记',
      updatedAt: t - 26 * HOUR,
      bodyMd: '顶层笔记。左侧选「全部」时能看到它，选某个文件夹时不该出现。\n',
    }),
  ]
  return { notes, folders }
}

const KEY = 'anvil.mock.store.v2'

/** `?mock=1&empty=1`：把种子数据清空，用来验 §9 M5 那条「空/满两态都不出现死白」。 */
const EMPTY = new URLSearchParams(location.search).has('empty')

/**
 * `?mock=1&hotkey=Ctrl+Alt+C`：把唤起键的初始绑定换成指定值。
 * 用来演「存着但没挂上」那一格 —— 它的真实来源是换机或恢复快照后撞上别人，
 * 界面上没有任何操作能造出这个状态，所以只能从种子数据进来。
 */
const SEEDED_HOTKEY = new URLSearchParams(location.search).get('hotkey')

const HOTKEY_ACTION = 'show'
const SETTINGS_HOTKEYS = 'hotkeys'
const SETTINGS_AUTOSTART = 'autostart'

/**
 * 演「这个组合键已经被别的程序拿走了」。
 *
 * 真实名单只有内核知道（Windows 没有查询归属的 API），所以浏览器里只能挑几个
 * 确实常被占用的组合来演示那条失败分支 —— 它的价值是让「未挂上」这一格能被看见，
 * 不是假装还原了本机实况。
 */
const MOCK_TAKEN = ['Ctrl+Alt+C', 'Alt+Shift+Q', 'Ctrl+Shift+Space']

function seedStickies(): Sticky[] {
  const t = now()
  const base = (part: Partial<Sticky>): Sticky => ({
    id: part.id ?? uid(),
    rev: 1,
    createdAt: t - 2 * DAY,
    createdBy: DEVICE_ID,
    updatedAt: t - 30 * 60_000,
    updatedBy: DEVICE_ID,
    deletedAt: null,
    slot: 1,
    content: '',
    ...part,
  })
  return [
    base({ slot: 1, content: '猫粮快见底了，顺便问店家有没有冻干。\n周五之前。' }),
    base({ slot: 2, content: '' }),
  ]
}

function seedCountdowns(): Countdown[] {
  const t = now()
  const base = (part: Partial<Countdown>): Countdown => ({
    id: part.id ?? uid(),
    rev: 1,
    createdAt: t - DAY,
    createdBy: DEVICE_ID,
    updatedAt: t,
    updatedBy: DEVICE_ID,
    deletedAt: null,
    label: '',
    mode: 'duration',
    spec: {},
    endsAt: null,
    paused: false,
    state: 'idle',
    remainingMs: null,
    ...part,
  })
  return [
    base({ label: '健身', mode: 'duration', spec: { minutes: 45, startedAt: t - 10 * MINUTE }, state: 'running', endsAt: t + 35 * MINUTE }),
    base({ label: '喝水', mode: 'interval', spec: { everyMinutes: 45, startedAt: t - 2 * HOUR }, state: 'running', endsAt: t + 30 * MINUTE }),
    base({ label: '番茄钟', mode: 'duration', spec: { minutes: 25, startedAt: t - 20 * MINUTE }, state: 'paused', paused: true, remainingMs: 5 * MINUTE }),
    base({ label: '明早吃药', mode: 'daily', spec: { at: '08:30' }, state: 'idle' }),
  ]
}

function freshStore(): Store {
  if (EMPTY) {
    return { todos: [], resources: [], zones: [], secrets: {}, settings: {}, backup: { email: null, hasPass: false, log: [], remote: [] }, medicine: freshMedicine(), timetable: freshTimetable(), stickies: [], countdowns: [], notes: [], folders: [] }
  }
  return {
    todos: seedTodos(),
    resources: seedResources(),
    zones: seedZones(),
    secrets: {},
    settings: SEEDED_HOTKEY ? { [SETTINGS_HOTKEYS]: { [HOTKEY_ACTION]: SEEDED_HOTKEY } } : {},
    backup: freshBackup(),
    medicine: freshMedicine(),
    timetable: freshTimetable(),
    stickies: seedStickies(),
    countdowns: seedCountdowns(),
    ...seedNotes(),
  }
}

function load(): Store {
  try {
    const raw = localStorage.getItem(KEY)
    if (raw) return { ...freshStore(), ...(JSON.parse(raw) as Store) }
  } catch {
    /* 解析失败就用种子数据，别让一次坏写入卡死整个界面 */
  }
  return freshStore()
}

const store = load()

function save() {
  localStorage.setItem(KEY, JSON.stringify(store))
}

const live = (t: Todo) => t.deletedAt === null
const byId = (id: string) => store.todos.find((t) => t.id === id && live(t))

function touch(t: { rev: number; updatedAt: number; updatedBy: string }) {
  t.rev += 1
  t.updatedAt = now()
  t.updatedBy = DEVICE_ID
}

function dayStart(ms: number): number {
  const d = new Date(ms)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

/** 两个 yyyymmdd 之间隔了几个自然日。**不能整数相减**：20261029 - 20260930 = 99，真实是 29。 */
function daysBetween(fromDay: number, toDay: number): number {
  const parts = (n: number) => [Math.floor(n / 10000), Math.floor(n / 100) % 100, n % 100] as const
  const [ay, am, ad] = parts(fromDay)
  const [by, bm, bd] = parts(toDay)
  return Math.round((Date.UTC(by, bm - 1, bd) - Date.UTC(ay, am - 1, ad)) / DAY)
}

/** 队列与环共用的排序：置顶优先，然后优先级（0 最急），最后分数索引。 */
function byPinnedPriority(a: Todo, b: Todo): number {
  return (
    Number(b.pinned) - Number(a.pinned) || a.priority - b.priority || a.sortKey - b.sortKey
  )
}

let sysSampled = 0

function needCountdown(id: string): Countdown {
  const c = store.countdowns.find((v) => v.id === id)
  if (!c) throw new Error('这个倒计时不存在或已被删除')
  return c
}

function needNote(id: string): Note {
  const n = store.notes.find((v) => v.id === id && v.deletedAt === null)
  if (!n) throw new Error('这条笔记不存在或已被彻底删除')
  return n
}

function needFolder(id: string): NoteFolder {
  const f = store.folders.find((v) => v.id === id && v.deletedAt === null)
  if (!f) throw new Error('这个文件夹不存在或已被删除')
  return f
}

/**
 * 药箱库存不足 2 天。读的是和药箱视图同一份 fixture ——
 * 状态不是 ok 或压根没缓存时返回 null，绝不拿「读不到」当「库存充足」（§2 原则 5）。
 */
function medicineIfLow(): Suggestion | null {
  const snap = medicineFixture()
  if (snap.status !== 'ok') return null
  const payload = JSON.parse(snap.payloadJson) as MedicineTodayPayload
  const worst = payload.items
    .filter((i) => i.daysLeft != null && i.daysLeft < 2)
    .sort((a, b) => (a.daysLeft ?? 0) - (b.daysLeft ?? 0))[0]
  if (!worst) return null
  return {
    rule: 'medicine-low',
    headline: `${worst.name} 快吃完了`,
    detail: `库存约够 ${worst.daysLeft?.toFixed(1)} 天，可以生成一条「补买」待办`,
    todoId: null,
    view: 'medicine',
  }
}

// 与 Rust 的 deadline_for 同一套折算规则。
function cdDeadline(c: Countdown, t: number): number | null {
  const s = c.spec
  if (c.mode === 'duration') return s.startedAt != null && s.minutes != null ? s.startedAt + s.minutes * MINUTE : null
  if (c.mode === 'timed') return s.until ?? null
  if (c.mode === 'interval') {
    if (s.startedAt == null || !s.everyMinutes) return null
    const every = s.everyMinutes * MINUTE
    if (t < s.startedAt) return s.startedAt + every
    return s.startedAt + Math.floor((t - s.startedAt) / every) * every + every
  }
  if (c.mode === 'daily') {
    const m = /^(\d{1,2}):(\d{2})$/.exec(s.at ?? '')
    if (!m) return null
    const at = dayStart(t) + (+m[1] * 60 + +m[2]) * MINUTE
    return at > t ? at : at + DAY
  }
  return null
}

function sortedInZone(zone: TodoZone): Todo[] {
  return store.todos
    .filter((t) => live(t) && t.zone === zone)
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.sortKey - b.sortKey)
}

function nextSortKey(zone: TodoZone): number {
  const list = sortedInZone(zone)
  return list.length ? Math.max(...list.map((t) => t.sortKey)) + GAP : GAP
}

function descendantsOf(id: string): Todo[] {
  const out: Todo[] = []
  const queue = [id]
  while (queue.length) {
    const current = queue.pop() as string
    for (const child of store.todos.filter((t) => t.parentId === current && live(t))) {
      out.push(child)
      queue.push(child.id)
    }
  }
  return out
}

function comboOf(): string {
  const raw = store.settings[SETTINGS_HOTKEYS]
  const map = raw && typeof raw === 'object' ? (raw as Record<string, string>) : {}
  return map[HOTKEY_ACTION] || DEFAULT_COMBO
}

function mockHotkey(action: string, combo: string): HotkeyItem {
  return {
    action,
    label: '唤起窗口',
    combo,
    registered: !MOCK_TAKEN.some((t) => sameCombo(t, combo)),
  }
}

function assertCombo(combo: string): string {
  const next = combo.trim()
  // 与 Rust 侧同一条规则：至少一个修饰键 + 一个主键，主键在最后
  if (next.split('+').filter(Boolean).length < 2) throw new Error(`组合键格式不认识：${next}`)
  return next
}

// 统计页的窗口与门槛，逐条对齐 `repos/stats.rs` 的常量。
const HEATMAP_WEEKS = 15
const BAR_DAYS = 30
const TAG_DAYS = 90
const HOUR_MIN_DAYS = 30
const TAG_LIMIT = 8

const splitDay = (day: number): [number, number, number] => [
  Math.floor(day / 10000),
  Math.floor(day / 100) % 100,
  day % 100,
]

/** 加减自然日。整数相加会造出 `20261033` 这种日子，所以走 `Date` 再折回整数。 */
function shiftDay(day: number, by: number): number {
  const [y, m, d] = splitDay(day)
  return dayInt(new Date(y, m - 1, d + by).getTime())
}

/** 1 = 周一 … 7 = 周日，与 Rust 的 `days::weekday` 同一约定。 */
function weekdayOf(day: number): number {
  const [y, m, d] = splitDay(day)
  const g = new Date(y, m - 1, d).getDay()
  return g === 0 ? 7 : g
}

function mockStats(): StatsReport {
  const live = store.todos.filter((t) => t.deletedAt === null)
  const done = live.filter((t) => t.completed && t.doneDay != null)
  const todayDay = dayInt(Date.now())
  const today0 = new Date().setHours(0, 0, 0, 0)
  const today1 = new Date().setHours(23, 59, 59, 999)

  const anchor = shiftDay(todayDay, -(HEATMAP_WEEKS * 7 - 7))
  const start = shiftDay(anchor, 1 - weekdayOf(anchor))
  const end = shiftDay(start, HEATMAP_WEEKS * 7 - 1)
  const byDay = new Map<number, number>()
  for (const t of done) byDay.set(t.doneDay as number, (byDay.get(t.doneDay as number) ?? 0) + 1)
  const cells = Array.from({ length: HEATMAP_WEEKS * 7 }, (_, i) => {
    const day = shiftDay(start, i)
    return { day, done: byDay.get(day) ?? 0 }
  })

  const activeSet = [...new Set(done.map((t) => t.doneDay as number))].filter((d) => d <= todayDay)
  const activeSetSorted = activeSet.sort((a, b) => a - b)
  const last = activeSetSorted.at(-1)
  let streak = 0
  if (last != null) {
    streak = 1
    let cursor = last
    while (activeSetSorted.includes(shiftDay(cursor, -1))) {
      cursor = shiftDay(cursor, -1)
      streak += 1
    }
  }

  const bars = Array.from({ length: BAR_DAYS }, (_, i) => ({
    day: shiftDay(todayDay, -(BAR_DAYS - 1) + i),
    onTime: 0,
    late: 0,
    noDue: 0,
  }))
  const barIndex = new Map(bars.map((b, i) => [b.day, i]))
  for (const t of done) {
    const i = barIndex.get(t.doneDay as number)
    if (i === undefined) continue
    if (t.dueAt == null) bars[i].noDue += 1
    else if ((t.doneDay as number) <= dayInt(t.dueAt)) bars[i].onTime += 1
    else bars[i].late += 1
  }

  const windowFrom = shiftDay(todayDay, -(TAG_DAYS - 1))
  const inWindow = done.filter((t) => (t.doneDay as number) >= windowFrom)
  const tagCount = new Map<string, number>()
  let untagged = 0
  for (const t of inWindow) {
    if (t.tags.length === 0) untagged += 1
    for (const name of t.tags) tagCount.set(name, (tagCount.get(name) ?? 0) + 1)
  }
  const tags = [...tagCount.entries()]
    .sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
    .slice(0, TAG_LIMIT)
    .map(([tag, n]) => ({ tag, done: n }))
  if (untagged > 0) tags.push({ tag: '无标签', done: untagged })

  const hours = Array.from({ length: 24 }, (_, hour) => ({
    hour,
    created: live.filter((t) => t.createdHour === hour).length,
  }))

  const firstDay = live.reduce<number | null>(
    (min, t) => (t.createdDay != null && (min === null || t.createdDay < min) ? t.createdDay : min),
    null,
  )
  const historyDays = firstDay === null ? 0 : daysBetween(firstDay, todayDay) + 1

  return {
    todayDay,
    historyDays,
    heatmap: {
      cells,
      startDay: start,
      endDay: end,
      activeDays: cells.filter((c) => c.done > 0).length,
      streak,
      streakThrough: last ?? null,
    },
    bars,
    gauge: {
      open: live.filter(
        (t) =>
          !t.completed &&
          ((t.dueAt != null && t.dueAt >= today0 && t.dueAt <= today1) ||
            (t.zone === 'today' && t.dueAt == null)),
      ).length,
      done: done.filter((t) => t.doneDay === todayDay).length,
    },
    tags,
    tagsTotal: inWindow.length,
    hours,
    hoursEnough: historyDays >= HOUR_MIN_DAYS,
    hourMinDays: HOUR_MIN_DAYS,
    hasActivity: done.length > 0,
  }
}

/** 外部源的路径。云端那两个字段跟着 `store.medicine` 走，改完立刻能在界面上看到。 */
function mockPaths(): SourcePaths {
  return {
    blogRepo: String.raw`D:\ADevelop\Repositories\Mine\Rewmington-Blog-main`,
    blogRepoExists: true,
    medicineDb: store.medicine.dbPath ?? MEDICINE_DB_DEFAULT,
    medicineDbExists: true,
    medicineDav: store.medicine.davDir,
    medicineCloudReady: store.medicine.hasPass,
    onlineCheckEnabled: true,
    blogRepoDefault: String.raw`D:\ADevelop\Repositories\Mine\Rewmington-Blog-main`,
    medicineDbDefault: MEDICINE_DB_DEFAULT,
    medicineDavDefault: MEDICINE_DAV_DEFAULT,
  }
}

export const mockApi: Api = {
  paths: async () => paths,
  deviceId: async () => DEVICE_ID,
  settingsAll: async (): Promise<SettingsView> => ({ values: store.settings, paths, deviceId: DEVICE_ID }),
  settingsGet: async (key) => store.settings[key] ?? null,
  settingsSet: async (key, value) => {
    store.settings[key] = value
    save()
  },
  contextMenu: async () => {
    /* 浏览器里没有 Win32 菜单 */
  },
  openDataDir: async () => {
    /* 同上：这条在浏览器里无事可做，原生下会开资源管理器 */
  },
  openUrl: async () => {
    /* 同上 */
  },

  todosList: async (zone) => sortedInZone(zone).map((t) => ({ ...t })),
  todosAll: async () => store.todos.filter(live).map((t) => ({ ...t })),
  todosListUndone: async (zone) => sortedInZone(zone).filter((t) => !t.completed).map((t) => ({ ...t })),
  todosGet: async (id) => {
    const t = byId(id)
    return t ? { ...t } : null
  },
  todosChildren: async (id) => store.todos.filter((t) => t.parentId === id && live(t)).map((t) => ({ ...t })),
  todosCreate: async (draft: TodoDraft) => {
    const t = makeTodo({
      title: draft.title.trim(),
      note: draft.note ?? '',
      zone: draft.zone ?? 'inbox',
      priority: draft.priority ?? 2,
      dueAt: draft.dueAt ?? null,
      remindAt: draft.remindAt ?? null,
      parentId: draft.parentId ?? null,
      tags: [...new Set((draft.tags ?? []).map((s) => s.trim()).filter(Boolean))],
      sortKey: nextSortKey(draft.zone ?? 'inbox'),
    })
    store.todos.push(t)
    save()
    return { ...t }
  },
  todosUpdate: async (id, patch: TodoPatch) => {
    const t = byId(id)
    if (!t) throw new Error('这条待办不存在或已被删除')
    if (patch.title !== undefined) t.title = patch.title.trim()
    if (patch.note !== undefined) t.note = patch.note ?? ''
    if (patch.zone !== undefined) t.zone = patch.zone
    if (patch.priority !== undefined) t.priority = patch.priority
    if (patch.dueAt !== undefined) t.dueAt = patch.dueAt
    if (patch.remindAt !== undefined) t.remindAt = patch.remindAt
    if (patch.pinned !== undefined) t.pinned = patch.pinned
    if (patch.parentId !== undefined) t.parentId = patch.parentId
    touch(t)
    save()
    return { ...t }
  },
  todosSetDone: async (id, done) => {
    const t = byId(id)
    if (!t) throw new Error('这条待办不存在或已被删除')
    const ms = now()
    t.completed = done
    if (done) {
      t.doneAt = ms
      t.doneDay = dayInt(ms)
    }
    // 取消勾选只翻 completed：done_at / done_day 是「最后一次完成于何时」，不清空（§5.1）
    touch(t)
    save()
    return { ...t }
  },
  todosDelete: async (id) => {
    const t = byId(id)
    if (!t) throw new Error('这条待办不存在或已被删除')
    const affected = [t, ...descendantsOf(id)]
    const ms = now()
    for (const row of affected) {
      row.deletedAt = ms
      touch(row)
    }
    save()
    return affected.map((row) => row.id)
  },
  todosRestore: async (ids) => {
    for (const id of ids) {
      const t = store.todos.find((x) => x.id === id)
      if (t) {
        t.deletedAt = null
        touch(t)
      }
    }
    save()
  },
  todosReorder: async (id, prevId, nextId) => {
    const t = byId(id)
    if (!t) throw new Error('这条待办不存在')
    const prev = prevId ? byId(prevId)?.sortKey : undefined
    const next = nextId ? byId(nextId)?.sortKey : undefined
    if (prev === undefined && next === undefined) t.sortKey = GAP
    else if (prev !== undefined && next === undefined) t.sortKey = prev + GAP
    else if (prev === undefined && next !== undefined) t.sortKey = next - GAP
    else t.sortKey = (prev as number) + ((next as number) - (prev as number)) / 2
    touch(t)
    save()
  },
  todosSetTags: async (id, tags) => {
    const t = byId(id)
    if (!t) return
    t.tags = [...new Set(tags.map((s) => s.trim()).filter(Boolean))]
    touch(t)
    save()
  },

  resourcesList: async () =>
    [...store.resources].sort((a, b) => b.useCount - a.useCount || a.createdAt - b.createdAt).map((r) => ({ ...r })),
  resourceAddFromPath: async (path, zoneId) => {
    if (!path) throw new Error('文件不存在，可能被移动或删除了')
    const lower = path.toLowerCase()
    const ext = lower.slice(lower.lastIndexOf('.') + 1)
    const kind = ext === 'exe' || ext === 'lnk' ? 'app' : 'file'
    const name = path.replace(/^.*[\\/]/, '').replace(/\.[^.]+$/, '')
    const r: Resource = {
      id: uid(),
      rev: 1,
      createdAt: now(),
      createdBy: DEVICE_ID,
      updatedAt: now(),
      updatedBy: DEVICE_ID,
      kind,
      name,
      target: path,
      args: '',
      iconPath: null,
      zoneId,
      useCount: 0,
      lastLaunchedAt: null,
      note: '',
    }
    store.resources.push(r)
    save()
    return { ...r }
  },
  resourcePick: async () => null,
  resourceAddWeb: async (name, url, zoneId) => {
    const r: Resource = {
      id: uid(),
      rev: 1,
      createdAt: now(),
      createdBy: DEVICE_ID,
      updatedAt: now(),
      updatedBy: DEVICE_ID,
      kind: 'web',
      name,
      target: url,
      args: '',
      iconPath: null,
      zoneId,
      useCount: 0,
      lastLaunchedAt: null,
      note: '',
    }
    store.resources.push(r)
    save()
    return { ...r }
  },
  resourceLaunch: async (id) => {
    const r = store.resources.find((x) => x.id === id)
    if (!r) throw new Error('速达条目不存在')
    r.useCount += 1
    r.lastLaunchedAt = now()
    save()
    return 'started'
  },
  resourceRename: async (id, name) => {
    const r = store.resources.find((x) => x.id === id)
    if (r) r.name = name.trim()
    save()
  },
  resourceMove: async (id, zoneId) => {
    const r = store.resources.find((x) => x.id === id)
    if (r) r.zoneId = zoneId
    save()
  },
  resourceDelete: async (id) => {
    store.resources = store.resources.filter((x) => x.id !== id)
    save()
  },
  resourceSetSecret: async (id, text) => {
    store.secrets[id] = text
    save()
  },
  resourceRevealSecret: async (id) => store.secrets[id] ?? null,
  zonesList: async () => store.zones.map((z) => ({ ...z, itemCount: store.resources.filter((r) => r.zoneId === z.id).length })),
  zoneAdd: async (name) => {
    const z: Zone = { id: uid(), name: name.trim(), sort: (store.zones.length + 1) * GAP, itemCount: 0 }
    store.zones.push(z)
    save()
    return { ...z }
  },
  zoneDelete: async (id) => {
    store.zones = store.zones.filter((z) => z.id !== id)
    for (const r of store.resources) if (r.zoneId === id) r.zoneId = null
    save()
  },

  paletteSearch: async (query) => {
    const q = query.trim().toLowerCase()
    if (!q) return []
    const hits: PaletteHit[] = []
    for (const t of store.todos.filter(live)) {
      if (t.title.toLowerCase().includes(q) || t.note.toLowerCase().includes(q)) {
        hits.push({ kind: 'todo', id: t.id, label: t.title, sub: t.note.slice(0, 40) || null })
      }
    }
    for (const r of store.resources) {
      if (r.name.toLowerCase().includes(q) || r.target.toLowerCase().includes(q)) {
        hits.push({ kind: 'resource', id: r.id, label: r.name, sub: r.kind })
      }
    }
    return hits.slice(0, 16)
  },

  // ---- 外部联动：浏览器里读不到本机仓库，给一份与真实形状一致的夹具，只用于看版式 ----
  linkedState: async () => ({
    paths: mockPaths(),
    blog: blogFixtures(),
    medicine: medicineFixture(),
  }),
  linkedRefreshBlog: async () => blogFixtures(),
  linkedRefreshMedicine: async () => medicineFixture(),
  linkedCheckOnline: async () => blogFixtures(),
  linkedRestockTodo: async (name, daysLeft) =>
    mockApi.todosCreate({
      title: `补买 ${name}（${daysLeft == null ? '库存待确认' : `剩余 ${daysLeft} 天`}）`,
      zone: 'today',
      priority: 1,
      tags: ['跑腿'],
    }),
  linkedSetBlogDir: async (path) => ({ ...mockPaths(), blogRepo: path || mockPaths().blogRepo }),
  linkedSetMedicinePassword: async (email, password) => {
    if (!email.trim()) throw new Error('邮箱不能为空')
    if (!password.trim()) throw new Error('应用密码不能为空')
    store.medicine.email = email.trim()
    store.medicine.hasPass = true
    // 与 Rust 一样落在两处：密码进凭据管理器（这里只有 hasPass 这个旗子），邮箱进 settings
    store.settings[MEDICINE_EMAIL_KEY] = email.trim()
    save()
    return true
  },
  linkedClearMedicinePassword: async () => {
    store.medicine.email = null
    store.medicine.hasPass = false
    store.settings[MEDICINE_EMAIL_KEY] = null
    save()
    return false
  },
  linkedSetMedicineDb: async (path) => {
    // 与 Rust 的 normalize_dir 同口径：去首尾空白与引号，空串 = 恢复默认。
    // 相对路径展开那步需要进程 CWD，浏览器里没有，只能留给真后端验。
    const next = path.trim().replace(/"/g, '').trim()
    store.medicine.dbPath = next || null
    save()
    return mockPaths()
  },
  linkedSetMedicineDir: async (dir) => {
    // 与 Rust 的 ReadRemote::check_dir 同一条规则：只有反向钉子，不校验地址格式
    const next = dir.trim()
    if (next.includes('/workbench-backup/')) throw new Error('那是工作台自己的备份目录，不是药箱的同步目录')
    store.medicine.davDir = next || MEDICINE_DAV_DEFAULT
    save()
    return store.medicine.davDir
  },

  // ---- 备份：只演状态机，加解密与传输由 cargo test + tools/m4-check.mjs 负责 ----
  backupStatus: async (): Promise<BackupStatus> => {
    const b = store.backup
    // 与 Rust 的 read_status 同一条 ORDER BY at DESC：之前直接在升序种子上 find()，
    // 取到的是**最旧**一次成功，界面上就出现「上次成功 2 天前」压着「4 分钟前刚备份」。
    const desc = [...b.log].sort((x, y) => y.at - x.at)
    return {
      configured: !!b.email && b.hasPass,
      email: b.email,
      lastSuccessAt: desc.find((l) => l.ok)?.at ?? null,
      lastError: desc.find((l) => !l.ok)?.error ?? null,
      lastErrorAt: desc.find((l) => !l.ok)?.at ?? null,
      log: desc.slice(0, 20),
      paths,
      journalMode: paths.journalMode,
    }
  },
  backupNow: async (): Promise<BackupOutcome> => {
    const at = Date.now()
    if (!store.backup.email || !store.backup.hasPass) {
      return { at, ok: false, bytes: null, remotePath: null, error: '还没配好坚果云凭据，备份先不跑', skipped: true }
    }
    const bytes = 120_000 + Math.floor(Math.random() * 40_000)
    const remotePath = abackupName(new Date(at), Math.random().toString(16).slice(2, 10))
    store.backup.log.unshift({ at, ok: true, bytes, remotePath, error: null, kind: 'manual' })
    store.backup.remote.unshift({ name: remotePath, size: bytes, readable: true })
    save()
    return { at, ok: true, bytes, remotePath, error: null, skipped: false }
  },
  backupSaveCredentials: async (email, password): Promise<BackupStatus> => {
    if (!email.trim()) throw new Error('邮箱不能为空')
    if (!password.trim()) throw new Error('应用密码不能为空')
    store.backup.email = email.trim()
    store.backup.hasPass = true
    save()
    return mockApi.backupStatus()
  },
  backupClearCredentials: async (): Promise<BackupStatus> => {
    store.backup.email = null
    store.backup.hasPass = false
    save()
    return mockApi.backupStatus()
  },
  backupListRemote: async () => store.backup.remote.filter((f) => !f.name.startsWith('device-')),
  backupTestConnection: async () => store.backup.remote.length,
  backupRestore: async (name): Promise<BackupOutcome> => {
    const at = Date.now()
    if (name.startsWith('device-')) throw new Error('那是药箱的同步文件，不是工作台的备份')
    store.backup.log.unshift({ at, ok: true, bytes: null, remotePath: name, error: null, kind: 'restore' })
    save()
    return { at, ok: true, bytes: null, remotePath: name, error: null, skipped: false }
  },
  backupSelfTest: async (email, password) => !!email.trim() && !!password.trim(),

  // ---- M5 工作台：聚合口径与 Rust 的 repos/dashboard.rs 逐条对齐 ----
  //
  // 这里重算一遍是为了让浏览器能验版式；**正确性由 cargo test 拥有**（tests/dashboard.rs）。
  // 两边哪天对不上了，以 Rust 为准并改这里。
  dashSummary: async (): Promise<DashSummary> => {
    const t = now()
    const ds = dayStart(t)
    const de = ds + DAY - 1
    const live = store.todos.filter((x) => x.deletedAt === null)
    const undone = live.filter((x) => !x.completed)
    const kpi = (value: number, delta: number | null = null, note: string | null = null) => ({ value, delta, note })

    const today = undone.filter(
      (x) => (x.dueAt != null && x.dueAt >= ds && x.dueAt <= de) || (x.zone === 'today' && x.dueAt == null),
    ).length
    const inbox = undone.filter((x) => x.zone === 'inbox')
    const inboxWeek = inbox.filter((x) => (x.createdDay ?? 0) >= dayInt(t - 6 * DAY)).length
    const doneToday = live.filter((x) => x.completed && x.doneDay === dayInt(t)).length
    const doneYesterday = live.filter((x) => x.completed && x.doneDay === dayInt(t - DAY)).length
    const late = undone.filter((x) => x.dueAt != null && x.dueAt < ds)
    const oldest = late.reduce<number | null>((min, x) => (x.dueAt != null && (min == null || x.dueAt < min) ? x.dueAt : min), null)

    const focus = undone
      .filter((x) => x.parentId === null && ((x.dueAt != null && x.dueAt <= de) || x.zone === 'today'))
      .sort(byPinnedPriority)
      .slice(0, 8)

    const groups: QueueGroup[] = [
      { key: 'late', label: '逾期', items: [] },
      { key: 'today', label: '今天', items: [] },
      { key: 'later', label: '稍后', items: [] },
    ]
    for (const x of undone.filter((v) => v.parentId === null).sort(byPinnedPriority)) {
      const g = x.dueAt == null ? 'later' : x.dueAt < ds ? 'late' : x.dueAt <= de ? 'today' : 'later'
      groups.find((v) => v.key === g)!.items.push({ ...x })
    }

    const nowDate = new Date(t)
    const monthFrom = dayInt(new Date(nowDate.getFullYear(), nowDate.getMonth(), 1).getTime())
    const counts: Record<number, number> = {}
    for (const x of live) {
      if (x.completed && x.doneDay != null && x.doneDay >= monthFrom) counts[x.doneDay] = (counts[x.doneDay] ?? 0) + 1
    }
    const monthOpen = undone.filter((x) => x.dueAt != null && x.dueAt >= ds && dayInt(x.dueAt) >= monthFrom).length
    const firstDay = live.reduce<number | null>((min, x) => (x.createdDay != null && (min == null || x.createdDay < min) ? x.createdDay : min), null)

    return {
      today: kpi(today),
      inbox: kpi(inbox.length, null, inboxWeek > 0 ? `近 7 天新捕 ${inboxWeek} 条` : null),
      done: kpi(doneToday, doneToday - doneYesterday),
      late: kpi(
        late.length,
        null,
        oldest != null ? `最久的一条逾期 ${Math.max(0, daysBetween(dayInt(oldest), dayInt(t)))} 天` : null,
      ),
      focus: focus.map((x) => ({ ...x })),
      groups,
      timeline: Object.entries(counts)
        .map(([day, done]) => ({ day: Number(day), done }))
        .sort((a, b) => a.day - b.day),
      month: { done: doneToday, open: monthOpen },
      historyDays: firstDay == null ? 0 : daysBetween(firstDay, dayInt(t)) + 1,
    }
  },

  dashSuggestions: async (): Promise<Suggestion[]> => {
    const t = now()
    const ds = dayStart(t)
    const out: Suggestion[] = []
    const live = store.todos.filter((x) => x.deletedAt === null)
    const undone = live.filter((x) => !x.completed)

    const late = undone
      .filter((x) => x.dueAt != null && x.dueAt < ds)
      .sort((a, b) => (a.dueAt ?? 0) - (b.dueAt ?? 0))
    if (late[0]) {
      const n = Math.max(0, daysBetween(dayInt(late[0].dueAt!), dayInt(t)))
      out.push({
        rule: 'overdue-oldest',
        headline: late[0].title,
        detail: n > 0 ? `已逾期 ${n} 天，欠得最久的一条` : '今天已经过了截止时间',
        todoId: late[0].id,
        view: 'todos',
      })
    }
    const untouched = undone
      .filter((x) => x.dueAt != null && x.dueAt >= ds && x.dueAt <= ds + DAY - 1 && x.updatedAt === x.createdAt)
      .sort((a, b) => a.priority - b.priority)[0]
    if (untouched) {
      out.push({ rule: 'due-today-untouched', headline: untouched.title, detail: '今天到期，建完还没动过', todoId: untouched.id, view: 'todos' })
    }
    const soon = store.countdowns
      .filter((c) => c.state === 'running' && !c.paused && c.endsAt != null && c.endsAt > t && c.endsAt <= t + 2 * HOUR)
      .sort((a, b) => (a.endsAt ?? 0) - (b.endsAt ?? 0))[0]
    if (soon) {
      out.push({
        rule: 'countdown-short',
        headline: soon.label.trim() ? soon.label : '倒计时',
        detail: `只剩 ${Math.max(0, Math.floor(((soon.endsAt ?? 0) - t) / MINUTE))} 分`,
        todoId: null,
        view: 'progress',
      })
    }
    const med = medicineIfLow()
    if (med) out.push(med)
    const inboxN = undone.filter((x) => x.zone === 'inbox').length
    if (inboxN > 3) {
      out.push({ rule: 'inbox-pileup', headline: '整理捕获', detail: `${inboxN} 条还在收件箱里没定日子`, todoId: null, view: 'todos' })
    }
    return out.slice(0, 3)
  },

  statsReport: async () => mockStats(),
  stickiesList: async () => [...store.stickies].sort((a, b) => a.slot - b.slot).map((s) => ({ ...s })),
  stickySave: async (slot, content) => {
    const found = store.stickies.find((s) => s.slot === slot)
    if (found) {
      // 内容没变就不推 rev：防抖多发一次不该留下写入痕迹（与 Rust 同一条规则）
      if (found.content !== content) {
        found.content = content
        touch(found)
      }
      save()
      return { ...found }
    }
    const created: Sticky = {
      id: uid(),
      rev: 1,
      createdAt: now(),
      createdBy: DEVICE_ID,
      updatedAt: now(),
      updatedBy: DEVICE_ID,
      deletedAt: null,
      slot,
      content,
    }
    store.stickies.push(created)
    save()
    return { ...created }
  },
  stickyClear: async (slot) => {
    const i = store.stickies.findIndex((s) => s.slot === slot)
    if (i < 0) return false
    store.stickies.splice(i, 1)
    save()
    return true
  },

  countdownsList: async () => store.countdowns.map((c) => ({ ...c })),
  countdownAdd: async (label, mode, spec) => {
    const t = now()
    const created: Countdown = {
      id: uid(),
      rev: 1,
      createdAt: t,
      createdBy: DEVICE_ID,
      updatedAt: t,
      updatedBy: DEVICE_ID,
      deletedAt: null,
      label,
      mode: mode as CountdownMode,
      spec,
      endsAt: null,
      paused: false,
      state: 'idle',
      remainingMs: null,
    }
    store.countdowns.push(created)
    save()
    return { ...created }
  },
  countdownStart: async (id) => {
    const c = needCountdown(id)
    const t = now()
    if (c.mode === 'duration' || c.mode === 'interval') c.spec = { ...c.spec, startedAt: t }
    c.state = 'running'
    c.paused = false
    c.remainingMs = null
    c.endsAt = cdDeadline(c, t)
    touch(c)
    save()
    return { ...c }
  },
  countdownPause: async (id) => {
    const c = needCountdown(id)
    if (c.paused || c.state !== 'running') return { ...c }
    c.remainingMs = Math.max(0, (c.endsAt ?? 0) - now())
    c.paused = true
    c.state = 'paused'
    touch(c)
    save()
    return { ...c }
  },
  countdownResume: async (id) => {
    const c = needCountdown(id)
    if (!c.paused) return { ...c }
    const t = now()
    c.endsAt = c.mode === 'duration' || c.mode === 'timed' ? t + (c.remainingMs ?? 0) : cdDeadline(c, t)
    c.paused = false
    c.remainingMs = null
    c.state = 'running'
    touch(c)
    save()
    return { ...c }
  },
  countdownReset: async (id) => {
    const c = needCountdown(id)
    c.state = 'idle'
    c.paused = false
    c.endsAt = null
    c.remainingMs = null
    touch(c)
    save()
    return { ...c }
  },
  countdownDelete: async (id) => {
    const i = store.countdowns.findIndex((c) => c.id === id)
    if (i >= 0) store.countdowns.splice(i, 1)
    save()
  },

  // 浏览器里演的正是「第一次拿不到差值 → 第二次起才有数」这条真实分支
  sysLoad: async (): Promise<SysLoad> => {
    const total = 16 * 1024 ** 3
    sysSampled += 1
    return {
      cpuPercent: sysSampled < 2 ? null : Math.round((12 + Math.random() * 26) * 10) / 10,
      memTotal: total,
      memAvail: Math.round(total * (0.38 + Math.random() * 0.12)),
    }
  },

  // ---- M6 笔记 ----
  notesList: async (folderId) => {
    const live = store.notes.filter((n) => n.deletedAt === null && n.trashedAt === null)
    const picked = folderId == null ? live : live.filter((n) => n.folderId === folderId)
    return picked
      .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt)
      .map((n) => ({ ...n }))
  },
  notesTrashed: async () =>
    store.notes
      .filter((n) => n.deletedAt === null && n.trashedAt !== null)
      .sort((a, b) => (b.trashedAt ?? 0) - (a.trashedAt ?? 0))
      .map((n) => ({ ...n })),
  notesGet: async (id) => {
    const n = store.notes.find((v) => v.id === id && v.deletedAt === null)
    return n ? { ...n } : null
  },
  notesCreate: async (draft) => {
    const title = draft.title.trim()
    if (!title) throw new Error('笔记标题不能为空')
    const t = now()
    const created: Note = {
      id: uid(),
      rev: 1,
      createdAt: t,
      createdBy: DEVICE_ID,
      updatedAt: t,
      updatedBy: DEVICE_ID,
      deletedAt: null,
      title,
      bodyMd: draft.bodyMd ?? '',
      folderId: draft.folderId ?? null,
      pinned: false,
      trashedAt: null,
      createdDay: dayInt(t),
      tags: [...new Set((draft.tags ?? []).map((s) => s.trim()).filter(Boolean))],
    }
    store.notes.push(created)
    save()
    return { ...created }
  },
  notesUpdate: async (id, patch) => {
    const n = needNote(id)
    if (patch.title !== undefined) {
      const title = patch.title.trim()
      if (!title) throw new Error('笔记标题不能为空')
      n.title = title
    }
    if (patch.bodyMd !== undefined) n.bodyMd = patch.bodyMd
    if (patch.folderId !== undefined) n.folderId = patch.folderId
    if (patch.pinned !== undefined) n.pinned = patch.pinned
    touch(n)
    save()
    return { ...n }
  },
  notesSetTags: async (id, tags) => {
    const n = needNote(id)
    n.tags = [...new Set(tags.map((s) => s.trim()).filter(Boolean))]
    touch(n)
    save()
  },
  notesTrash: async (id) => {
    const n = needNote(id)
    n.trashedAt = now()
    touch(n)
    save()
    return { ...n }
  },
  notesRestore: async (id) => {
    const n = needNote(id)
    if (n.trashedAt !== null) {
      n.trashedAt = null
      touch(n)
      save()
    }
    return { ...n }
  },
  notesPurge: async (id) => {
    const n = needNote(id)
    // 与 Rust 一致：打墓碑而不是物理删除，v2 同步要靠它传播这次删除
    n.deletedAt = now()
    touch(n)
    save()
  },
  foldersList: async () => {
    const live = store.notes.filter((n) => n.deletedAt === null && n.trashedAt === null)
    return store.folders
      .filter((f) => f.deletedAt === null)
      .sort((a, b) => a.sort - b.sort)
      .map((f) => ({ ...f, noteCount: live.filter((n) => n.folderId === f.id).length }))
  },
  folderAdd: async (name, parentId) => {
    const trimmed = name.trim()
    if (!trimmed) throw new Error('文件夹名不能为空')
    const t = now()
    const created: NoteFolder = {
      id: uid(),
      rev: 1,
      createdAt: t,
      createdBy: DEVICE_ID,
      updatedAt: t,
      updatedBy: DEVICE_ID,
      deletedAt: null,
      name: trimmed,
      parentId,
      sort: (store.folders.at(-1)?.sort ?? 0) + GAP,
      noteCount: 0,
    }
    store.folders.push(created)
    save()
    return { ...created }
  },
  folderRename: async (id, name) => {
    const f = needFolder(id)
    const trimmed = name.trim()
    if (!trimmed) throw new Error('文件夹名不能为空')
    f.name = trimmed
    touch(f)
    save()
    return { ...f }
  },
  folderMove: async (id, parentId) => {
    const f = needFolder(id)
    if (parentId !== null) {
      if (parentId === id) throw new Error('不能把文件夹挪进它自己或它的子目录里')
      // 挪进自己的后代会造出环，树一渲染就死循环
      let cursor: string | null = parentId
      while (cursor) {
        if (cursor === id) throw new Error('不能把文件夹挪进它自己或它的子目录里')
        cursor = store.folders.find((v) => v.id === cursor)?.parentId ?? null
      }
    }
    f.parentId = parentId
    touch(f)
    save()
    return { ...f }
  },
  folderDelete: async (id) => {
    const f = needFolder(id)
    const inside = store.notes.filter((n) => n.deletedAt === null && n.folderId === id)
    for (const n of inside) n.folderId = null
    for (const child of store.folders.filter((v) => v.deletedAt === null && v.parentId === id)) {
      child.parentId = f.parentId
    }
    f.deletedAt = now()
    touch(f)
    save()
    return inside.length
  },

  timetableState: async () => mockTimetableState(),
  timetableSetWeek1: async (date) => {
    const trimmed = date.trim()
    if (trimmed && !/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) throw new Error('开学日要用 yyyy-mm-dd 这种写法')
    store.timetable.week1Monday = trimmed || null
    save()
    return mockTimetableState()
  },
  timetableWeek: async (week) => {
    const st = mockTimetableState()
    const w = week ?? st.currentWeek
    if (w == null) return []
    return store.timetable.rows.filter((r) => weeksInclude(r.weeks, w)).map((r) => ({ ...r }))
  },
  // 浏览器里没有原生对话框：与 resourcePick 同一套路，返回 null 表示"这里演不了"
  timetableImport: async (): Promise<TimetableImportReport | null> => null,

  hotkeysList: async () => [mockHotkey(HOTKEY_ACTION, comboOf())],
  hotkeysSet: async (action, combo) => {
    const next = assertCombo(combo)
    if (action !== HOTKEY_ACTION) throw new Error(`不认识的快捷键动作：${action}`)
    const item = mockHotkey(action, next)
    // 真实侧失败时 Rust 会回滚旧绑定，这里同样不落盘：留着旧值才是对的
    if (!item.registered) throw new Error(`「${next}」已被其他程序占用`)
    store.settings[SETTINGS_HOTKEYS] = { [action]: next }
    save()
    return item
  },
  hotkeysReset: async (action) => {
    if (action !== HOTKEY_ACTION) throw new Error(`不认识的快捷键动作：${action}`)
    store.settings[SETTINGS_HOTKEYS] = { [action]: DEFAULT_COMBO }
    save()
    return mockHotkey(action, DEFAULT_COMBO)
  },
  autostartGet: async () => store.settings[SETTINGS_AUTOSTART] === true,
  autostartSet: async (enabled) => {
    store.settings[SETTINGS_AUTOSTART] = enabled
    save()
    return enabled
  },
}

function snap(kind: string, status: SnapshotView['status'], items: unknown[], detail = '', extraMeta: Record<string, unknown> = {}): SnapshotView {
  const payload = { items, count: items.length, meta: extraMeta }
  return {
    source: 'blog',
    kind,
    fetchedAt: Date.now() - 5 * 60_000,
    sig: `${kind}-fixture`,
    status,
    detail,
    payloadJson: JSON.stringify(payload),
  }
}

function blogFixtures(): SnapshotView[] {
  return [
    snap('projects', 'ok', [
      { name: 'Rewmington Desk', description: '基于 Next.js 16 的个人工作台，毛玻璃 + Bento Grid', tags: ['Next.js'], url: 'https://example.dev', status: 'live' },
    ]),
    snap('repos', 'ok', [
      { name: 'rewmington-blog', description: '个人站点', language: 'TypeScript', stars: 12, pushedAt: '2026-10-22', archived: false, url: 'https://github.com/x/y' },
      { name: 'medicine', description: '药盒余量追踪', language: 'Dart', stars: 3, pushedAt: '2026-10-26', archived: false, url: 'https://github.com/x/m' },
    ], '', { remoteFetchedAt: '2026-10-25T00:14:00Z' }),
    snap('photos', 'ok', [{ id: 'p1', caption: '夜里的工作台', date: '2026-10-12', location: '杭州' }]),
    snap('friends', 'ok', [{ name: '某友链', url: 'https://friend.dev' }]),
    snap('profile', 'ok', [{ authorName: 'Rewmington', techStack: ['Vue', 'Rust'] }]),
    snap('music', 'ok', []),
    snap('notes', 'missing', [], 'notes 清单不在仓库里'),
  ]
}
/**
 * `detail` 就是「这一屏数据是从哪份来的、有多新」，口径逐条对齐 `medicine_copy::refresh`：
 * 没配第二条应用密码 → 空串（压根不启用云端，别在界面上提它）；
 * 配了 → 三种结果各对应 Rust 里的一条分支。用 `?mock=1&cloud=…` 切换，
 * 因为浏览器算不出真实的新旧比较（那是读两边的 `updated_at` 才知道的事）。
 */
function medicineDetail(): string {
  const m = store.medicine
  if (!m.hasPass) return ''
  const wanted = new URLSearchParams(location.search).get('cloud')
  if (wanted === 'local') return '本机 · 云端那份是 phone 3 天前发布'
  if (wanted === 'failed') return '本机 · 云端这次没读上：坚果云返回了 401（应用密码不对）'
  if (wanted === 'only') return '仅云端 · phone 发布于 3 分钟前'
  return '云端 · phone 发布于 3 分钟前'
}

function medicineFixture(): SnapshotView {
  const payload: MedicineTodayPayload = {
    items: [
      {
        id: 'm1', name: '二甲双胍', unit: '片', remaining: 6, boxes: 0.2, dailyUse: 2,
        daysLeft: 3, lowDays: 7, isLow: true, isOut: false,
        slots: [
          { minute: 450, label: '07:30', state: 'taken' },
          { minute: 1260, label: '21:00', state: 'upcoming' },
        ],
        taken: 1, owed: 2, pending: 0, skipped: 0,
      },
      {
        id: 'm2', name: '丁螺环酮', unit: '片', remaining: 30, boxes: 1, dailyUse: 2,
        daysLeft: 15, lowDays: 7, isLow: false, isOut: false,
        slots: [
          { minute: 450, label: '07:30', state: 'taken' },
          { minute: 840, label: '14:00', state: 'pending' },
          { minute: 1260, label: '21:00', state: 'upcoming' },
        ],
        taken: 1, owed: 3, pending: 1, skipped: 0,
      },
    ],
    count: 2,
    meta: { todayTaken: 2, todayOwed: 5, lowCount: 1, nowMinutes: 900 },
  }
  return {
    source: 'medicine',
    kind: 'today',
    fetchedAt: Date.now() - 2 * 60_000,
    sig: 'medicine-fixture',
    status: 'ok',
    detail: medicineDetail(),
    payloadJson: JSON.stringify(payload),
  }
}

/* ==== M9 课表：浏览器里演状态机。课程名全是假的 —— 仓库里不留任何人的真实课表 ==== */

interface TimetableSeed {
  week1Monday: string | null
  rows: ClassRow[]
}

/** 与真导出同形状的假数据：同一门课两段不同周次、有重叠、有空档 */
function freshTimetable(): TimetableSeed {
  const at = (name: string, teacher: string, room: string, day: number, s: number, e: number, weeks: string, period: string): ClassRow => ({
    name, teacher, room, day, startMin: s, endMin: e, period, weeks,
  })
  return {
    week1Monday: null,
    rows: [
      at('编译原理', '张老师', '一教 101', 1, 480, 580, '1-10,12-16', '第一大节 [01-02]节'),
      at('编译原理', '张老师', '一教 101', 1, 600, 700, '2-16', '第二大节 [03-04]节'),
      at('算法设计', '李老师', '三教 202', 2, 480, 580, '1-8', '第一大节 [01-02]节'),
      at('算法设计', '助教', '机房 B305', 2, 600, 700, '9-16', '第二大节 [03-04]节'),
      at('线性代数', '王老师', '一教 105', 3, 840, 940, '1-16', '第三大节 [05-06]节'),
      at('英语读写', 'Brown', '外院 305', 4, 480, 580, '1-12', '第一大节 [01-02]节'),
      at('体育课', '赵老师', '北操场', 5, 960, 1060, '1-16', '第四大节 [07-08]节'),
    ],
  }
}

/** 与 Rust 的 weeks_include 同口径（mock 是镜像，不是第二套生产实现） */
function weeksInclude(spec: string, week: number): boolean {
  return spec.split(',').some((part) => {
    const t = part.trim()
    const [a, b] = t.split('-')
    if (b === undefined) return Number(t) === week
    return Number(a) <= week && week <= Number(b)
  })
}

function mockTimetableState(): TimetableState {
  const t = store.timetable
  const total = t.rows.reduce((max, r) => Math.max(max, ...r.weeks.split(',').map((p) => Number(p.split('-').at(-1)))), 0)
  return {
    term: t.rows.length ? '2026-2027-1' : null,
    week1Monday: t.week1Monday,
    currentWeek: weekOf(t.week1Monday, Date.now(), t.rows.length ? total : null),
    totalWeeks: t.rows.length ? total : null,
    rows: t.rows.length,
    courses: new Set(t.rows.map((r) => r.name)).size,
    periods: [...new Map(t.rows.map((r) => [r.startMin, r.period])).entries()]
      .map(([startMin, label]) => ({ startMin, label }))
      .sort((a, b) => a.startMin - b.startMin),
  }
}
