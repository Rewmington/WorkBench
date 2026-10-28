import { invoke } from '@tauri-apps/api/core'

import type {
  Api,
  BackupOutcome,
  BackupStatus,
  LinkedState,
  PaletteHit,
  PathsView,
  RemoteFile,
  Resource,
  SettingsView,
  SnapshotView,
  SourcePaths,
  Todo,
  Zone,
} from './types'

// 命令名 = Rust 函数名（snake_case）；参数对象用 camelCase 键。
export const realApi: Api = {
  paths: () => invoke<PathsView>('paths'),
  deviceId: () => invoke<string>('device_id'),
  settingsAll: () => invoke<SettingsView>('settings_all'),
  settingsGet: (key) => invoke<unknown>('settings_get', { key }),
  settingsSet: (key, value) => invoke<void>('settings_set', { key, value }),
  contextMenu: (target) => invoke<void>('context_menu', { target }),
  openDataDir: () => invoke<void>('open_data_dir'),
  openUrl: (url) => invoke<void>('open_url', { url }),

  todosList: (zone) => invoke<Todo[]>('todos_list', { zone }),
  todosAll: () => invoke<Todo[]>('todos_all'),
  todosListUndone: (zone) => invoke<Todo[]>('todos_list_undone', { zone }),
  todosGet: (id) => invoke<Todo | null>('todos_get', { id }),
  todosChildren: (id) => invoke<Todo[]>('todos_children', { id }),
  todosCreate: (draft) => invoke<Todo>('todos_create', { draft }),
  todosUpdate: (id, patch) => invoke<Todo>('todos_update', { id, patch }),
  todosSetDone: (id, done) => invoke<Todo>('todos_set_done', { id, done }),
  todosDelete: (id) => invoke<string[]>('todos_delete', { id }),
  todosRestore: (ids) => invoke<void>('todos_restore', { ids }),
  todosReorder: (id, prevId, nextId) =>
    invoke<void>('todos_reorder', { id, prevId, nextId }),
  todosSetTags: (id, tags) => invoke<void>('todos_set_tags', { id, tagsInput: tags }),

  resourcesList: () => invoke<Resource[]>('resources_list'),
  resourceAddFromPath: (path, zoneId) =>
    invoke<Resource>('resource_add_from_path', { path, zoneId }),
  resourcePick: (zoneId) => invoke<Resource | null>('resource_pick', { zoneId }),
  resourceAddWeb: (name, url, zoneId) =>
    invoke<Resource>('resource_add_web', { name, url, zoneId }),
  resourceLaunch: (id) => invoke<'started' | 'raised'>('resource_launch', { id }),
  resourceRename: (id, name) => invoke<void>('resource_rename', { id, name }),
  resourceMove: (id, zoneId) => invoke<void>('resource_move', { id, zoneId }),
  resourceDelete: (id) => invoke<void>('resource_delete', { id }),
  resourceSetSecret: (id, text) => invoke<void>('resource_set_secret', { id, text }),
  resourceRevealSecret: (id) => invoke<string | null>('resource_reveal_secret', { id }),
  zonesList: () => invoke<Zone[]>('zones_list'),
  zoneAdd: (name) => invoke<Zone>('zone_add', { name }),
  zoneDelete: (id) => invoke<void>('zone_delete', { id }),

  paletteSearch: (query) => invoke<PaletteHit[]>('palette_search', { query }),

  linkedState: () => invoke<LinkedState>('linked_state'),
  linkedRefreshBlog: () => invoke<SnapshotView[]>('linked_refresh_blog'),
  linkedRefreshMedicine: () => invoke<SnapshotView | null>('linked_refresh_medicine'),
  linkedCheckOnline: () => invoke<SnapshotView[]>('linked_check_online'),
  linkedRestockTodo: (name, daysLeft) =>
    invoke<Todo>('linked_restock_todo', { name, daysLeft }),
  linkedSetBlogDir: (path) => invoke<SourcePaths>('linked_set_blog_dir', { path }),

  backupStatus: () => invoke<BackupStatus>('backup_status'),
  backupNow: () => invoke<BackupOutcome>('backup_now'),
  backupSaveCredentials: (email, password) =>
    invoke<BackupStatus>('backup_save_credentials', { email, password }),
  backupClearCredentials: () => invoke<BackupStatus>('backup_clear_credentials'),
  backupListRemote: () => invoke<RemoteFile[]>('backup_list_remote'),
  backupTestConnection: () => invoke<number>('backup_test_connection'),
  backupRestore: (name) => invoke<BackupOutcome>('backup_restore', { name }),
  backupSelfTest: (email, password) => invoke<boolean>('backup_self_test', { email, password }),
}
