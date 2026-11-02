/**
 * mock 后端与真后端的行为一致性（批次 9 的第 11 条：用户选的是「逐条镜像」）。
 *
 * 为什么这些测试要**直接 import mockApi**：视图测试全都 `vi.mock('../api')`，
 * vitest 根本不经过 mock.ts —— 于是这里的每一条偏差都骗得过测试，只骗不过
 * 在浏览器里（`npm run dev` + `?mock=1`）做验收的人。下面每条都对着 Rust 的那一行写。
 */
import { beforeEach, describe, expect, it } from 'vitest'

import { mockApi } from './mock'

async function titles(query: string): Promise<string[]> {
  return (await mockApi.paletteSearch(query)).map((h) => h.label)
}

beforeEach(() => {
  localStorage.clear()
})

describe('空标题：真后端拒绝，mock 也必须拒绝', () => {
  it('建一条空标题的待办会抛，而不是存下一条空白', async () => {
    // repos/todos.rs 的 create：title.trim().is_empty() → Err("待办标题不能为空")
    await expect(mockApi.todosCreate({ title: '   ', note: '', zone: 'inbox' })).rejects.toThrow(
      '待办标题不能为空',
    )
    const before = await mockApi.todosAll()
    const made = await mockApi.todosCreate({ title: '  真标题  ', note: '' })
    expect(made.title, '存的要 trim 过').toBe('真标题')
    expect((await mockApi.todosAll()).length).toBe(before.length + 1)
  })

  it('改名改成空白同样拒绝', async () => {
    const t = await mockApi.todosCreate({ title: '别改成空白', note: '' })
    await expect(mockApi.todosUpdate(t.id, { title: '  ' })).rejects.toThrow('待办标题不能为空')
    const kept = await mockApi.todosGet(t.id)
    expect(kept?.title, '拒绝的那次不该已经把标题写坏').toBe('别改成空白')
  })
})

describe('命令面板：每类各 8 条、各有顺序（commands/palette.rs）', () => {
  it('待办与速达各自封顶，不是混在一起砍前 16', async () => {
    for (let i = 0; i < 12; i++) await mockApi.todosCreate({ title: `QQPANEL 待办 ${i}`, note: '' })
    for (let i = 0; i < 12; i++) await mockApi.resourceAddFromPath(`C:/x/QQPANEL${i}.txt`, null)

    const hits = await mockApi.paletteSearch('QQPANEL')
    const todos = hits.filter((h) => h.kind === 'todo')
    const resources = hits.filter((h) => h.kind === 'resource')
    expect(todos.length).toBe(8)
    expect(resources.length).toBe(8)
    expect(hits.length).toBe(16)

    // 混砍前 16 的写法会把速达整类挤掉：待办排在前面时就再也看不到速达
    expect(await titles('不存在的那种词')).toEqual([])
  })

  it('速达按使用次数降序（真后端是 ORDER BY use_count DESC LIMIT 8）', async () => {
    const a = await mockApi.resourceAddFromPath('C:/x/ORDERA.txt', null)
    await mockApi.resourceAddFromPath('C:/x/ORDERB.txt', null)
    await mockApi.resourceLaunch(a.id)
    await mockApi.resourceLaunch(a.id)

    const labels = await titles('ORDER')
    expect(labels[0], '用得多的那条该排在前面').toBe('ORDERA')
  })
})

describe('药箱目录禁令：分段相等，不是子串（dav_readonly::check_dir）', () => {
  const bad = [
    'https://dav.jianguoyun.com/dav/workbench-backup/',
    // 末尾不带斜杠的真填法：子串判据漏得掉它
    'https://dav.jianguoyun.com/dav/workbench-backup',
    // 大小写不同的真填法：子串判据也漏得掉它
    'https://dav.jianguoyun.com/dav/Workbench-Sync/',
  ]

  it.each(bad)('拒绝 %s', async (dir) => {
    await expect(mockApi.linkedSetMedicineDir(dir)).rejects.toThrow(/不是药箱的同步目录/)
  })

  it('药箱自己的地址要放行，留空是回默认而不是报错', async () => {
    expect(await mockApi.linkedSetMedicineDir('https://dav.jianguoyun.com/dav/medicine-sync/')).toBe(
      'https://dav.jianguoyun.com/dav/medicine-sync/',
    )
    expect(await mockApi.linkedSetMedicineDir('   ')).toBe('https://dav.jianguoyun.com/dav/medicine-sync/')
  })

  it('百分号编码的目录名也要按还原后的名字比', async () => {
    // workbench-backup 编码成 %77%6F%72%6B%62%65%6E%63%68%2D%62%61%63%6B%75%70
    await expect(
      mockApi.linkedSetMedicineDir('https://dav.jianguoyun.com/dav/%77orkbench-backup/'),
    ).rejects.toThrow(/不是药箱的同步目录/)
  })
})

describe('补买待办的文案：三支分叉（commands/linked.rs）', () => {
  it('≤0 说已用完、其余留一位小数、null 才是待确认', async () => {
    const cases: [number | null, string][] = [
      [null, '补买 钙片（库存待确认）'],
      [0, '补买 钙片（已用完）'],
      [-1.7, '补买 钙片（已用完）'],
      [2.54, '补买 钙片（剩余 2.5 天）'],
      [3, '补买 钙片（剩余 3 天）'],
    ]
    for (const [daysLeft, want] of cases) {
      const t = await mockApi.linkedRestockTodo('钙片', daysLeft)
      expect(t.title).toBe(want)
    }
  })
})

describe('便签与倒计时：删除是墓碑，不是把这行抹掉', () => {
  it('清空便签后列表里没有它，再打字是新建一条而不是复活墓碑', async () => {
    // 槽位 7 挑一个夹具里没占过的：seed 里已经有便签，断言要写"这一格/这一条"而不是"整个列表空"
    const first = await mockApi.stickySave(7, '第一段')
    expect(await mockApi.stickyClear(7)).toBe(true)
    expect(await mockApi.stickiesList()).not.toContainEqual(expect.objectContaining({ id: first.id }))
    // 空着的那一格再清一次，真后端读不到未删除的行 → false
    expect(await mockApi.stickyClear(7)).toBe(false)

    const again = await mockApi.stickySave(7, '第二段')
    expect(again.id).not.toBe(first.id)
    const live = (await mockApi.stickiesList()).filter((s) => s.slot === 7)
    expect(live.map((s) => s.content)).toEqual(['第二段'])
    expect(live[0].deletedAt).toBeNull()
  })

  it('删掉的倒计时不再出现，也对它操作会抛', async () => {
    const c = await mockApi.countdownAdd('喝水', 'duration', { minutes: 25 })
    await mockApi.countdownDelete(c.id)
    const listed = await mockApi.countdownsList()
    expect(listed.map((x) => x.id)).not.toContain(c.id)
    expect(listed.every((x) => x.deletedAt === null), '列表里不该有墓碑：读侧要筛掉已删除的').toBe(true)
    await expect(mockApi.countdownStart(c.id)).rejects.toThrow('这个倒计时不存在或已被删除')
    // 重复删除不该抛，也不该留下第二份墓碑
    await mockApi.countdownDelete(c.id)
    expect((await mockApi.countdownsList()).map((x) => x.id)).not.toContain(c.id)
  })
})

describe('速达换分区 / 写备注 / 文件夹挪动（批次 10 新接上的三个入口）', () => {
  it('换分区对不存在的条目要抛：真后端 get 找不到就 Err，静默 no-op 会让界面以为挪成功了', async () => {
    // repos/resources.rs 的 move_to_zone：get(conn, id)? .ok_or_else(|| "速达条目不存在")
    await expect(mockApi.resourceMove('no-such-id', null)).rejects.toThrow('速达条目不存在')

    const zone = await mockApi.zoneAdd('开发')
    const r = await mockApi.resourceAddFromPath('C:\Tools\a.exe', null)
    await mockApi.resourceMove(r.id, zone.id)
    const listed = await mockApi.resourcesList()
    expect(listed.find((x) => x.id === r.id)?.zoneId).toBe(zone.id)
  })

  it('换分区要涨 rev：那是同步的比较依据，改了不涨等于没改', async () => {
    const r = await mockApi.resourceAddFromPath('C:\Tools\b.exe', null)
    const before = r.rev
    const zone = await mockApi.zoneAdd('生活')
    await mockApi.resourceMove(r.id, zone.id)
    const after = (await mockApi.resourcesList()).find((x) => x.id === r.id)!
    expect(after.rev).toBeGreaterThan(before)
  })

  it('备注：全空白就是清除，其余原样存（真后端判 trim 为空、存的是原文）', async () => {
    const r = await mockApi.resourceAddFromPath('C:\Tools\c.exe', null)
    await mockApi.resourceSetSecret(r.id, '  钥匙在抽屉  ')
    expect(await mockApi.resourceRevealSecret(r.id), '存的是原文，不是 trim 过的').toBe('  钥匙在抽屉  ')
    await mockApi.resourceSetSecret(r.id, '   ')
    expect(await mockApi.resourceRevealSecret(r.id), '清除后解出来是空串，不是 null').toBe('')
  })

  it('文件夹挪进自己或自己的后代要抛（note_folders::move_to 同一条拒绝）', async () => {
    const a = await mockApi.folderAdd('A', null)
    const b = await mockApi.folderAdd('B', null)
    const a1 = await mockApi.folderAdd('A1', a.id)
    await expect(mockApi.folderMove(a.id, a.id)).rejects.toThrow('不能把文件夹挪进它自己或它的子目录里')
    await expect(mockApi.folderMove(a.id, a1.id)).rejects.toThrow('不能把文件夹挪进它自己或它的子目录里')
    const moved = await mockApi.folderMove(a.id, b.id)
    expect(moved.parentId).toBe(b.id)
  })
})
