import { ref } from 'vue'

import { api } from '../api'
import type { Note, NoteFolder } from '../api/types'

/**
 * 笔记的读模型 + 编辑态。
 *
 * 正文落盘走 600 ms 防抖，且**只有正文与标题变了才发请求**：每次按键都写库会把
 * `rev` 与「最后写入时间」打成一堆无意义的抖动，而这些字段将来要给同步用。
 */
const notes = ref<Note[]>([])
const trashed = ref<Note[]>([])
const folders = ref<NoteFolder[]>([])
const selectedId = ref<string | null>(null)
const scope = ref<{ kind: 'all' } | { kind: 'folder'; id: string } | { kind: 'trash' }>({ kind: 'all' })

const DEBOUNCE = 600
let timer: number | undefined
const dirty = new Map<string, { title?: string; bodyMd?: string }>()

export const EDITOR_MODES = ['split', 'source', 'preview'] as const
export type EditorMode = (typeof EDITOR_MODES)[number]
const mode = ref<EditorMode>('split')

function sortLive(list: Note[]): Note[] {
  return [...list].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt)
}

export function useNotes() {
  const selected = () => notes.value.find((n) => n.id === selectedId.value) ?? null

  async function load() {
    const folder = scope.value.kind === 'folder' ? scope.value.id : null
    // 回收站的计数要在侧栏显示，所以不管当前看哪个作用域都得带着它
    const [list, trash, tree] = await Promise.all([
      api.notesList(folder),
      api.notesTrashed(),
      api.foldersList(),
    ])
    notes.value = list
    trashed.value = trash
    folders.value = tree
    if (!notes.value.some((n) => n.id === selectedId.value)) {
      selectedId.value = sortLive(notes.value)[0]?.id ?? null
    }
  }

  /** 编辑器里的当前文本 —— 防抖窗口内以本地值为准，别让库里的旧值把它冲掉。 */
  function bodyOf(id: string, fallback = ''): string {
    return dirty.get(id)?.bodyMd ?? selected()?.bodyMd ?? fallback
  }

  function titleOf(id: string): string {
    return dirty.get(id)?.title ?? selected()?.title ?? ''
  }

  function scheduleSave(patch: { title?: string; bodyMd?: string }) {
    const id = selectedId.value
    if (!id) return
    const prev = dirty.get(id) ?? {}
    dirty.set(id, { ...prev, ...patch })
    // 本地立刻反映，列表才不会被防抖窗口里的旧值显示成"没改"
    if (patch.title !== undefined) {
      const n = notes.value.find((v) => v.id === id)
      if (n) n.title = patch.title
    }
    if (timer !== undefined) window.clearTimeout(timer)
    timer = window.setTimeout(() => void flush(), DEBOUNCE)
  }

  /** 切笔记、切视图、失焦、收进托盘之前都要调它。 */
  async function flush() {
    if (timer !== undefined) {
      window.clearTimeout(timer)
      timer = undefined
    }
    const pending = [...dirty.entries()]
    dirty.clear()
    for (const [id, patch] of pending) {
      try {
        const saved = await api.notesUpdate(id, patch)
        const at = notes.value.findIndex((n) => n.id === id)
        if (at >= 0) notes.value[at] = saved
      } catch (e) {
        console.error('笔记保存失败', e)
      }
    }
  }

  async function open(id: string) {
    if (id !== selectedId.value) await flush()
    selectedId.value = id
  }

  async function create(title = '无标题笔记') {
    await flush()
    const created = await api.notesCreate({
      title,
      bodyMd: '',
      folderId: scope.value.kind === 'folder' ? scope.value.id : null,
    })
    selectedId.value = created.id
    scope.value = { kind: 'all' }
    await load()
    return created
  }

  async function setTags(tags: string[]) {
    const id = selectedId.value
    if (!id) return
    await flush()
    await api.notesSetTags(id, tags)
    await load()
  }

  async function togglePin() {
    const note = selected()
    if (!note) return
    await flush()
    const saved = await api.notesUpdate(note.id, { pinned: !note.pinned })
    const at = notes.value.findIndex((n) => n.id === note.id)
    if (at >= 0) notes.value[at] = saved
    notes.value = sortLive(notes.value)
  }

  async function trash() {
    const id = selectedId.value
    if (!id) return
    await flush()
    await api.notesTrash(id)
    selectedId.value = null
    await load()
  }

  async function restore(id: string) {
    await api.notesRestore(id)
    await load()
  }

  async function purge(id: string) {
    await api.notesPurge(id)
    await load()
  }

  async function addFolder(name: string) {
    await api.folderAdd(name, null)
    await load()
  }

  async function removeFolder(id: string) {
    const moved = await api.folderDelete(id)
    if (scope.value.kind === 'folder' && scope.value.id === id) scope.value = { kind: 'all' }
    await load()
    return moved
  }

  async function renameFolder(id: string, name: string) {
    await api.folderRename(id, name)
    await load()
  }

  function go(next: typeof scope.value) {
    scope.value = next
    void (async () => {
      await flush()
      await load()
    })()
  }

  function setMode(next: EditorMode) {
    mode.value = next
  }

  return {
    notes,
    trashed,
    folders,
    selectedId,
    scope,
    mode,
    go,
    load,
    flush,
    open,
    create,
    setTags,
    togglePin,
    trash,
    restore,
    purge,
    addFolder,
    removeFolder,
    renameFolder,
    scheduleSave,
    selected,
    bodyOf,
    titleOf,
    setMode,
  }
}
