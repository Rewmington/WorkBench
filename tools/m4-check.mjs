// M4 的门：起本地 WebDAV 桩 → 跑 #[ignore] 的 webdav_e2e → 核对远端落成了什么。
// 用法：node tools/m4-check.mjs
// 真实上传（PUT 到 dav.jianguoyun.com）不在这儿，那是你在设置页填完应用密码后自己点的事。
import { spawn } from 'node:child_process'
import { readdir, rm, mkdir } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

const PORT = 8791
const USER = 'stub@example.com'
const PASS = 'stub-app-password'
const ROOT = resolve('tools/.webdav-root')
const COLLECTION = join(ROOT, 'dav', 'workbench-backup')
const BASE = `http://127.0.0.1:${PORT}/dav/workbench-backup/`

const run = (cmd, args, opts = {}) =>
  new Promise((done, fail) => {
    const p = spawn(cmd, args, { stdio: ['ignore', 'pipe', 'pipe'], shell: false, ...opts })
    let out = ''
    p.stdout.on('data', (b) => {
      out += b
      if (opts.echo) process.stdout.write(b)
    })
    p.stderr.on('data', (b) => {
      out += b
      if (opts.echo) process.stderr.write(b)
    })
    p.on('error', fail)
    p.on('exit', (code) => done({ code, out }))
  })

await rm(ROOT, { recursive: true, force: true })
await mkdir(COLLECTION, { recursive: true })

const stub = spawn(process.execPath, ['tools/webdav-stub.mjs', '--port', String(PORT), '--root', ROOT, '--user', USER, '--pass', PASS], {
  stdio: ['ignore', 'pipe', 'inherit'],
})
let markReady
const ready = new Promise((r) => {
  markReady = r
})
stub.stdout.on('data', (b) => {
  const text = String(b)
  if (text.includes('ready')) {
    console.log(text.trim())
    markReady(true)
  }
})
// 桩没起来就把测试跑完是浪费时间，直接超时
const readyInTime = await Promise.race([ready, new Promise((r) => setTimeout(() => r(false), 5000))])

try {
  if (!readyInTime) throw new Error('桩 5 秒没起来')
  const { code, out } = await run('cargo', ['test', '--test', 'webdav_e2e', '--', '--ignored', '--test-threads=1'], {
    cwd: resolve('src-tauri'),
    env: { ...process.env, ANVIL_WEBDAV_BASE: BASE, ANVIL_WDAV_USER: USER, ANVIL_WDAV_PASS: PASS, ANVIL_STUB_DIR: COLLECTION },
    echo: true,
  })

  const names = existsSync(COLLECTION) ? await readdir(COLLECTION) : []
  const pattern = /^anvil-\d{8}T\d{6}Z-[0-9a-f]{8}\.abackup$/
  const uploads = names.filter((n) => pattern.test(n))
  const device = names.filter((n) => n.startsWith('device-'))
  const strays = names.filter((n) => !pattern.test(n) && !n.startsWith('device-'))

  console.log('\n远端落成：')
  const mark = (n) => (pattern.test(n) ? 'ok  ' : n.startsWith('device-') ? '药箱 ' : 'BAD ')
  for (const n of names) console.log(`  ${mark(n)} ${n}`)

  const problems = []
  if (uploads.length < 1) problems.push('一个 anvil-*.abackup 都没上传')
  if (strays.length) problems.push(`出现了不该有的文件：${strays.join(', ')}`)
  if (device.length !== 1) problems.push(`device-* 数量应为 1（桩预置的那台药箱），实际 ${device.length}`)
  if (code !== 0) problems.push('cargo test 失败')
  if (problems.length) {
    console.log('\nM4 门未通过：')
    for (const p of problems) console.log(`  - ${p}`)
    process.exitCode = 1
  } else {
    console.log('\nM4 门通过：信封往返 + 目录边界 + 还原一致性 + 远端只多出 anvil-* 文件')
    out.split('\n').filter((l) => l.startsWith('test result')).forEach((l) => console.log(`  ${l.trim()}`))
  }
} catch (e) {
  console.error(`m4-check 自己出错了：${e.message}`)
  process.exitCode = 1
} finally {
  stub.kill()
}
