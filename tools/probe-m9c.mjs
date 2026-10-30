// 真后端体检：只读**结构计数**，不读任何标题 / 课程名 / 教室。
// 用户的真库里有真待办，验收要的是"这一屏渲染出来了没有"，不是把内容搬进对话。
//
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222 --remote-allow-origins=*" \
//     src-tauri/target/release/anvil.exe &
//   node tools/probe-m9c.mjs
import { openCdp, pollPage, sleep } from './cdp.mjs'

// 发布版加载的是打包资源（tauri.localhost），dev 版才是 :1420 —— 两个都要能认
const ws = await pollPage(9222, { urlIncludes: 'tauri.localhost', tries: 8 }).catch(() =>
  pollPage(9222, { urlIncludes: ':1420', tries: 8 }),
)
const cdp = await openCdp(ws, 'ws 握手超时：要带 --remote-allow-origins=*')

const evalJs = async (expr) => {
  try {
    return await cdp.eval(expr, true)
  } catch (e) {
    throw new Error(`${e.message} ← ${expr.slice(0, 80)}`)
  }
}
const nav = (label) =>
  evalJs(`(async()=>{
    const b=[...document.querySelectorAll('.y-nav button,.y-rail button')].find(x=>(x.textContent||'').includes('${label}'));
    if(!b) return 'no-nav';
    b.click(); await new Promise(r=>setTimeout(r,900)); return 'ok';
  })()`)

const out = {}
out.url = await evalJs('location.href')
out.win = await evalJs(`!!document.querySelector('.y-win')`)
out.navCount = await evalJs(`document.querySelectorAll('.y-nav button,.y-rail button').length`)
out.hasTimetableEntry = await evalJs(
  `[...document.querySelectorAll('.y-nav button,.y-rail button')].some(b=>(b.textContent||b.title||'').includes('课表'))`,
)

await nav('课表')
out.tt = await evalJs(`JSON.stringify({
  view: !!document.querySelector('.y-view'),
  cells: document.querySelectorAll('.cls').length,
  days: document.querySelectorAll('table thead th').length,
  periodRows: document.querySelectorAll('th.hour').length,
  needWeek1: !!document.querySelector('.need'),
  legend: (document.querySelector('.legend')||{}).textContent ?? null,
})`)

await nav('待办')
out.todos = await evalJs(`JSON.stringify({
  bands: [...document.querySelectorAll('.bandh')].map(b=>b.textContent.replace(/\\s+/g,' ').trim()),
  gap: (document.querySelector('.gap')||{}).textContent ?? null,
  grips: document.querySelectorAll('.grip').length,
  greenLines: document.querySelectorAll('.list .over').length,
  overdueOnDone: [...document.querySelectorAll('.row.done .over-txt')].length,
})`)

console.log(JSON.stringify(out, null, 2))
cdp.close()
await sleep(200)
process.exit(0)
