import type {
  Api,
  BackupLogRow,
  BackupOutcome,
  BackupStatus,
  Countdown,
  CountdownMode,
  DashSummary,
  MedicineTodayPayload,
  PaletteHit,
  PathsView,
  QueueGroup,
  RemoteFile,
  Resource,
  SettingsView,
  SnapshotView,
  Sticky,
  Suggestion,
  SysLoad,
  Todo,
  TodoDraft,
  TodoPatch,
  TodoZone,
  Zone,
} from './types'

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
 * 过去 90 天的完成记录：让时间轴与（M7 的）统计有真实形状可看。
 * 天数刻意跨过 60 天门槛，这样「不足 60 天不画趋势线」那条降级也能反过来验到。
 */
function seedHistory(t: number): Todo[] {
  const out: Todo[] = []
  for (let i = 1; i <= 90; i++) {
    // 周末做得少，工作日做得多：柱状图与热力图才不至于是一片均匀的噪声
    const dow = new Date(t - i * DAY).getDay()
    const n = dow === 0 || dow === 6 ? (i % 4 === 0 ? 1 : 0) : i % 3 === 0 ? 3 : i % 5 === 0 ? 0 : 2
    for (let k = 0; k < n; k++) {
      const at = t - i * DAY + (9 + k * 4) * HOUR
      out.push(
        makeTodo({
          title: `完成于 ${String(dayInt(at)).slice(4, 6)}/${String(dayInt(at)).slice(6, 8)} 的第 ${k + 1} 项`,
          zone: 'today',
          createdAt: at - 2 * DAY,
          doneAt: at,
          doneDay: dayInt(at),
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
  stickies: Sticky[]
  countdowns: Countdown[]
}

/** 备份在浏览器里只演「状态机」：配没配凭据、流水、远端清单。加解密链路归 cargo test。 */
interface BackupSeed {
  email: string | null
  hasPass: boolean
  log: BackupLogRow[]
  remote: RemoteFile[]
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

const KEY = 'anvil.mock.store.v2'

/** `?mock=1&empty=1`：把种子数据清空，用来验 §9 M5 那条「空/满两态都不出现死白」。 */
const EMPTY = new URLSearchParams(location.search).has('empty')

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
    return { todos: [], resources: [], zones: [], secrets: {}, settings: {}, backup: { email: null, hasPass: false, log: [], remote: [] }, stickies: [], countdowns: [] }
  }
  return {
    todos: seedTodos(),
    resources: seedResources(),
    zones: seedZones(),
    secrets: {},
    settings: {},
    backup: freshBackup(),
    stickies: seedStickies(),
    countdowns: seedCountdowns(),
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
    paths: {
      blogRepo: String.raw`D:\ADevelop\Repositories\Mine\Rewmington-Blog-main`,
      blogRepoExists: true,
      medicineDb: String.raw`C:\Users\hp\AppData\Roaming\dev.mine\medicine_tracker\medicine_tracker.db`,
      medicineDbExists: true,
      onlineCheckEnabled: true,
    },
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
  linkedSetBlogDir: async (path) => ({
    blogRepo: path || String.raw`D:\ADevelop\Repositories\Mine\Rewmington-Blog-main`,
    blogRepoExists: true,
    medicineDb: String.raw`C:\Users\hp\AppData\Roaming\dev.mine\medicine_tracker\medicine_tracker.db`,
    medicineDbExists: true,
    onlineCheckEnabled: true,
  }),

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
    detail: '',
    payloadJson: JSON.stringify(payload),
  }
}
