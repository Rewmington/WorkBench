import type {
  Api,
  BackupLogRow,
  BackupOutcome,
  BackupStatus,
  MedicineTodayPayload,
  PaletteHit,
  PathsView,
  RemoteFile,
  Resource,
  SettingsView,
  SnapshotView,
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
 * 但语义要跟 Rust 一致，否则「看着对」是假的：rev 每次写入 +1、取消勾选不清 done_at、
 * 删除是打墓碑、排序取邻居中点。这些规则在这里各写一遍，是因为它们同时也是渲染前提。
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
    makeTodo({ title: '写周报', zone: 'inbox', parentId: null }),
  ]
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

const KEY = 'anvil.mock.store.v1'

function freshStore(): Store {
  return {
    todos: seedTodos(),
    resources: seedResources(),
    zones: seedZones(),
    secrets: {},
    settings: {},
    backup: freshBackup(),
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

function touch(t: Todo) {
  t.rev += 1
  t.updatedAt = now()
  t.updatedBy = DEVICE_ID
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
