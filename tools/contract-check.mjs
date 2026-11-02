#!/usr/bin/env node
/**
 * invoke 契约检查：前端 `src/api/real.ts` 里每个 `invoke('name', { ... })` 的参数名，
 * 必须能在 Rust 那一侧 `src-tauri/src/commands/**` 对应命令的签名里找到。
 *
 * 为什么需要它（issue #1 复核时翻出的 N1）：`open_url` 前端发的是 `{ url }`，
 * 命令签名是 `target` —— Tauri 按参数名反序列化，缺字段当场失败，而调用点写的是
 * `void api.openUrl(...)`，于是「用系统浏览器打开」从 M3 起一次都没生效过，
 * 而 `tsc`、`vitest`（跑 mock）、`cargo test`（不碰前端）三层检查谁都抓不到它。
 * 参数名是跨语言的字符串协议，编译期看不见，只能这样对一次。
 *
 * 用法：node tools/contract-check.mjs      （CI 里跑，不匹配就非零退出）
 */
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname.replace(/^\/(.:)/, '$1')
/**
 * 相对仓库根的路径。**不能**靠切仓库目录名来算：本地克隆叫 `workbench`（小写），
 * GitHub runner 上叫 `WorkBench`，切不出来就是 undefined，而下面 `.filter((s) => !s.file.endsWith(...))`
 * 会当场 TypeError —— v0.1.7 第一次推 tag 就是这么在 CI 上挂掉、Release 没建出来的。
 * 现在按 root 前缀切，切不掉就用原路径，任何机器上都不会返回 undefined。
 */
const relBase = root.replace(/\\/g, '/').replace(/\/+$/, '')
const rel = (f) => {
  const p = f.replace(/\\/g, '/')
  return p.startsWith(`${relBase}/`) ? p.slice(relBase.length + 1) : p
}
const CMD_DIR = join(root, 'src-tauri', 'src', 'commands')
const REAL_TS = join(root, 'src', 'api', 'real.ts')

const snake = (s) => s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase()

// ---------- 收 Rust 侧：命令名 → 参数名集合 ----------
const commands = new Map()
const walk = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  )

/**
 * 按顶层逗号切参数。Rust 签名里到处是 `State<'_, AppState>`、`Option<String>`，
 * 直接 split(',') 会把泛型里的逗号当成分隔符 —— 第一版就这么把
 * `AppState>` 当成了一个参数名，报了 78 条假错。
 */
function splitTop(s) {
  const out = []
  let depth = 0
  let cur = ''
  for (const ch of s) {
    if ('<([{'.includes(ch)) depth += 1
    else if ('>)]}'.includes(ch)) depth -= 1
    if (ch === ',' && depth === 0) {
      out.push(cur)
      cur = ''
    } else {
      cur += ch
    }
  }
  if (cur.trim()) out.push(cur)
  return out.map((x) => x.trim()).filter(Boolean)
}

/** Tauri 注入的参数按**类型**认，不按名字 —— 名字是作者起的，类型才是框架定的。 */
const INJECTED_TYPE = /^(?:mut\s+)?(?:State|AppHandle|Window|WebviewWindow|App|tauri::)/

for (const file of walk(CMD_DIR).filter((f) => f.endsWith('.rs'))) {
  const src = readFileSync(file, 'utf8')
  // 只认真正注册过的命令：`#[tauri::command]` 紧跟的 `pub fn` / `pub async fn`
  for (const m of src.matchAll(/#\[tauri::command\][^\n]*\n\s*pub (?:async )?fn (\w+)\(([\s\S]*?)\)\s*(?:->|where|\{)/g)) {
    const [, name, params] = m
    const argNames = new Set()
    const optional = new Set()
    for (const p of splitTop(params)) {
      const [head = '', ...rest] = p.split(':')
      const type = rest.join(':').trim()
      const arg = head.trim().replace(/^mut\s+/, '')
      if (!arg || arg === 'self') continue
      if (INJECTED_TYPE.test(arg) || INJECTED_TYPE.test(type)) continue
      argNames.add(snake(arg))
      // Option<T> 可以不传（Tauri 给 None），不该要求前端一定给键
      if (/^Option</.test(type)) optional.add(snake(arg))
    }
    commands.set(name, { argNames, optional })
  }
}

// ---------- 收前端侧：invoke('name', { k: v }) 的键 ----------
const real = readFileSync(REAL_TS, 'utf8')
const problems = []
let checked = 0

for (const m of real.matchAll(/invoke(?:<[^>]*>)?\(\s*'(\w+)'(\s*,\s*\{)?/g)) {
  const name = m[1]
  let keys = []
  if (m[2]) {
    // 从 `{` 之后开始手工配对括号：参数对象里还会嵌对象（`{ draft: { label, mode } }`），
    // 用 `[^}]*` 会在第一个内层 `}` 就截断，把嵌套的键当成顶层键 —— 第一版就误报了这一条。
    let i = m.index + m[0].length
    let depth = 1
    let body = ''
    while (i < real.length && depth > 0) {
      const ch = real[i]
      if ('<([{'.includes(ch)) depth += 1
      else if ('>)]}'.includes(ch)) {
        depth -= 1
        if (depth === 0) break
      }
      body += ch
      i += 1
    }
    keys = splitTop(body)
      .map((s) => s.split(':')[0].trim().replace(/^['"]|['"]$/g, ''))
      .filter(Boolean)
      .map(snake)
  }
  checked += 1

  const sig = commands.get(name)
  if (!sig) {
    problems.push(`real.ts 调了 '${name}'，但 commands/ 里没有这个 #[tauri::command]`)
    continue
  }
  for (const k of keys) {
    if (!sig.argNames.has(k)) {
      problems.push(
        `'${name}'：前端传了 { ${k} }，Rust 签名里没有这个参数（实有：${[...sig.argNames].join(', ') || '无'}）`,
      )
    }
  }
  for (const p of sig.argNames) {
    if (!keys.includes(p) && !sig.optional.has(p)) {
      problems.push(`'${name}'：Rust 要参数 ${p}，前端没传 —— Tauri 会当场报缺字段`)
    }
  }
}

console.log(`invoke 契约一致：${checked} 个调用点对上了 ${commands.size} 条命令`)

// ---------- 第二层：事件名（Rust `emit` ↔ 前端 `listen`）----------
// 和 invoke 参数名一样是跨语言的字符串协议，而且比参数名更隐蔽：参数名对不上会当场报错，
// 事件名对不上只是**安静地什么都不发生**。`menu://click` 就是这么空转了几个版本 ——
// 托盘「设置」不切页、右键「复制选中内容」没反应，三层检查一层都不响。
const RUST_SRC = walk(join(root, 'src-tauri', 'src'))
const TS_SRC = [join(root, 'src')].flatMap(walk).filter((f) => /\.(ts|vue)$/.test(f))

const consts = new Map()
const emitted = new Map() // 事件名 → 出处
for (const file of RUST_SRC) {
  if (!file.endsWith('.rs')) continue
  const text = readFileSync(file, 'utf8')
  for (const [, name, value] of text.matchAll(/pub const (\w+): &str = "([^"]+)"/g)) consts.set(name, value)
  for (const [, quoted, ident] of text.matchAll(/\.emit\(\s*(?:"([^"]+)"|([A-Za-z_0-9 :]+))/g)) {
    const raw = (quoted ?? ident).trim().split('::').pop()
    const value = consts.get(raw)
    // 只关心形如 `a://b` 的事件名；别的常量当 payload 传时会走到这里，跳过。
    if (value && value.includes('://')) emitted.set(value, rel(file))
  }
}

const listened = new Map() // 事件名 → 出处
for (const file of TS_SRC) {
  const text = readFileSync(file, 'utf8')
  for (const [, name] of text.matchAll(/listen(?:<[^>]*>)?\(\s*'([^']+)'/g)) {
    listened.set(name, rel(file))
  }
}

for (const [name, where] of emitted) {
  if (!listened.has(name)) problems.push(`事件 '${name}' 由 ${where} 发出，但前端没有任何 listen —— 它不会报错，只是永远不发生`)
}
for (const [name, where] of listened) {
  if (!emitted.has(name)) problems.push(`前端 ${where} 在 listen '${name}'，Rust 那边没有一处 emit —— 这条回调永远不触发`)
}
console.log(
  `事件名契约一致：${emitted.size} 个 emit 全部有 listen，${listened.size} 个 listen 全部有 emit`,
)

// ---------- 第三层：每条命令都要走到一个界面上的入口 ----------
// 前两层查的是「两边对不上」，这一层查的是「一边根本没人用」：命令注册了、real.ts 包好了、
// composable 也写了，但模板里没有一个按钮按得到它 —— 对用户来说这条能力不存在。
// 这一类在本仓库出得最多（分区能建不能删、文件夹改名/删除/挪动整片没入口、
// 清理对端文件只有命令没有门、便签槽位删不掉），而失败模式是**静默的**：不报错、不弹窗，
// 只是那个功能谁也找不到。静态就能查，所以钉在这里。
//
// 「走到界面」要穿两层：命令 → composable 的某个函数 → 模板里调那个函数。
// 只看第一层会放过文件夹那一片（api.folderRename 确实被 useNotes 调了，缺的是按钮）。
const NO_UI_OK = {
  // 命令名 → 为什么允许界面上调不到。目前一条都没有：加之前先想想是不是真的只能由 Rust 侧触发。
}

const keyByCommand = new Map()
for (const m of real.matchAll(/^ {2}(\w+):\s*(?:async\s*)?\(?[^=]*?\)?\s*=>\s*invoke(?:<[^>]*>)?\(\s*'(\w+)'/gm)) {
  keyByCommand.set(m[2], m[1])
}

const apiLayer = new Set(['src/api/real.ts', 'src/api/mock.ts', 'src/api/types.ts', 'src/api/index.ts'])
const sources = TS_SRC.map((f) => ({ file: rel(f), text: readFileSync(f, 'utf8') })).filter(
  (s) => !apiLayer.has(s.file) && !s.file.endsWith('.spec.ts'),
)
const views = sources.filter((s) => /(\/|\\)(views|components)(\/|\\)/.test(s.file) || s.file === 'src/App.vue')
const middle = sources.filter((s) => !views.includes(s))

/**
 * 拆成「函数名 → 函数体」+「函数体外的那些行」。
 * 缩进 0~4 都认：composable 里的函数两格，.vue 的 `<script setup>` 顶格。
 * 只按缩进配对花括号收尾，不引 AST 解析器 —— 这里要的是「谁可能调到谁」的近似，
 * 近似只让检查变松（同名函数会合流），不会凭空报错。
 */
function partsOf(text) {
  const fns = []
  const rest = []
  let cur = null
  let stop = null
  for (const line of text.split(/\r?\n/)) {
    if (cur && stop !== null && line === `${stop}}`) {
      cur = null
      stop = null
    }
    const d = line.match(
      /^( {0,4})(?:export )?(?:async )?function (\w+)|^( {0,4})(?:const|let) (\w+) = (?:async )?(?:\([^)]*\)|[\w$]+) =>/,
    )
    if (d) {
      stop = d[1] ?? d[3]
      cur = { name: d[2] ?? d[4], body: `${line}\n` }
      fns.push(cur)
      // 一行就写完的声明（`const onVis = () => sample()`）没有配对的 `}`，
      // 不收尾就会把后面整段都当成它的身体 —— 于是 onMounted 里的调用点被藏进没人走的节点。
      if (!line.trimEnd().endsWith('{')) {
        cur = null
        stop = null
      }
      continue
    }
    if (cur) cur.body += line + '\n'
    else rest.push(line)
  }
  return { fns, rest: rest.join('\n') }
}

const NODES = [] // { key, file, name|null, body, tpl? }
const LOCAL = new Map() // `file#name` → 同一个文件里的这个函数
const EXPOSED = new Map() // 名字 → 把这个名字对外暴露出来的文件（跨文件只认这条路）
const VIEWISH = (file) => /(\/|\\)(views|components)(\/|\\)/.test(file) || file === 'src/App.vue' || file === 'src/main.ts'

/**
 * 一个文件对外暴露哪些名字：`export function/const`，以及 composable 末尾那份 `return { ... }` 清单。
 * 视图里写 `notes.renameFolder(...)` 时，`renameFolder` 并不在视图里定义 ——
 * 它靠 useNotes 的返回清单挂回去。按名字全局合流是不行的：
 * 笔记页和详情面板各有一个都叫 `commitTags` 的函数，合流之后拆掉其中一个的按钮，
 * 检查照样「看得见另一个」，于是那道按钮没了也不报（M2 变异实测）。
 */
function exposedNames(text) {
  const out = new Set()
  for (const m of text.matchAll(/^export (?:async )?(?:function|const|let|var) ([\w$]+)/gm)) out.add(m[1])
  // composable 末尾那份返回清单，两种写法都要认：
  // `return { status, running, load, runNow }`（一行，useSync/useStats/useDashboard 是这样）
  // 和一行一个字段的多行版（useNotes 是这样）。只认多行版就会把整份清单当成没暴露，
  // 于是「同步」这一类明明界面上有按钮，检查却说走不到。
  for (const m of text.matchAll(/^ {0,4}return \{([^}]*)\}/gm)) {
    for (const piece of m[1].split(/[,\n]/)) {
      const n = piece.match(/([\w$]+)/)
      if (n) out.add(n[1])
    }
  }
  return out
}

for (const s of sources) {
  const { fns, rest } = partsOf(s.text)
  const tpl = (s.text.match(/<template>([\s\S]*)<\/template>/) ?? [, ''])[1]
  NODES.push({ key: `${s.file}#@top`, file: s.file, name: null, body: rest, tpl })
  for (const f of fns) {
    const key = `${s.file}#${f.name}`
    NODES.push({ key, file: s.file, name: f.name, body: f.body })
    LOCAL.set(key, key)
  }
  for (const n of exposedNames(s.text)) {
    if (!EXPOSED.has(n)) EXPOSED.set(n, [])
    EXPOSED.get(n).push(s.file)
  }
}

/**
 * 每个节点「可能提到」的标识符。这里要的是标识符级别的粗边，不是 `foo(` 那种精确调用：
 * `setInterval(tick, 5000)`、`addEventListener('focus', tick)` 传的是函数本身，没有括号，
 * 只按 `name(` 连边就会把「系统读数每 5 秒采一次」判成没人调用。
 */
for (const n of NODES) {
  n.calls = new Set([
    ...[...n.body.matchAll(/\b[\w$]+\b/g)].map((m) => m[0]),
    ...[...(n.tpl ?? '').matchAll(/\b[\w$]+\b/g)].map((m) => m[0]),
  ])
}

/**
 * 从「界面真的会执行到的地方」出发做闭包：每个视图/组件的顶层代码（`<script setup>` 一挂载就跑，
 * `onMounted` 也在里面）与模板里出现过的标识符；`src/main.ts` 是程序入口，也算。
 */
const reached = new Set()
const queue = NODES.filter((n) => n.name === null && VIEWISH(n.file))
for (const n of queue) reached.add(n.key)
for (let i = 0; i < queue.length; i++) {
  const n = queue[i]
  for (const name of n.calls) {
    const own = `${n.file}#${name}`
    const targets = LOCAL.has(own) ? [own] : []
    for (const f of EXPOSED.get(name) ?? []) {
      const k = `${f}#${name}`
      if (f !== n.file && LOCAL.has(k)) targets.push(k)
    }
    for (const key of targets) {
      if (reached.has(key)) continue
      reached.add(key)
      queue.push(NODES.find((x) => x.key === key))
    }
  }
}

const reachedApiKeys = new Set()
for (const n of NODES) {
  if (!reached.has(n.key)) continue
  for (const m of `${n.body}\n${n.tpl ?? ''}`.matchAll(/\bapi\.([\w$]+)\(/g)) reachedApiKeys.add(m[1])
}

let reachedCount = 0
for (const name of commands.keys()) {
  if (name in NO_UI_OK) continue
  const key = keyByCommand.get(name)
  if (!key) {
    problems.push(`命令 '${name}' 在 src/api/real.ts 里没有对应封装 —— 前端根本调不到它`)
    continue
  }
  if (reachedApiKeys.has(key)) {
    reachedCount += 1
    continue
  }
  // 「包好了但没人调」与「调了但那行代码在界面上按不到」分开说，第二种更容易看半天
  const holder = NODES.find((n) => n.body.includes(`api.${key}(`))
  problems.push(
    holder
      ? `命令 '${name}'（api.${key}）的调用点在 ${holder.file}${holder.name ? ` 的 ${holder.name}()` : ''} 里，但从界面上的任何入口都走不到它 —— 这个功能等于不存在`
      : `命令 '${name}'（api.${key}）注册了、real.ts 也包好了，但 src/ 里没有任何地方调 api.${key} —— 这个功能等于不存在`,
  )
}

if (problems.length) {
  console.error(`契约不匹配（${problems.length} 处）：`)
  for (const p of problems) console.error(`  - ${p}`)
  process.exit(1)
}
console.log(
  `命令入口检查通过：${reachedCount}/${commands.size} 条命令都能从界面上走到（豁免 ${Object.keys(NO_UI_OK).length} 条）`,
)
