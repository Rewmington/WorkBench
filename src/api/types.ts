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

export interface Api {
  paths(): Promise<PathsView>
  deviceId(): Promise<string>
  settingsAll(): Promise<SettingsView>
  settingsGet(key: string): Promise<unknown>
  settingsSet(key: string, value: unknown): Promise<void>
  /** 弹 Win32 原生右键菜单；target 只说明点的是哪一类东西 */
  contextMenu(target: 'shell' | 'input'): Promise<void>
}
