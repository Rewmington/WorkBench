// 断点 × 主题截图仪：PLAN「UI 验证两条腿」里浏览器那一腿的工具。
//
// 为什么不用现成截图：§8.4 的断点与 Backup.vue 的 @media 都读**视口宽度**，
// 只能靠真视口复现，改容器宽度是假验收。
//
//   node tools/shots.mjs backup                     # 三档宽 × 暗亮各一张
//   node tools/shots.mjs todos --widths=1400 --modes=dark
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const ROOT = resolve(import.meta.dirname, '..')
const EDGE = join(process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Microsoft', 'Edge', 'Application', 'msedge.exe')
const PORT = 9337
const URL = 'http://localhost:1420/?mock=1'
const WIDTHS = [1400, 1000, 760]

const args = process.argv.slice(2)
const view = args.find((a) => !a.startsWith('--')) ?? '备份'
const opt = (name, dflt) => {
  const hit = args.find((a) => a.startsWith(`--${name}=`))
  return hit ? hit.slice(name.length + 3) : dflt
}
const slug = opt('as', view)
const extra = opt('then', '')
/**
 * --attach=9222 直连已开远程调试的 WebView2（原生 anvil.exe），用来验真后端：
 *   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npm run tauri:dev
 * 附身模式不改视口也不重新导航 —— 要看的就是那个真实窗口现在的样子。
 */
const attach = opt('attach', '')
/** --url 用来截 design/ 下的基准图，实现与参考稿并排比对，不靠记忆。 */
const target = opt('url', URL)
/** --raw：只导航 + 跑 --then + 截图，不去点应用内的侧栏。用来截 design/ 基准图。 */
const raw = args.includes('--raw')
const widths = (opt('widths', '') || '').split(',').filter(Boolean).map(Number)
const wlist = widths.length ? widths : WIDTHS
const mlist = (opt('modes', 'dark,light') || '').split(',').filter(Boolean)

const profile = join(tmpdir(), 'anvil-shots-profile')

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms))
}

let edge = null

async function launch() {
  if (!existsSync(EDGE)) throw new Error(`找不到 Edge：${EDGE}`)
  const p = spawn(EDGE, [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${profile}`,
    '--disable-gpu',
    '--no-first-run',
    '--no-default-browser-check',
    '--force-color-profile=srgb',
    'about:blank',
  ])
  edge = new Promise((ok) => p.on('exit', ok))
  p.on('error', (e) => {
    console.error(`edge 起不来：${e.message}`)
    process.exit(1)
  })
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/list`)
      const list = await r.json()
      const page = list.find((t) => t.type === 'page')
      if (page) return page.webSocketDebuggerUrl
    } catch {
      /* 端口还没监听 */
    }
    await sleep(250)
  }
  throw new Error('9337 上没等到 DevTools')
}

class Cdp {
  constructor(ws) {
    this.ws = ws
    this.n = 0
    this.pending = new Map()
    this.events = []
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data)
      if (msg.id != null && this.pending.has(msg.id)) {
        const { ok, bad } = this.pending.get(msg.id)
        this.pending.delete(msg.id)
        msg.error ? bad(new Error(msg.error.message)) : ok(msg.result)
      } else if (msg.method) {
        this.events.push(msg)
      }
    }
  }
  send(method, params = {}) {
    const id = ++this.n
    return new Promise((ok, bad) => {
      this.pending.set(id, { ok, bad })
      this.ws.send(JSON.stringify({ id, method, params }))
    })
  }
  async eval(expression, awaitPromise = false) {
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise, returnByValue: true })
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? '页面脚本报错')
    return r.result.value
  }
  /** 轮询到条件为真，替掉拍脑袋的固定 timeout：mock 是异步灌数据的 */
  async until(expression, timeout = 8000) {
    const t0 = Date.now()
    while (Date.now() - t0 < timeout) {
      if (await this.eval(`(function(){return (${expression})})()`)) return true
      await sleep(60)
    }
    throw new Error(`等不到 ${expression}`)
  }
}

/** 点侧栏进视图。窄档侧栏收成图标轨，所以两种都要认。 */
function clickNav(v) {
  return `(function(){
    const items = [...document.querySelectorAll('.y-nav button, .y-rail button')];
    const hit = items.find(b => (b.textContent||'').includes('${v}') || (b.title||'').includes('${v}'));
    if (!hit) return 'no-nav:' + items.map(b=>b.title||b.textContent.trim()).join('|');
    hit.click();
    return 'ok';
  })()`
}

async function attachUrl() {
  const r = await fetch(`http://127.0.0.1:${attach}/json/list`)
  const list = await r.json()
  const page = list.find((t) => t.type === 'page' && t.url.includes(':1420'))
  if (!page) throw new Error(`${attach} 上没有 localhost:1420 的页面（原生 anvil 起了吗？）`)
  return page.webSocketDebuggerUrl
}

async function main() {
  mkdirSync(join(ROOT, 'shots'), { recursive: true })
  const wsUrl = attach ? await attachUrl() : await launch()
  const ws = new WebSocket(wsUrl)
  await new Promise((ok, bad) => {
    ws.onopen = ok
    ws.onerror = () => bad(new Error('ws 连不上'))
    // WebView2 没开 --remote-allow-origins 时握手会静默挂住，不定时就会把整个工具吊死
    setTimeout(() => bad(new Error(attach ? `ws 握手超时：原生要带 --remote-allow-origins=*` : 'ws 握手超时')), 6000)
  })
  const cdp = new Cdp(ws)
  await cdp.send('Page.enable')
  await cdp.send('Runtime.enable')

  // 附身模式只拍真实窗口：不改视口、不重新导航、不覆盖主题，否则证据是摆出来的
  const plan = attach ? [[0, '']] : wlist.flatMap((w) => mlist.map((m) => [w, m]))
  const fails = []
  for (const [width, mode] of plan) {
    const file = attach ? `${opt('tag', 'm4')}-${slug}-native-${mode || 'real'}.png` : `${opt('tag', 'm4')}-${slug}-${width}-${mode}.png`
    try {
      if (!attach) {
        await cdp.send('Emulation.setDeviceMetricsOverride', {
          width,
          height: 900,
          deviceScaleFactor: 2,
          mobile: false,
        })
        await cdp.send('Page.navigate', { url: target })
      }
      await cdp.until(raw ? "document.body" : "document.querySelector('.y-win')")
      if (!raw) {
        const nav = await cdp.eval(clickNav(view))
        if (nav !== 'ok') throw new Error(nav)
        await cdp.until(`!!document.querySelector('h2')`, 5000)
      }
      // --then 用来演「点一下才出现」的状态：内联确认、未配凭据、选中详情
      if (extra) {
        // async IIFE 也支持：命令面板那条链路要「按键 → 等渲染 → 打字 → 回车」
        const r = await cdp.eval(extra, true)
        if (r === false) throw new Error('--then 表达式没命中任何元素')
        if (typeof r === 'string' && r.length) console.log(`  ↳ ${r}`)
      }
      if (mode) {
        // 走应用自己的切换路径（顶栏那个日/月按钮），不直接改 data-mode：
        // 直接改属性会把「切了不生效」这类 bug 截成一张好看的照片。
        const r = await cdp.eval(`(function(){
          const want = '${mode}'
          if (document.documentElement.dataset.mode === want) return 'already'
          const btn = [...document.querySelectorAll('.iconbtn')].find(b => (b.title || '').includes(want === 'light' ? '亮色' : '暗色'))
          if (!btn) return 'no-toggle'
          btn.click()
          return 'clicked'
        })()`)
        if (r === 'no-toggle') throw new Error('找不到主题切换按钮')
        await cdp.until(`document.documentElement.dataset.mode === '${mode}'`, 3000)
      }
      // 让 1s tick 与图片解码都落定，否则首屏数字是空的
      await sleep(400)
      const shot = await cdp.send('Page.captureScreenshot', { format: 'png' })
      writeFileSync(join(ROOT, 'shots', file), Buffer.from(shot.data, 'base64'))
      const dims = await cdp.eval(
        `(function(){const r=document.querySelector('.y-ctr').getBoundingClientRect();return JSON.stringify({nav:document.querySelector('.y-nav')?'text':'rail',content:Math.round(r.width),scroll:document.querySelector('.y-view')?document.querySelector('.y-view').scrollHeight:0})})()`,
      )
      console.log(`✓ shots/${file}  ${attach ? 'native' : `${width}px ${mode}`}  ${dims}`)
    } catch (e) {
      fails.push(`${width}/${mode}: ${e.message}`)
      console.log(`✗ ${attach ? 'native' : `${width}px ${mode}`}: ${e.message}`)
    }
  }

  // 附身的是原生实例：关掉它等于把用户的程序杀了，只断 ws
  if (!attach) {
    await cdp.send('Browser.close').catch(() => {})
    // Browser.close 返回时进程还在释放 profile 里的文件句柄，立刻 rm 会 EPERM
    await Promise.race([edge, sleep(4000)])
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
  }
  if (fails.length) {
    console.error(`\n${fails.length} 张没截成`)
    process.exit(1)
  }
  ws.close()
}

main()
