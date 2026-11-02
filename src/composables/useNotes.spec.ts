import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { Note } from '../api/types'

const h = vi.hoisted(() => ({
  notesList: vi.fn(),
  notesUpdate: vi.fn(),
}))

vi.mock('../api', () => ({
  usingMock: false,
  api: {
    notesList: (...a: unknown[]) => h.notesList(...a),
    notesTrashed: async () => [],
    foldersList: async () => [],
    notesUpdate: (...a: unknown[]) => h.notesUpdate(...a),
  },
}))

/**
 * 状态是模块级的（列表与 `dirty` 都是单例），用例之间会互相留残留 ——
 * 每个用例重新导入一份模块，才不会靠"上一个用例恰好没留下东西"来成立。
 */
async function fresh() {
  vi.resetModules()
  const mod = await import('./useNotes')
  return mod.useNotes()
}

/** `bodyMd` 是这两条测试唯一关心的字段，其余字段给全免得哪天加断言又要重写夹具。 */
function note(id: string, bodyMd: string): Note {
  return {
    id,
    rev: 1,
    createdAt: 1,
    createdBy: 'pc-test',
    updatedAt: 1,
    updatedBy: 'pc-test',
    deletedAt: null,
    title: `笔记 ${id}`,
    bodyMd,
    folderId: null,
    pinned: false,
    trashedAt: null,
    createdDay: 20261101,
    tags: [],
  }
}

describe('笔记保存的失败路径', () => {
  beforeEach(() => {
    h.notesList.mockReset()
    h.notesUpdate.mockReset()
  })

  it('保存失败时刚写的正文还在，界面也能看见没存上', async () => {
    h.notesList.mockResolvedValue([note('a1', '库里的旧正文')])
    const n = await fresh()
    await n.load()
    n.selectedId.value = 'a1'
    n.scheduleSave({ bodyMd: '刚敲的一段话' })

    h.notesUpdate.mockRejectedValueOnce(new Error('IPC 挂了'))
    await n.flush()

    // 修复前：dirty 在 await 之前就被整表清掉，这里会回落到库里的旧正文
    expect(n.bodyOf('a1')).toBe('刚敲的一段话')
    expect(n.saveError.value).toContain('没保存上')
  })

  it('flush 进行中又改了正文，成功回来也不能把新改动一起清掉', async () => {
    h.notesList.mockResolvedValue([note('b1', '库里的旧正文')])
    const n = await fresh()
    await n.load()
    n.selectedId.value = 'b1'
    n.scheduleSave({ bodyMd: '第一段' })

    let release!: (v: Note) => void
    h.notesUpdate.mockImplementationOnce(() => new Promise<Note>((resolve) => { release = resolve }))
    const inflight = n.flush()

    // 防抖窗口里继续打字：dirty 里换成了一个新的 patch 对象
    n.scheduleSave({ bodyMd: '第二段' })
    release(note('b1', '第一段'))
    await inflight

    expect(n.bodyOf('b1')).toBe('第二段')

    h.notesUpdate.mockResolvedValueOnce(note('b1', '第二段'))
    await n.flush()
    expect(h.notesUpdate).toHaveBeenLastCalledWith('b1', { bodyMd: '第二段' })
    expect(n.saveError.value).toBeNull()
  })
})
