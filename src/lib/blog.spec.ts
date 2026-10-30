import { describe, expect, it } from 'vitest'

import { mergeFromPayloads, mergedProjects, type Projectish } from './blog'

const wrap = <T extends Projectish>(items: T[]) => ({ items, count: items.length, meta: {} })

/** 2026-10-30 线上那两份清单原样抄下来：wmddd.online/content/{projects,github-repos}.json */
const HAND: Projectish[] = [
  {
    name: 'Rewmington Desk',
    description: '基于 Next.js 16 的个人工作台，毛玻璃拟态设计 + Bento Grid 布局',
    tags: ['Next.js', 'React', 'Tailwind CSS'],
    url: 'https://github.com/Rewmington/Rewmington-Blog',
    status: '编码中',
  },
]
const REPOS: Projectish[] = [
  {
    name: 'Rewmington-Blog',
    description: '个人工作台',
    url: 'https://github.com/Rewmington/Rewmington-Blog',
    language: 'TypeScript',
    stars: 0,
    pushedAt: '2026-10-30T11:21:46Z',
    archived: false,
  },
  {
    name: 'medtrack',
    description: '',
    url: 'https://github.com/Rewmington/medtrack',
    language: 'Dart',
    stars: 0,
    pushedAt: '2026-10-20T14:38:40Z',
    archived: false,
  },
  {
    name: 'SoundBeam',
    url: 'https://github.com/Rewmington/SoundBeam',
    language: 'Python',
    stars: 1,
    pushedAt: '2026-09-08T10:40:40Z',
    archived: false,
  },
  {
    name: 'deskwhale-harness',
    url: 'https://github.com/Rewmington/deskwhale-harness',
    language: 'TypeScript',
    stars: 1,
    pushedAt: '2026-08-18T02:18:15Z',
    archived: false,
  },
]

describe('项目卡按站点 src/lib/projects.ts 的口径合并', () => {
  it('标题与顺序和 /projects/ 那一屏相同', () => {
    expect(mergedProjects(HAND, REPOS).map((p) => p.name)).toEqual([
      'Rewmington Desk',
      'medtrack',
      'SoundBeam',
      'deskwhale-harness',
    ])
  })

  /** 这条就是「软件和网页对不上」的根因：以前手写在前、同 url 的仓库整条删掉，
   *  于是被覆盖那张卡只剩手写那几样，网页上明明还有语言与星数。 */
  it('覆盖只改显示字段，仓库行的语言/星数/最近 push 必须留下', () => {
    const desk = mergedProjects(HAND, REPOS)[0]
    expect(desk.name).toBe('Rewmington Desk')
    expect(desk.description).toBe('基于 Next.js 16 的个人工作台，毛玻璃拟态设计 + Bento Grid 布局')
    expect(desk.tags).toEqual(['Next.js', 'React', 'Tailwind CSS'])
    expect(desk.language).toBe('TypeScript')
    expect(desk.pushedAt).toBe('2026-10-30T11:21:46Z')
  })

  it('手写没给状态的仓库，状态按是否归档推出来', () => {
    const [medtrack, archived] = mergedProjects([], [
      REPOS[1],
      { ...REPOS[2], archived: true },
    ])
    expect(medtrack.status).toBe('编码中')
    expect(archived.status).toBe('已完成')
  })

  it('判同看 url 末段的仓库名：大小写、尾斜杠、.git 都算同一个仓库', () => {
    const hand: Projectish[] = [{ name: '乙', url: 'https://GitHub.com/a/b.git' }]
    const repos: Projectish[] = [
      { name: 'b', url: 'https://github.com/a/b/', language: 'Go' },
      { name: 'c', url: 'https://github.com/a/c' },
    ]
    const out = mergedProjects(hand, repos)
    expect(out.map((p) => p.name)).toEqual(['乙', 'c'])
    expect(out[0].language).toBe('Go')
  })

  it('GitHub 上没有的手写条目当手工项目，排在自动项后面且不丢', () => {
    const hand: Projectish[] = [...HAND, { name: '没开源的东西', url: 'https://example.com/private' }]
    const out = mergedProjects(hand, REPOS)
    expect(out.map((p) => p.name)).toContain('没开源的东西')
    expect(out[out.length - 1].name).toBe('没开源的东西')
    expect(out).toHaveLength(5)
  })

  it('没有 url 的条目没法判同，一律照原样列出来', () => {
    const hand: Projectish[] = [{ name: '手写没链接' }]
    const repos: Projectish[] = [{ name: '仓库没链接' }, { name: '也没链接' }]
    expect(mergedProjects(hand, repos).map((p) => p.name)).toEqual([
      '仓库没链接',
      '也没链接',
      '手写没链接',
    ])
  })

  it('两边缓存缺任何一边都不报错', () => {
    expect(mergeFromPayloads(wrap(HAND), null).map((p) => p.name)).toEqual(['Rewmington Desk'])
    expect(mergeFromPayloads(null, wrap(REPOS))).toHaveLength(4)
    expect(mergeFromPayloads(null, null)).toEqual([])
  })
})
