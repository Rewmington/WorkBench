/**
 * 后端能力的唯一形状。real.ts（Tauri invoke）与 mock.ts（浏览器里验 UI）都实现它，
 * 谁在跑由 index.ts 决定 —— 组件只认这个接口。
 */

export type ViewId =
  | 'dashboard'
  | 'todos'
  | 'launcher'
  | 'notes'
  | 'progress'
  | 'stats'
  | 'calendar'
  | 'blog'
  | 'medicine'
  | 'backup'
  | 'settings'

/** PLAN §5.1 的同步列。UI 里要显示 rev 与「最后写入设备」，所以它们必须在形状里。 */
export interface Stamp {
  id: string
  rev: number
  createdAt: number
  createdBy: string
  updatedAt: number
  updatedBy: string
  deletedAt: number | null
}

export type TodoZone = 'inbox' | 'today' | 'later' | 'someday'

/** 0 最急；状态不只靠颜色，优先级同时改文字与图标（§2 原则 3） */
export type Priority = 0 | 1 | 2 | 3

export interface Todo extends Stamp {
  title: string
  note: string
  doneAt: number | null
  doneDay: number | null
  dueAt: number | null
  remindAt: number | null
  priority: Priority
  pinned: boolean
  parentId: string | null
  sortKey: number
  zone: TodoZone
  createdDay: number | null
  createdHour: number | null
  tags: string[]
}

export interface TodoDraft {
  title: string
  note?: string
  zone?: TodoZone
  priority?: Priority
  dueAt?: number | null
  remindAt?: number | null
  parentId?: string | null
  tags?: string[]
}

/** null = 清空这个字段；undefined = 不动它。少一层就没法表达「去掉截止时间」。 */
export interface TodoPatch {
  title?: string
  note?: string | null
  zone?: TodoZone
  priority?: Priority
  dueAt?: number | null
  remindAt?: number | null
  pinned?: boolean
  parentId?: string | null
}

export type ResourceKind = 'app' | 'web' | 'file'

export interface Resource {
  id: string
  rev: number
  createdAt: number
  createdBy: string
  updatedAt: number
  updatedBy: string
  kind: ResourceKind
  name: string
  target: string
  args: string
  iconPath: string | null
  zoneId: string | null
  useCount: number
  lastLaunchedAt: number | null
  note: string
}

export interface Zone {
  id: string
  name: string
  sort: number
  itemCount: number
}

export interface PaletteHit {
  kind: 'todo' | 'resource'
  id: string
  label: string
  sub: string | null
}

export interface PathsView {
  root: string
  db: string
  icons: string
  logs: string
  backups: string
  deviceId: string
  journalMode: string
}

export interface SettingsView {
  values: Record<string, unknown>
  paths: PathsView
  deviceId: string
}

export type ThemeMode = 'auto' | 'dark' | 'light'
export type ThemePreset = 'graphite' | 'moss' | 'ink'
export type ThemeAccent = 'green' | 'blue' | 'amber'

/** 外部联动的缓存快照。UI 只读这张表，对方升级或删掉都不伤本体（PLAN §5.2）。 */
export interface SnapshotView {
  source: string
  kind: string
  fetchedAt: number
  sig: string
  status: 'ok' | 'missing' | 'stale' | 'error'
  detail: string
  payloadJson: string
}

export interface SourcePaths {
  blogRepo: string
  blogRepoExists: boolean
  medicineDb: string
  medicineDbExists: boolean
  onlineCheckEnabled: boolean
}

export interface LinkedState {
  paths: SourcePaths
  blog: SnapshotView[]
  medicine: SnapshotView | null
}

export interface BlogPayload<T> {
  items: T[]
  count: number
  meta: Record<string, unknown>
}

export interface SlotView {
  minute: number
  label: string
  state: 'taken' | 'skipped' | 'pending' | 'upcoming'
}

export interface MedicineTodayItem {
  id: string
  name: string
  unit: string
  remaining: number
  boxes: number
  dailyUse: number
  daysLeft: number | null
  lowDays: number
  isLow: boolean
  isOut: boolean
  slots: SlotView[]
  taken: number
  owed: number
  pending: number
  skipped: number
}

export interface MedicineTodayPayload {
  items: MedicineTodayItem[]
  count: number
  meta: { todayTaken?: number; todayOwed?: number; lowCount?: number; nowMinutes?: number }
}

export interface Api {
  paths(): Promise<PathsView>
  deviceId(): Promise<string>
  settingsAll(): Promise<SettingsView>
  settingsGet(key: string): Promise<unknown>
  settingsSet(key: string, value: unknown): Promise<void>
  /** 弹 Win32 原生菜单；target 只说明点的是哪一类东西 */
  contextMenu(target: 'shell' | 'input'): Promise<void>
  openDataDir(): Promise<void>
  /** 交给系统默认程序，不在 webview 里开新窗口 */
  openUrl(url: string): Promise<void>

  todosList(zone: TodoZone): Promise<Todo[]>
  /** 跨分区的活动待办：日历与驾驶舱要的是「一天」，不是四个区各查一遍 */
  todosAll(): Promise<Todo[]>
  todosListUndone(zone: TodoZone): Promise<Todo[]>
  todosGet(id: string): Promise<Todo | null>
  todosChildren(id: string): Promise<Todo[]>
  todosCreate(draft: TodoDraft): Promise<Todo>
  todosUpdate(id: string, patch: TodoPatch): Promise<Todo>
  todosSetDone(id: string, done: boolean): Promise<Todo>
  /** 返回受影响 id 列表（父删子随），撤销按这份列表整体还原 */
  todosDelete(id: string): Promise<string[]>
  todosRestore(ids: string[]): Promise<void>
  todosReorder(id: string, prevId: string | null, nextId: string | null): Promise<void>
  todosSetTags(id: string, tags: string[]): Promise<void>

  resourcesList(): Promise<Resource[]>
  resourceAddFromPath(path: string, zoneId: string | null): Promise<Resource>
  /** 原生文件对话框（在 Rust 侧弹）；取消返回 null */
  resourcePick(zoneId: string | null): Promise<Resource | null>
  resourceAddWeb(name: string, url: string, zoneId: string | null): Promise<Resource>
  /** "started" = 新起进程；"raised" = 本来在跑，已把窗口带到前台 */
  resourceLaunch(id: string): Promise<'started' | 'raised'>
  resourceRename(id: string, name: string): Promise<void>
  resourceMove(id: string, zoneId: string | null): Promise<void>
  resourceDelete(id: string): Promise<void>
  resourceSetSecret(id: string, text: string): Promise<void>
  resourceRevealSecret(id: string): Promise<string | null>
  zonesList(): Promise<Zone[]>
  zoneAdd(name: string): Promise<Zone>
  zoneDelete(id: string): Promise<void>

  paletteSearch(query: string): Promise<PaletteHit[]>

  /** 只读缓存，绝不碰外部文件或网络：首屏必须立刻画出来 */
  linkedState(): Promise<LinkedState>
  linkedRefreshBlog(): Promise<SnapshotView[]>
  linkedRefreshMedicine(): Promise<SnapshotView | null>
  /** 线上比对，慢的时候首包 8 秒 —— 只在后台跑，不进首屏 */
  linkedCheckOnline(): Promise<SnapshotView[]>
  /** 「补买 X」是用户点出来的，不是系统偷偷建的 */
  linkedRestockTodo(name: string, daysLeft: number | null): Promise<Todo>
  linkedSetBlogDir(path: string): Promise<SourcePaths>
}
