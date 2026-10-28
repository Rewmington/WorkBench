// 断言 src/style.css 里 PLAN §8.5 冻结的令牌值原样存在。
// 「只加不改」靠自觉是假的 —— 改一个值要回看全部部件，这条脚本是那个回看的自动化。
import { readFileSync } from 'node:fs'

const FILE = 'src/style.css'
const src = readFileSync(FILE, 'utf8')

/** 冻结表：名称 → 暗色下的期望值（亮色另表） */
const FROZEN = {
  '--bg': '#080b0a',
  '--panel': '#0d1110',
  '--card': '#131816',
  '--card-sel': '#f6faf7',
  '--sel-muted': '#4b5450',
  '--t1': '#f1f5f3',
  '--t2': '#a3ada8',
  '--t3': '#6b7571',
  '--accent': '#2ee27a',
  '--r': '14px',
}

/** 亮色主题必须把默认强调色降下来，否则白底上 #2ee27a 对比度不够 */
const LIGHT = {
  '--bg': '#eef1f0',
  '--panel': '#f8faf9',
  '--card': '#ffffff',
  '--accent': '#059669',
}

const BASELINE = [
  ['13px 基线', /font:\s*400 13px\/1\.5/],
  ['tabular-nums', /font-variant-numeric:\s*tabular-nums/],
  ['全局禁选中', /user-select:\s*none/],
  ['PingFang / 雅黑回退', /'PingFang SC',\s*'Microsoft YaHei'/],
  ['6px 细滚动条', /::-webkit-scrollbar\s*\{\s*width:\s*6px/],
]

function parseDecls(block) {
  const out = new Map()
  for (const m of block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    out.set(m[1], m[2].trim().toLowerCase())
  }
  return out
}

function section(startNeedle, endNeedle) {
  const from = src.indexOf(startNeedle)
  if (from < 0) return ''
  const to = endNeedle ? src.indexOf(endNeedle, from + startNeedle.length) : src.length
  return src.slice(from, to < 0 ? src.length : to)
}

const dark = parseDecls(section(':root', '[data-mode'))
const light = parseDecls(section("[data-mode='light']", '[data-preset'))

const problems = []

for (const [name, want] of Object.entries(FROZEN)) {
  const got = dark.get(name)
  if (got !== want) problems.push(`${FILE}: ${name} = ${got ?? '(缺失)'}，冻结值是 ${want}`)
}

for (const [name, want] of Object.entries(LIGHT)) {
  const got = light.get(name)
  if (got !== want) problems.push(`${FILE} 亮色: ${name} = ${got ?? '(缺失)'}，期望 ${want}`)
}

// 描边：rgba 的写法会被 prettier 展开空格，归一化后再比
const normalized = src.replace(/\s+/g, '').replace(/0\.(?=\d)/g, '.')
if (!normalized.includes('rgba(255,255,255,.07)')) {
  problems.push(`${FILE}: 1px 描边 rgba(255,255,255,.07) 不在了`)
}

for (const [label, re] of BASELINE) {
  if (!re.test(src)) problems.push(`${FILE}: 缺基线条目 —— ${label}`)
}

if (problems.length) {
  console.error('令牌核对失败：\n' + problems.map((p) => '  · ' + p).join('\n'))
  process.exit(1)
}

console.log(`令牌冻结表核对通过：${Object.keys(FROZEN).length} 暗色 + ${Object.keys(LIGHT).length} 亮色 + ${BASELINE.length} 基线条目`)
