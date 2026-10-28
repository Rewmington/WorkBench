import { type Component, ref } from 'vue'
import {
  BookOpen,
  CalendarDays,
  ChartNoAxesColumn,
  Gauge,
  HardDrive,
  ListChecks,
  Pill,
  Settings as SettingsIcon,
  StickyNote,
  Timer,
  UploadCloud,
  Zap,
} from 'lucide-vue-next'

import type { ViewId } from '../api/types'

export interface NavItem {
  /** 侧栏条目的稳定标识，也是 §8.7 计数徽章的取数键 */
  key: string
  label: string
  icon: Component
  view: ViewId
  /** 同一视图内的落点（例如「进行」视图里的倒计时部件） */
  focus?: string
}

/**
 * 侧栏条目由功能清单决定，不由风格决定（PLAN §8.7，v1 共 12 项）。
 * 参考图里的智能分析/知识图谱/吸收与沉淀/扫描记录一条都不放 —— 没有对应功能，
 * 硬凑只会做出「点进去是空页」的导航。
 */
export const NAV: { title?: string; items: NavItem[] }[] = [
  {
    items: [
      { key: 'dashboard', label: '今日驾驶舱', icon: Gauge, view: 'dashboard' },
      { key: 'todos', label: '待办', icon: ListChecks, view: 'todos' },
      { key: 'launcher', label: '速达', icon: Zap, view: 'launcher' },
      { key: 'notes', label: '笔记', icon: BookOpen, view: 'notes' },
    ],
  },
  {
    title: '进行',
    items: [
      { key: 'countdowns', label: '倒计时', icon: Timer, view: 'progress', focus: 'countdowns' },
      { key: 'stickies', label: '便签', icon: StickyNote, view: 'progress', focus: 'stickies' },
    ],
  },
  {
    title: '回顾',
    items: [
      { key: 'stats', label: '统计', icon: ChartNoAxesColumn, view: 'stats' },
      { key: 'calendar', label: '日历', icon: CalendarDays, view: 'calendar' },
    ],
  },
  {
    title: '外部',
    items: [
      { key: 'blog', label: '博客', icon: HardDrive, view: 'blog' },
      { key: 'medicine', label: '药箱', icon: Pill, view: 'medicine' },
    ],
  },
  {
    title: '系统',
    items: [
      { key: 'backup', label: '备份', icon: UploadCloud, view: 'backup' },
      { key: 'settings', label: '设置', icon: SettingsIcon, view: 'settings' },
    ],
  },
]

export const VIEW_TITLES: Record<ViewId, string> = {
  dashboard: '今日驾驶舱',
  todos: '待办',
  launcher: '速达',
  notes: '笔记',
  progress: '进行',
  stats: '统计',
  calendar: '日历',
  blog: '博客',
  medicine: '药箱',
  backup: '备份',
  settings: '设置',
}

const view = ref<ViewId>('dashboard')
const focus = ref<string | null>(null)
/** 还没有对应数据时保持 null —— 宁可无徽章，不显示假计数（§2 原则 5） */
const counts = ref<Record<string, number | null>>({})

/** 详情面板的当前对象。窄窗时它变成覆盖层，所以「有没有被用户主动打开」要单独记。 */
export type Selection =
  | { kind: 'todo'; id: string }
  | { kind: 'resource'; id: string }
  | { kind: 'day'; day: number }
  | null

const selection = ref<Selection>(null)
const detailOpen = ref(false)
/** 命令面板说「新建待办」时，视图要自己把输入框聚焦。递增计数而不是布尔，连点两次也能感知。 */
const composerTick = ref(0)
const composerTarget = ref<'todo' | 'resource' | 'note'>('todo')

export function useShell() {
  function go(next: ViewId, at?: string) {
    view.value = next
    focus.value = at ?? null
  }

  function setCount(key: string, value: number | null) {
    counts.value = { ...counts.value, [key]: value }
  }

  function select(next: Selection) {
    selection.value = next
    detailOpen.value = next !== null
  }

  function requestComposer(target: 'todo' | 'resource' | 'note' = 'todo') {
    composerTarget.value = target
    composerTick.value += 1
  }

  return {
    view,
    focus,
    counts,
    selection,
    detailOpen,
    composerTick,
    composerTarget,
    go,
    setCount,
    select,
    requestComposer,
    NAV,
    VIEW_TITLES,
  }
}
