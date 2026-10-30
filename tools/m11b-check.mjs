// M11b 的门：起本地 WebDAV 桩（两个集合）→ 跑 #[ignore] 的 sync_relay_e2e → 核对同步目录落成了什么。
// 用法：node tools/m11b-check.mjs
// 真实上传（PUT 到 dav.jianguoyun.com/dav/workbench-sync/）不在这儿，那是设置页点「立即同步」的事。
import { spawn } from 'node:child_process'
import { readdir, rm, mkdir } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

const PORT = 8793
const USER = 'stub@example.com'
const PASS = 'stub-app-password'
const ROOT = resolve('tools/.webdav-sync-root')
const BACKUP_COLLECTION = join(ROOT, 'dav', 'workbench-backup')
const SYNC_COLLECTION = join(ROOT, 'dav', 'workbench-sync')
// 与 m4-check 同一个环境变量：ANVIL_WEBDAV_BASE 指备份目录，同步 base 由客户端自己折成兄弟目录
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
await mkdir(BACKUP_COLLECTION, { recursive: true })
await mkdir(SYNC_COLLECTION, { recursive: true })

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
  const { code, out } = await run('cargo', ['test', '--test', 'sync_relay_e2e', '--', '--ignored', '--test-threads=1'], {
    cwd: resolve('src-tauri'),
    env: {
      ...process.env,
      ANVIL_WEBDAV_BASE: BASE,
      ANVIL_WDAV_USER: USER,
      ANVIL_WDAV_PASS: PASS,
      ANVIL_SYNC_DIR: SYNC_COLLECTION,
    },
    echo: true,
  })

  const names = existsSync(SYNC_COLLECTION) ? await readdir(SYNC_COLLECTION) : []
  const encPattern = /^device-.+\.enc$/
  const uploads = names.filter((n) => encPattern.test(n))
  const strays = names.filter((n) => !encPattern.test(n) && n !== 'stray-note.txt')

  console.log('\n同步目录落成：')
  const mark = (n) => (encPattern.test(n) ? 'ok   ' : n === 'stray-note.txt' ? '预放 ' : 'BAD  ')
  for (const n of names) console.log(`  ${mark(n)} ${n}`)

  const problems = []
  if (uploads.length < 1) problems.push('一个 device-*.enc 都没同步上去')
  if (strays.length) problems.push(`出现了不该有的文件：${strays.join(', ')}`)
  if (names.some((n) => n.includes('@') || n.includes('example.com')))
    problems.push('邮箱泄漏进了文件名')
  if (code !== 0) problems.push('cargo test 失败')
  if (problems.length) {
    console.log('\nM11b 门未通过：')
    for (const p of problems) console.log(`  - ${p}`)
    process.exitCode = 1
  } else {
    console.log('\nM11b 门通过：完整一次同步对桩跑通，同步目录只多出 device-*.enc，邮箱不进文件名')
    out.split('\n').filter((l) => l.startsWith('test result')).forEach((l) => console.log(`  ${l.trim()}`))
  }
} catch (e) {
  console.error(`m11b-check 自己出错了：${e.message}`)
  process.exitCode = 1
} finally {
  stub.kill()
}
