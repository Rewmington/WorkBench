/**
 * 文件夹的三个入口（改名 / 移到 / 删除）。
 *
 * 这片是 2026-11-02 扫第二层时才露出来的：`folder_rename`、`folder_delete`、`folder_move`
 * 三条命令 + composable 里的 `renameFolder`/`removeFolder`/`moveFolder` 一直都在，
 * 模板里一个按钮都没有 —— 文件夹只能建，建歪了既改不了也删不掉，更不能挪。
 * 与用户报的「速达创建分区了不能删除」是同一类账，而且这一片更大。
 *
 * 第 6 条钉的是顺手修掉的另一半：老模板写死「根 + 直接子级」两层，
 * 一旦有了「移到」，把 A 挪进 B（B 本身是子级）就出现第三层，老模板会把它渲染没了 ——
 * 看起来像数据丢了。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

import type { Note, NoteFolder } from '../api/types'

const folders = vi.fn()
const folderRename = vi.fn()
const folderMove = vi.fn()
const folderDelete = vi.fn()
const notesList = vi.fn()
const notesTrashed = vi.fn()

vi.mock('../api', () => ({
  api: {
    notesList: (f: string | null) => notesList(f),
    notesTrashed: () => notesTrashed(),
    foldersList: () => folders(),
    folderAdd: async (name: string) => ({ id: `f-new-${name}`, name, parentId: null }),
    folderRename: (id: string, name: string) => folderRename(id, name),
    folderMove: (id: string, parentId: string | null) => folderMove(id, parentId),
    folderDelete: (id: string) => folderDelete(id),
    notesCreate: async () => ({}),
    notesUpdate: async (id: string) => ({ id }),
    notesSetTags: async () => {},
    notesTrash: async () => {},
    notesRestore: async () => {},
    notesPurge: async () => {},
    openUrl: async () => {},
    settingsGet: async () => null,
    settingsSet: async () => {},
  },
  usingMock: true,
  runningInBrowser: false,
}))

const Notes = (await import('./Notes.vue')).default

const F = (id: string, name: string, parentId: string | null): NoteFolder => ({
  id,
  name,
  parentId,
  sort: 100,
  rev: 1,
  createdAt: 1,
  createdBy: 'pc',
  updatedAt: 1,
  updatedBy: 'pc',
  deletedAt: null,
  noteCount: 0,
})

const NOTE: Note = {
  id: 'n-1',
  rev: 1,
  createdAt: 1,
  createdBy: 'pc',
  updatedAt: 1,
  updatedBy: 'pc',
  deletedAt: null,
  title: '周报草稿',
  bodyMd: '正文',
  folderId: 'f-a',
  pinned: false,
  trashedAt: null,
  createdDay: 20261102,
  tags: [],
}

function tree() {
  return [F('f-a', 'A', null), F('f-b', 'B', null), F('f-a1', 'A1', 'f-a'), F('f-a11', 'A11', 'f-a1')]
}

beforeEach(() => {
  for (const f of [folders, folderRename, folderMove, folderDelete, notesList, notesTrashed]) f.mockReset()
  folders.mockResolvedValue(tree())
  notesList.mockResolvedValue([{ ...NOTE }])
  notesTrashed.mockResolvedValue([])
  folderRename.mockResolvedValue(F('f-a', 'A 改过', null))
  folderMove.mockResolvedValue(F('f-a', 'A', 'f-b'))
  folderDelete.mockResolvedValue(2)
})

async function opened() {
  const w = mount(Notes)
  await flushPromises()
  return w
}

/** 某一行的「…」按钮（行本身是 .trow，里面第一个 .ti 是文件夹名） */
function mgFor(w: Awaited<ReturnType<typeof opened>>, name: string) {
  const row = w.findAll('.trow').find((r) => r.find('.ti')?.text().startsWith(name))
  return row!.find('.mg')
}

describe('文件夹菜单', () => {
  it('每一行都有一颗「…」，点开是改名 / 移到 / 删除', async () => {
    const w = await opened()
    expect(w.findAll('.mg').length).toBe(4)
    await mgFor(w, 'A').trigger('click')
    const acts = w.findAll('.fa').map((b) => b.text())
    expect(acts).toEqual(['改名', '移到…', '删除'])
  })

  it('改名：输入框带着现名，回车才写库', async () => {
    const w = await opened()
    await mgFor(w, 'A').trigger('click')
    await w.findAll('.fa')[0].trigger('click')
    const input = w.find('.rin')
    expect((input.element as HTMLInputElement).value).toBe('A')
    await input.setValue('A 改过')
    await input.trigger('keydown.enter')
    await flushPromises()
    expect(folderRename).toHaveBeenCalledWith('f-a', 'A 改过')
  })

  it('改名：清空回车不写库（后端也会拒，界面上先别发）', async () => {
    const w = await opened()
    await mgFor(w, 'A').trigger('click')
    await w.findAll('.fa')[0].trigger('click')
    const input = w.find('.rin')
    await input.setValue('   ')
    await input.trigger('keydown.enter')
    await flushPromises()
    expect(folderRename).not.toHaveBeenCalled()
  })

  it('名字一个字没改就别写库：白涨一条 rev 会被同步当成一次真改动带给手机', async () => {
    const w = await opened()
    await mgFor(w, 'A').trigger('click')
    await w.findAll('.fa')[0].trigger('click')
    const input = w.find('.rin')
    await input.setValue('A')
    await input.trigger('keydown.enter')
    await flushPromises()
    expect(folderRename).not.toHaveBeenCalled()
  })

  it('移到：候选里没有它自己，也没有它的整棵子树', async () => {
    const w = await opened()
    await mgFor(w, 'A').trigger('click')
    await w.findAll('.fa')[1].trigger('click')
    const targets = w.findAll('.tgt').map((b) => b.text())
    expect(targets).toEqual(['顶层', 'B'])
  })

  it('移到：点目标就发 folderMove，点「顶层」发 null', async () => {
    const w = await opened()
    await mgFor(w, 'A1').trigger('click')
    await w.findAll('.fa')[1].trigger('click')
    const targets = w.findAll('.tgt')
    await targets[0].trigger('click')
    await flushPromises()
    expect(folderMove).toHaveBeenCalledWith('f-a1', null)
  })

  it('删除要按两下：第一下只是把它变成确认，第二下才删', async () => {
    const w = await opened()
    await mgFor(w, 'B').trigger('click')
    await w.findAll('.fa')[2].trigger('click')
    expect(folderDelete).not.toHaveBeenCalled()
    const confirm = w.findAll('.y-chip').find((c) => c.text() === '确认删除')
    await confirm!.trigger('click')
    await flushPromises()
    expect(folderDelete).toHaveBeenCalledWith('f-b')
  })

  it('删完说的是里面的东西去了哪儿，不是光一句「已删除」', async () => {
    const w = await opened()
    await mgFor(w, 'B').trigger('click')
    await w.findAll('.fa')[2].trigger('click')
    const confirm = w.findAll('.y-chip').find((c) => c.text() === '确认删除')
    await confirm!.trigger('click')
    await flushPromises()
    expect(w.find('.fnote').text()).toContain('2 篇笔记退回上一层')
  })

  it('第三层也照样列出来（老模板只写两层，有了「移到」就会把更深的藏掉）', async () => {
    const w = await opened()
    const names = w.findAll('.ti').map((b) => b.text().replace(/\s+\d+$/, ''))
    expect(names).toContain('A11')
    expect(w.find('.ti.kid2').text()).toContain('A11')
  })

  it('后端拒绝时（比如库里已经有环）把那句话写出来，不是悄悄不动', async () => {
    folderMove.mockRejectedValue(new Error('不能把文件夹挪进它自己或它的子目录里'))
    const w = await opened()
    await mgFor(w, 'A').trigger('click')
    await w.findAll('.fa')[1].trigger('click')
    await w.findAll('.tgt')[1].trigger('click')
    await flushPromises()
    expect(w.find('.fnote').text()).toContain('不能把文件夹挪进它自己或它的子目录里')
  })
})
