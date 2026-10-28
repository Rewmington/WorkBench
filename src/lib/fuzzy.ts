/**
 * 命令面板的模糊匹配。40 行自写而不是引 fuse：
 * 中文没有词边界，「牙医」必须是包含匹配赢，而英文要奖励词首与连续 —— 这两条
 * 现成库都调不出来，而这里判错的代价是「找不到自己写的待办」。
 */

const WORD_START = /[\s\-_/\\.·、（(]/

function boundaries(text: string): Set<number> {
  const out = new Set<number>([0])
  for (let i = 0; i < text.length; i += 1) {
    if (WORD_START.test(text[i] ?? '')) out.add(i + 1)
  }
  return out
}

/** 返回 0 表示不匹配；越大越靠前。 */
export function score(query: string, text: string): number {
  const q = query.trim().toLowerCase()
  if (!q || !text) return 0
  const t = text.toLowerCase()

  const direct = t.indexOf(q)
  if (direct >= 0) {
    let s = 120 - direct
    if (boundaries(text).has(direct)) s += 40
    if (direct === 0) s += 20
    return s
  }

  // 子序列兜底：英文缩写式输入（"yz" 命中 "预约牙医" 的拼音首字母不行，但 "yay" 命中 "牙医" 也不该指望）
  // 所以这里只服务一种真实场景：英文短语漏打中间字符，如 "vscode" 命中 "VS Code"
  let ti = 0
  let s = 0
  let streak = 0
  let last = -2
  for (const ch of q) {
    const found = t.indexOf(ch, ti)
    if (found < 0) return 0
    s += found === last + 1 ? 6 + streak : 2
    streak = found === last + 1 ? streak + 1 : 0
    if (boundaries(text).has(found)) s += 3
    last = found
    ti = found + 1
  }
  return Math.max(1, s - Math.floor(t.length / 8))
}

export function ranked<T>(query: string, items: T[], text: (item: T) => string, limit = 20): T[] {
  if (!query.trim()) return items.slice(0, limit)
  return items
    .map((item) => ({ item, s: score(query, text(item)) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .slice(0, limit)
    .map((x) => x.item)
}
