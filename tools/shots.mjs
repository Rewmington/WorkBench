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

import { openCdp, pollPage, sleep } from './cdp.mjs'

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
 * --preset=moss|ink|graphite 先经设置页那个真开关换预设，再回到要拍的视图。
 * 买的是「三轴组合都要能验」：直接写 data-preset 会把"切了不生效"拍成一张好看的照片。
 */
const preset = opt('preset', '')
const PRESET_LABEL = { graphite: '石墨', moss: '苔绿', ink: '墨黑' }
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
  return pollPage(PORT, { tries: 60 })
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

async function main() {
  if (preset && !PRESET_LABEL[preset]) {
    console.error(`--preset 只认 ${Object.keys(PRESET_LABEL).join(' / ')}`)
    process.exit(1)
  }
  mkdirSync(join(ROOT, 'shots'), { recursive: true })
  const wsUrl = attach
    ? await pollPage(Number(attach), { urlIncludes: ':1420', tries: 8 })
    : await launch()
  const cdp = await openCdp(
    wsUrl,
    attach ? 'ws 握手超时：原生要带 --remote-allow-origins=*' : 'ws 握手超时',
  )

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
        // 每个视图的根都是 section.y-view；等 h2 会漏掉「待办」这类没有 h2 的视图
        await cdp.until(`!!document.querySelector('.y-view')`, 5000)
      }
      // 预设轴也走真开关：先经设置页点一下，再回到要拍的视图
      if (!raw && preset) {
        await cdp.eval(clickNav('设置'))
        await cdp.until(`!!document.querySelector('.y-view .s')`, 5000)
        const picked = await cdp.eval(`(function(){
          const all = [...document.querySelectorAll('.y-view .s')];
          const hit = all.find(b => (b.textContent || '').trim().startsWith('${PRESET_LABEL[preset]}'));
          if (!hit) return 'no-preset:' + all.map(b => b.textContent.trim()).join('|');
          hit.click();
          return 'ok';
        })()`)
        if (picked !== 'ok') throw new Error(picked)
        await cdp.until(`document.documentElement.dataset.preset === '${preset}'`, 3000)
        await cdp.eval(clickNav(view))
        await cdp.until(`!!document.querySelector('.y-view')`, 5000)
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
  cdp.close()
}

main()
