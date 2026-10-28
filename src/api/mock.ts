import type { Api, PathsView, SettingsView } from './types'

/**
 * 只在浏览器里跑（`npm run dev` + `?mock=1`）时用的实现。
 *
 * 它存在的唯一理由：M5/M7 的验收是「逐断点截图、量选中态对比度、数同屏 accent 实心填充」，
 * 这些是纯视觉判据，需要能任意改视口尺寸反复截。原生窗口那一条腿由 tools/capture.ps1 负责，
 * 行为正确性由 cargo test 负责 —— 不要用这里的返回值证明任何后端结论。
 */

const LS_KEY = 'anvil.mock.settings'

const paths: PathsView = {
  root: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil`,
  db: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\app.db`,
  icons: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\icons`,
  logs: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\logs`,
  backups: String.raw`C:\Users\hp\AppData\Local\dev.rewmington.anvil\backups`,
  deviceId: 'pc-0196e4b3a1c27d80a9b0c1d2e3f40000',
  journalMode: 'wal',
}

function readStore(): Record<string, unknown> {
  try {
    return JSON.parse(localStorage.getItem(LS_KEY) ?? '{}') as Record<string, unknown>
  } catch {
    return {}
  }
}

function writeStore(next: Record<string, unknown>) {
  localStorage.setItem(LS_KEY, JSON.stringify(next))
}

export const mockApi: Api = {
  paths: async () => paths,
  deviceId: async () => paths.deviceId,
  settingsAll: async (): Promise<SettingsView> => ({
    values: readStore(),
    paths,
    deviceId: paths.deviceId,
  }),
  settingsGet: async (key) => readStore()[key] ?? null,
  settingsSet: async (key, value) => {
    writeStore({ ...readStore(), [key]: value })
  },
  contextMenu: async () => {
    // 浏览器里没有 Win32 菜单可弹；验 UI 时这条路径本就不该出现原生菜单。
  },
}
