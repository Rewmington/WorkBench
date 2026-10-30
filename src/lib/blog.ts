import type { BlogPayload } from '../api/types'

/**
 * 站点「项目」页（`src/lib/projects.ts`）的口径，2026-10-30 逐字对过：
 *
 * 1. **GitHub 仓库是主源**，`projects.json` 只是覆盖表 —— 它改显示名、简介、标签、状态，
 *    但**不会把仓库行顶掉**。所以被覆盖那张卡照样有语言 / ★ / 最近 push。
 * 2. 覆盖表里 GitHub 没有的条目 = 手工项目（没开源的），排在自动项**后面**。
 * 3. 没被覆盖的仓库：状态由是否归档推出来（已完成 / 编码中），进度留空。
 *
 * 原来这里做的是「手写在前 + 同 url 的仓库整条删掉」，看着条目数一样，实际上
 * `Rewmington Desk` 那张卡丢掉了仓库行的语言与星数 —— 和站点那一屏对不上。
 */
export interface Mergeable {
  name: string
  url?: string
}

/** 项目卡上会出现的字段：仓库行给语言/星数/最近 push，覆盖表给显示名/简介/标签/状态 */
export interface Projectish extends Mergeable {
  description?: string
  tags?: string[]
  status?: string
  language?: string
  stars?: number
  pushedAt?: string
  archived?: boolean
}

/** 覆盖表与仓库靠 url 末段的仓库名对上：`…/Rewmington-Blog` ↔ `Rewmington-Blog` */
function repoKey(url?: string): string {
  const parts = (url ?? '').trim().toLowerCase().split('/').filter(Boolean)
  return (parts.pop() ?? '').replace(/\.git$/, '')
}

/** 覆盖表只改这几样，其余（语言/星数/最近 push/是否归档）一律留仓库那份 */
function overlay<T extends Projectish>(repo: T, hand?: Projectish): T {
  if (!hand) return repo
  const tags = hand.tags?.length ? hand.tags : undefined
  const name = hand.name?.trim() ? hand.name : repo.name
  const status = hand.status?.trim() ? hand.status : repo.status
  return {
    ...repo,
    name,
    description: hand.description ?? repo.description,
    url: hand.url || repo.url,
    tags,
    status,
  }
}

/** 站点规则 3：手写没给状态的仓库，状态由是否归档推出来，不留空 */
function withStatus(p: Projectish): Projectish {
  return { ...p, status: p.status?.trim() ? p.status : p.archived ? '已完成' : '编码中' }
}

/** 与站点 `listProjects()` 同一条规则：仓库打头（含覆盖），手工项目补在后面 */
export function mergedProjects(hand: Projectish[], repos: Projectish[]): Projectish[] {
  const byKey = new Map(hand.map((h) => [repoKey(h.url), h]))
  const matched = new Set<string>()

  const fromRepos = repos.map((r) => {
    const key = repoKey(r.url)
    const h = key ? byKey.get(key) : undefined
    if (h) matched.add(key)
    return withStatus(overlay(r, h))
  })

  // 没对上手写条目的仓库保持原样；手写里 GitHub 没有的那几条排到后面。
  // 没有 url 的手写条目没法判同，只能当手工项目 —— 不许被吞掉。
  const manual = hand.filter((h) => {
    const key = repoKey(h.url)
    return !key || !matched.has(key)
  })

  return [...fromRepos, ...manual]
}

/** 从两份缓存里取出 items 再合并；任何一边缺失都只少一半，不报错。 */
export function mergeFromPayloads(
  projects: BlogPayload<Projectish> | null,
  repos: BlogPayload<Projectish> | null,
): Projectish[] {
  return mergedProjects(projects?.items ?? [], repos?.items ?? [])
}
