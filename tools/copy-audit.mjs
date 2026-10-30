// 界面文案审计仪（PLAN §8.9）：把模板里那些"说给人听的长句"揪出来分类。
// 不是门禁 —— 有些长句是该搬走而不是该删掉，判据要人看。跑法：node tools/copy-audit.mjs
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

const walk = (dir, out = []) => {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name)
    if (e.isDirectory()) walk(p, out)
    else if (p.endsWith('.vue')) out.push(p)
  }
  return out
}

/** 超过这个长度就不像"操作提示"，像解释或辩护 */
const LONG = 14

const rows = []
for (const file of walk('src')) {
  const src = readFileSync(file, 'utf8')
  const tpl = src.split('<template>')[1]?.split('</template>')[0]
  if (!tpl) continue
  tpl.split('\n').forEach((line, i) => {
    // 文本节点与属性值里的中文散文（跳过 {{ }} 插值本身）
    for (const m of line.matchAll(/[>"]([^<>{}"]*[一-龥][^<>{}"]*)[<"]/g)) {
      const text = m[1].trim()
      if (text.length >= LONG) rows.push({ file: file.replace(/\\/g, '/'), line: i + 1, text })
    }
  })
}

const byFile = new Map()
for (const r of rows) byFile.set(r.file, [...(byFile.get(r.file) ?? []), r])
console.log(`${rows.length} 处 ≥${LONG} 字的界面散文，分布在 ${byFile.size} 个文件\n`)
for (const [file, list] of [...byFile].sort((a, b) => b[1].length - a[1].length)) {
  console.log(`── ${file}  (${list.length})`)
  for (const r of list) console.log(`   :${r.line} ${r.text}`)
}
