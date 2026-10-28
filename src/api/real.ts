import { invoke } from '@tauri-apps/api/core'

import type { Api, PathsView, SettingsView } from './types'

/// 命令名 = Rust 函数名（snake_case）；参数对象用 camelCase 键。
export const realApi: Api = {
  paths: () => invoke<PathsView>('paths'),
  deviceId: () => invoke<string>('device_id'),
  settingsAll: () => invoke<SettingsView>('settings_all'),
  settingsGet: (key) => invoke<unknown>('settings_get', { key }),
  settingsSet: (key, value) => invoke<void>('settings_set', { key, value }),
  contextMenu: (target) => invoke<void>('context_menu', { target }),
}
