// §8.6 反网页感清单里「可以静态检查」的那几条 + §8.8 的实现禁令。
// 原则：外观不受设计令牌控制的元素，在主题化应用里就是债。
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'

const ROOT = 'src'

function walk(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) out.push(...walk(p))
    else if (/\.(css|vue|ts)$/.test(name)) out.push(p)
  }
  return out
}

const RULES = [
  {
    file: /\.vue$/,
    re: /\bDate\.now\(\)/,
    why: '组件里不许自己取时间：一切相对时间走 useClock 的 now（1 秒 tick 是唯一时间源，否则「改系统时间」那条验收不可能成立）',
  },
  { re: /backdrop-filter/, why: '§8.8.2：常驻模糊把 GPU 吃到 26%，观感用预烘焙渐变拿' },
  { re: /filter:\s*blur/, why: '§8.8.2 同上；动效面上的 filter 正是掉帧来源' },
  { re: /animation-play-state/, why: '§8.8.1：CSS 暂停不算真停，rAF 必须 cancelAnimationFrame' },
  { re: /-webkit-app-region/, why: '§8.6：那是 Electron 的属性，wry 不认 —— 用 data-tauri-drag-region' },
  { re: /font-size:\s*16px/, why: '§8.6：16px 是浏览器默认字号，出现就说明基线没落到 13px（层级该靠字重与色彩档位，不是靠放大字号）' },
  { re: /<select[\s>]/, why: '原生 <select> 不吃主题令牌，用 Segmented 或自绘弹层' },
  { re: /scroll-behavior:\s*smooth/, why: '§8.6：网页式平滑滚动，原生软件没有' },
]

// 允许直接取时间的白名单
const CLOCK_ALLOW = [join('src', 'composables', 'useClock.ts')]

const hits = []
for (const file of walk(ROOT)) {
  if (CLOCK_ALLOW.includes(file)) continue
  const text = readFileSync(file, 'utf8')
  const lines = text.split(/\r?\n/)
  for (const rule of RULES) {
    if (rule.file && !rule.file.test(file.replace(/\\/g, '/'))) continue
    lines.forEach((line, i) => {
      const t = line.trim()
      // 注释里写「不要用 -webkit-app-region」正是这段代码的价值所在，不该被自己拦下
      if (t.startsWith('//') || t.startsWith('/*') || t.startsWith('*')) return
      if (rule.re.test(line)) {
        hits.push(`${file}:${i + 1}  ${rule.re}  ${rule.why}\n            ${line.trim().slice(0, 90)}`)
      }
    })
  }
}

if (hits.length) {
  console.error(`样式/实现纪律违反 ${hits.length} 处：\n\n` + hits.join('\n\n'))
  process.exit(1)
}

console.log(`样式纪律检查通过（${walk(ROOT).length} 个文件，${RULES.length} 条规则）`)
