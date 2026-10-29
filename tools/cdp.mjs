// 极简 CDP 客户端，`shots.mjs`（截图）与 `verify.mjs`（逐图核对）共用一份。
//
// 为什么要共用：连 WebView2 有三个不写在文档里的坑 —— 握手会静默挂住（要
// `--remote-allow-origins=*`）、`Runtime.evaluate` 不 `awaitPromise` 就拿不到
// async 结果、轮询条件比固定 sleep 可靠。两处各抄一遍，迟早只修一边。

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

export class Cdp {
  constructor(ws) {
    this.ws = ws
    this.n = 0
    this.pending = new Map()
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data)
      if (msg.id != null && this.pending.has(msg.id)) {
        const { ok, bad } = this.pending.get(msg.id)
        this.pending.delete(msg.id)
        msg.error ? bad(new Error(msg.error.message)) : ok(msg.result)
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

  /** 轮询到条件为真。固定 sleep 在异步灌数据的页面上要么太短拍到空图，要么白等。 */
  async until(expression, timeout = 8000) {
    const t0 = Date.now()
    while (Date.now() - t0 < timeout) {
      if (await this.eval(`(function(){return (${expression})})()`)) return true
      await sleep(60)
    }
    throw new Error(`等不到 ${expression}`)
  }

  close() {
    this.ws.close()
  }
}

/** 等某个调试端口上出现一个 page 目标，返回它的 webSocketDebuggerUrl。 */
export async function pollPage(port, { urlIncludes = '', tries = 60, interval = 250 } = {}) {
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/list`)
      const list = await r.json()
      const page = list.find(
        (t) => t.type === 'page' && (!urlIncludes || t.url.includes(urlIncludes)),
      )
      if (page) return page.webSocketDebuggerUrl
    } catch {
      /* 端口还没监听 */
    }
    await sleep(interval)
  }
  throw new Error(`${port} 上没等到 DevTools${urlIncludes ? `（要找含 ${urlIncludes} 的页面）` : ''}`)
}

/**
 * 连上去并开好 Page / Runtime 域。
 * `handshakeHint` 用来把"静默挂住"这种失败说成人话 —— 原生实例必须带
 * `--remote-allow-origins=*`，否则 WS 握手既不报错也不返回。
 */
export async function openCdp(wsUrl, handshakeHint = 'ws 握手超时') {
  const ws = new WebSocket(wsUrl)
  await new Promise((ok, bad) => {
    ws.onopen = ok
    ws.onerror = () => bad(new Error('ws 连不上'))
    setTimeout(() => bad(new Error(handshakeHint)), 6000)
  })
  const cdp = new Cdp(ws)
  await cdp.send('Page.enable')
  await cdp.send('Runtime.enable')
  return cdp
}
