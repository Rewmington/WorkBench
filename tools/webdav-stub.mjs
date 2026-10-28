// M4 验收用的本地 WebDAV 桩。它不是坚果云的克隆，只复刻工作台真正依赖的五个动词
// 与三种「讨厌之处」：Basic 鉴权、PROPFIND 的命名空间前缀不稳定、href 百分号编码。
// 由 tools/m4-check.mjs 拉起，不要手工长期开着。
import { createServer } from 'node:http'
import { mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { dirname, join, posix } from 'node:path'

const argv = process.argv.slice(2)
const flag = (name, dflt) => {
  const i = argv.indexOf(`--${name}`)
  return i === -1 ? dflt : argv[i + 1]
}

const PORT = Number(flag('port', 8787))
const ROOT = flag('root', join('tools', '.webdav-root'))
const USER = flag('user', 'stub@example.com')
const PASS = flag('pass', 'stub-app-password')
const COLLECTION = '/dav/workbench-backup/'

const pathFor = (href) => join(ROOT, ...href.replace(/^\//, '').split('/').filter(Boolean))

function authed(req) {
  const head = req.headers.authorization ?? ''
  if (!head.startsWith('Basic ')) return false
  const [u, ...rest] = Buffer.from(head.slice(6), 'base64').toString('utf8').split(':')
  return u === USER && rest.join(':') === PASS
}

const send = (res, code, body = '', type = 'text/plain; charset=utf-8', extra = {}) => {
  const buf = Buffer.isBuffer(body) ? body : Buffer.from(body, 'utf8')
  res.writeHead(code, { 'Content-Type': type, 'Content-Length': buf.length, ...extra })
  res.end(buf)
}

// 前缀在 D: 与 s0: 之间来回换：写死前缀的解析器在这里一定会露馅
const row = (href, { isDir, size, n = 0 }) => {
  const p = n % 2 === 0 ? 'D' : 's0'
  const type = isDir ? `<${p}:resourcetype><${p}:collection/></${p}:resourcetype>` : `<${p}:resourcetype/>`
  const len = isDir ? '' : `<${p}:getcontentlength>${size}</${p}:getcontentlength>`
  return `<${p}:response><${p}:href>${href}</${p}:href><${p}:propstat><${p}:prop>${type}${len}</${p}:prop><${p}:status>HTTP/1.1 200 OK</${p}:status></${p}:propstat></${p}:response>`
}

const readBody = async (req) => {
  const chunks = []
  for await (const c of req) chunks.push(c)
  return Buffer.concat(chunks)
}

const server = createServer(async (req, res) => {
  const href = decodeURIComponent(req.url.split('?')[0])
  if (!authed(req)) {
    return send(res, 401, '需要 Basic 鉴权', 'text/plain; charset=utf-8', { 'WWW-Authenticate': 'Basic realm="stub"' })
  }
  if (!href.startsWith(COLLECTION)) return send(res, 404, '这个桩只服务 workbench-backup')

  const target = pathFor(href)
  try {
    if (req.method === 'MKCOL') {
      if (existsSync(target)) return send(res, 405, '已存在')
      await mkdir(target, { recursive: true })
      return send(res, 201, '')
    }

    if (req.method === 'PROPFIND') {
      if (!existsSync(target)) return send(res, 404, '目录还没建')
      const rows = [row(href, { isDir: true, n: 0 })]
      const names = (await readdir(target)).sort()
      for (const [i, name] of names.entries()) {
        const abs = join(target, name)
        if ((await stat(abs)).isDirectory()) continue
        const { size } = await stat(abs)
        rows.push(row(posix.join(href, encodeURIComponent(name)), { isDir: false, size, n: i + 1 }))
      }
      const xml = `<?xml version="1.0" encoding="utf-8"?><D:multistatus xmlns:D="DAV:" xmlns:s0="DAV:">${rows.join('')}</D:multistatus>`
      return send(res, 207, xml, 'application/xml; charset=utf-8')
    }

    if (req.method === 'PUT') {
      const body = await readBody(req)
      await mkdir(dirname(target), { recursive: true })
      await writeFile(target, body)
      return send(res, 201, '')
    }

    if (req.method === 'GET') {
      if (!existsSync(target)) return send(res, 404, '不在')
      return send(res, 200, await readFile(target), 'application/octet-stream')
    }

    if (req.method === 'DELETE') {
      if (!existsSync(target)) return send(res, 404, '本来就不在')
      await rm(target, { force: true })
      return send(res, 204, '')
    }

    return send(res, 405, '不支持的动词')
  } catch (e) {
    return send(res, 500, `桩出错了：${e.message}`)
  }
})

server.listen(PORT, '127.0.0.1', async () => {
  await mkdir(pathFor(COLLECTION), { recursive: true })
  // 药箱的文件混在同一个账号里：客户端必须看不见它们，哪怕远端真有
  await writeFile(join(pathFor(COLLECTION), 'device-pc1.json'), '{"medicines":[]}')
  console.log(`webdav-stub ready port=${PORT} base=http://127.0.0.1:${PORT}${COLLECTION}`)
})
