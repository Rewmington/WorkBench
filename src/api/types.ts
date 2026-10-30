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
  | 'timetable'
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
  /** **最后一次**完成于何时，取消勾选不清空。当前是不是完成态看 `completed`。 */
  doneAt: number | null
  doneDay: number | null
  /** 当前勾选状态：来回切都改它。渲染勾选框、过滤未完成一律用它，不要用 doneAt。 */
  completed: boolean
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
  /** 药箱在坚果云上的同步目录（只读） */
  medicineDav: string
  /** 配了那第二条应用密码。不是"云端连得上"—— 那要发一次请求才知道，而首屏不发。 */
  medicineCloudReady: boolean
  onlineCheckEnabled: boolean
  /**
   * 三个「不改时用它」的值 —— 界面那个「恢复默认」按钮得知道默认是什么。
   * 默认值只有 Rust 那一份：以前 TS 里手抄过一条坚果云地址，改 Rust 不会通知它。
   */
  blogRepoDefault: string
  medicineDbDefault: string
  medicineDavDefault: string
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

/** 备份流水的一行。`kind` 是 daily / manual / restore，直接决定这行怎么念。 */
export interface BackupLogRow {
  at: number
  ok: boolean
  bytes: number | null
  remotePath: string | null
  error: string | null
  kind: string
}

export interface BackupStatus {
  /** 邮箱在 settings 里，应用密码只在 Windows 凭据管理器里 —— 两个都齐才算配好 */
  configured: boolean
  /** 单看凭据管理器那条：还原之后邮箱空着而密码还在，界面该说"填个邮箱就行" */
  passwordStored: boolean
  email: string | null
  lastSuccessAt: number | null
  lastError: string | null
  lastErrorAt: number | null
  log: BackupLogRow[]
  paths: PathsView
  journalMode: string
}

export interface BackupOutcome {
  at: number
  ok: boolean
  bytes: number | null
  remotePath: string | null
  error: string | null
  /** 没配凭据是跳过，不是失败：不该进失败退避，也不该在 UI 上写成错误 */
  skipped: boolean
}

export interface RemoteFile {
  name: string
  size: number
  /** 文件名对不对得上我们的握手格式 */
  readable: boolean
}

// ---- M5 工作台 ----

/**
 * KPI 一格。`delta` 只有历史撑得住的那一项才有：完成数能算昨天，
 * 而「昨天的未整理捕获有多少条」没有任何地方记过 —— 没有就不假装（§2 原则 5）。
 */
export interface Kpi {
  value: number
  delta: number | null
  note: string | null
}

export interface DayCount {
  /** 本地自然日 yyyymmdd */
  day: number
  done: number
}

export interface QueueGroup {
  key: 'late' | 'today' | 'later'
  label: string
  items: Todo[]
}

export interface DashSummary {
  today: Kpi
  inbox: Kpi
  done: Kpi
  late: Kpi
  /** 3D 卡片环的卡源，最多 8 张 */
  focus: Todo[]
  groups: QueueGroup[]
  timeline: DayCount[]
  month: { done: number; open: number }
  /** 这个库记了多少个自然日。< 60 时时间轴不画趋势线（§8.8.5） */
  historyDays: number
}

/** 「下一步建议」= 规则命中，不是模型。`rule` 是稳定 id，界面不靠文案认它。 */
export interface Suggestion {
  rule: 'overdue-oldest' | 'due-today-untouched' | 'countdown-short' | 'medicine-low' | 'inbox-pileup'
  headline: string
  detail: string
  todoId: string | null
  view: ViewId | null
}

export interface Sticky extends Stamp {
  slot: number
  content: string
}

export type CountdownMode = 'duration' | 'timed' | 'daily' | 'interval'
export type CountdownState = 'idle' | 'running' | 'paused' | 'finished'

export interface Countdown {
  id: string
  rev: number
  createdAt: number
  createdBy: string
  updatedAt: number
  updatedBy: string
  deletedAt: number | null
  label: string
  mode: CountdownMode
  spec: {
    minutes?: number
    everyMinutes?: number
    startedAt?: number
    until?: number
    at?: string
  }
  endsAt: number | null
  paused: boolean
  state: CountdownState
  /** 只在 paused 时有值：冻住的剩余毫秒（见迁移 0003） */
  remainingMs: number | null
}

export interface SysLoad {
  /** 第一次采样拿不到差值，界面那一格先空着而不是显示 0% */
  cpuPercent: number | null
  memTotal: number
  memAvail: number
}

// ---- M6 笔记 ----

export interface Note extends Stamp {
  title: string
  /** 正文 Markdown 就是真相源，预览是派生物 —— 后端不存 HTML */
  bodyMd: string
  folderId: string | null
  pinned: boolean
  /** 回收站时间戳。与 `deletedAt`（同步墓碑）是两件事：进回收站的还能还原。 */
  trashedAt: number | null
  createdDay: number | null
  tags: string[]
}

export interface NoteDraft {
  title: string
  bodyMd?: string
  folderId?: string | null
  tags?: string[]
}

/** `folderId: null` 是「挪到无文件夹那一层」，不传这个键才是「不动它」。 */
export interface NotePatch {
  title?: string
  bodyMd?: string
  folderId?: string | null
  pinned?: boolean
}

export interface NoteFolder extends Stamp {
  name: string
  parentId: string | null
  sort: number
  /** 直属笔记数，由后端一次算好：让前端自己数等于逼它把全部笔记拉一遍 */
  noteCount: number
}

/**
 * 一条全局快捷键的当前状态。
 *
 * `registered` 是「此刻真的挂上了吗」，不是「设置里写了什么」——
 * 被别的程序抢走时注册会失败，界面必须能说清楚，而不是假装按了有反应。
 */
export interface HotkeyItem {
  action: string
  label: string
  combo: string
  registered: boolean
}

/**
 * 统计视图的五张图，一次拿全（`repos/stats.rs`）。
 *
 * 日期一律是**本地自然日整数** `yyyymmdd`，小时是 0–23 的本地小时：
 * 前端不做任何时区换算，换算的口径只有一处（Rust 的 `days` 模块）。
 */
export interface HeatCell {
  day: number
  done: number
}

export interface Heatmap {
  /** 长度恒为 15×7，周一开头、空白天也占一格（done = 0） */
  cells: HeatCell[]
  startDay: number
  endDay: number
  activeDays: number
  /** 连续做了几天，止于 `streakThrough` —— 今天还没做不代表连续断了 */
  streak: number
  streakThrough: number | null
}

/** 一天的三档。没设截止的既不属于按时也不属于补做，所以第三档明写出来。 */
export interface DayBar {
  day: number
  onTime: number
  late: number
  noDue: number
}

export interface TodayCounts {
  open: number
  done: number
}

export interface TagBar {
  tag: string
  done: number
}

export interface HourBar {
  hour: number
  created: number
}

export interface StatsReport {
  todayDay: number
  historyDays: number
  heatmap: Heatmap
  bars: DayBar[]
  gauge: TodayCounts
  tags: TagBar[]
  /** 近 90 天完成的总条数。一条待办能挂多个标签，所以各条加起来会超过它。 */
  tagsTotal: number
  hours: HourBar[]
  /** 时段直方图够不够画：不足门槛天数画出来的是噪声 */
  hoursEnough: boolean
  /** 门槛天数由后端送来，前端不抄一份常量 —— 改了 Rust 的数，那句话不能还说旧的 */
  hourMinDays: number
  hasActivity: boolean
}

/**
 * 课表的一行 = 一个「星期 × 节次 × 周次集合」，**不是一门课**（PLAN §6.7）。
 * 同一门课在不同周次段可以在不同教室，所以它在库里天然可能是好几行，界面按名字归组。
 */
export interface ClassRow {
  name: string
  teacher: string
  room: string
  /** 1 = 周一 … 7 = 周日，与 Rust 的 `days::weekday` 同一约定 */
  day: number
  /** 本地分钟数 0–1439。判断"现在是不是在上课"要能比大小，所以不存字符串 */
  startMin: number
  endMin: number
  /** 节次标签原样（如「第一大节 [01-02]节」），界面直接显示，不自己编号 */
  period: string
  /** 规范化后的周次集合，如 `1-10,12-16` */
  weeks: string
}

export interface TimetableState {
  term: string | null
  /** ISO `yyyy-mm-dd`。null = 还没设 —— 界面要要求填，不能自己猜一个 */
  week1Monday: string | null
  /** 没设开学日 / 还没开学 / 学期已结束 → null（不是 0） */
  currentWeek: number | null
  /** 从各课周次集合的最大端推的默认值 */
  totalWeeks: number | null
  rows: number
  courses: number
  /**
   * 节次骨架，来自整学期而不是"这一周的课"：空档周（比如第 11 周什么都没排）
   * 也要画出完整的行，否则网格会塌成一片空白，看着像"课表丢了"。
   */
  periods: { startMin: number; label: string }[]
}

export interface TimetableImportReport {
  term: string
  rows: number
  courses: number
  /** 导完了但还算不出"今天第几周"：界面拿这个去提示填开学日 */
  needsWeek1: boolean
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
  /** 药箱库在这台机器上的位置。空串恢复默认 —— 换机或对方改了 AppData 目录时要能改。 */
  linkedSetMedicineDb(path: string): Promise<SourcePaths>
  /** 药箱云端备用源：邮箱进 settings，应用密码只进凭据管理器（与备份那条分开） */
  linkedSetMedicinePassword(email: string, password: string): Promise<boolean>
  linkedClearMedicinePassword(): Promise<boolean>
  linkedSetMedicineDir(dir: string): Promise<string>

  backupStatus(): Promise<BackupStatus>
  backupNow(): Promise<BackupOutcome>
  /** 邮箱进 settings（它不是秘密，且要解释凭据条目属于谁），应用密码只进凭据管理器 */
  backupSaveCredentials(email: string, password: string): Promise<BackupStatus>
  backupClearCredentials(): Promise<BackupStatus>
  /** 只清这张列表：远端文件与本地暂存副本都不动 */
  backupClearLog(): Promise<BackupStatus>
  backupListRemote(): Promise<RemoteFile[]>
  /** 只列目录，不写任何东西 */
  backupTestConnection(): Promise<number>
  /** 整文件覆盖当前库；成功后应用会重启 */
  backupRestore(name: string): Promise<BackupOutcome>
  /** 不联网也能证明「换机能解开」这条链路 */
  backupSelfTest(email: string, password: string): Promise<boolean>

  /** 首屏一次拿全：KPI + 环 + 队列 + 时间轴。全部来自 SQLite，不等任何外部源 */
  dashSummary(): Promise<DashSummary>
  dashSuggestions(): Promise<Suggestion[]>

  /** 统计页五张图的聚合，一次出齐 */
  statsReport(): Promise<StatsReport>

  stickiesList(): Promise<Sticky[]>
  stickySave(slot: number, content: string): Promise<Sticky>
  /** 腾开槽位（连行一起软删），与「正文写空」是两件事 */
  stickyClear(slot: number): Promise<boolean>

  countdownsList(): Promise<Countdown[]>
  countdownAdd(label: string, mode: CountdownMode, spec: Countdown['spec']): Promise<Countdown>
  countdownStart(id: string): Promise<Countdown>
  /** 暂停会冻住剩余毫秒，继续从冻住的地方接着走（见迁移 0003） */
  countdownPause(id: string): Promise<Countdown>
  countdownResume(id: string): Promise<Countdown>
  countdownReset(id: string): Promise<Countdown>
  countdownDelete(id: string): Promise<void>

  /** 第一次调用拿不到 CPU 差值 → cpuPercent 为 null，界面那一格先空着 */
  sysLoad(): Promise<SysLoad>

  /** folderId 传 null = 不过滤（看全部），不是「只看没有文件夹的」 */
  notesList(folderId: string | null): Promise<Note[]>
  notesTrashed(): Promise<Note[]>
  notesGet(id: string): Promise<Note | null>
  notesCreate(draft: NoteDraft): Promise<Note>
  notesUpdate(id: string, patch: NotePatch): Promise<Note>
  notesSetTags(id: string, tags: string[]): Promise<void>
  /** 进回收站：正文与标签都留着 */
  notesTrash(id: string): Promise<Note>
  notesRestore(id: string): Promise<Note>
  /** 彻底删除 = 打墓碑。这一步之后只能靠快照找回 */
  notesPurge(id: string): Promise<void>
  foldersList(): Promise<NoteFolder[]>
  folderAdd(name: string, parentId: string | null): Promise<NoteFolder>
  folderRename(id: string, name: string): Promise<NoteFolder>
  folderMove(id: string, parentId: string | null): Promise<NoteFolder>
  /** 返回被挪走的笔记数：0 也要报，界面据此区分「空目录已删除」与「N 篇已挪到上一层」 */
  folderDelete(id: string): Promise<number>

  /** ---- M9 课表：导入即整学期替换，课不进 todos（§6.7） ---- */
  timetableState(): Promise<TimetableState>
  /** 设第 1 周的周一。空串 = 清掉，回到"没设"并要求重填 */
  timetableSetWeek1(date: string): Promise<TimetableState>
  /** 某一天的课。`week` 传 null = 当前周；算不出当前周时返回空列表而不是猜 */
  timetableWeek(week: number | null): Promise<ClassRow[]>
  /** 弹原生对话框选 CSV；取消返回 null。不是 UTF-8 会抛那句可执行的提示 */
  timetableImport(): Promise<TimetableImportReport | null>

  hotkeysList(): Promise<HotkeyItem[]>  /** 换绑。注册失败时 Rust 侧已回滚，这里抛错、前端只显示那句话 */
  hotkeysSet(action: string, combo: string): Promise<HotkeyItem>
  hotkeysReset(action: string): Promise<HotkeyItem>
  autostartGet(): Promise<boolean>
  autostartSet(enabled: boolean): Promise<boolean>
}
