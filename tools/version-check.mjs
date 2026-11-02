#!/usr/bin/env node
/**
 * 版本一致性检查：四处版本号必须完全一致，且推 tag 时 tag 必须就是那个版本。
 *
 * 为什么需要它：发版是手工抬四处版本（`package.json`、`src-tauri/tauri.conf.json`、
 * `src-tauri/Cargo.toml`、`mobile/pubspec.yaml`），然后打 `vX.Y.Z` 推上去。少抬一处
 * 或 tag 打错，CI **照样绿**：产物名是 `Anvil-vX.Y.Z-…`，里面装的却是上一个版本 ——
 * 从名字、Release 页面到"关于"页都看不出来，只有出事后回查 `Cargo.toml` 才发现。
 * 这跟 P1-18（缺签名时静默出 debug 包）是同一类：发布线上"错了但看着正常"。
 *
 * 用法：node tools/version-check.mjs              本地发版前跑
 *       node tools/version-check.mjs --tag=v0.1.7  CI 推 tag 时跑（对不上就非零退出）
 */
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname.replace(/^\/(.:)/, '$1')

const read = (rel) => readFileSync(join(root, rel), 'utf8')

const sources = [
  { name: 'package.json', version: () => JSON.parse(read('package.json')).version },
  { name: 'src-tauri/tauri.conf.json', version: () => JSON.parse(read('src-tauri/tauri.conf.json')).version },
  // 只取 `^version = ` 那一行：依赖写的是 `foo = { version = … }`，行首是包名，不会误中。
  { name: 'src-tauri/Cargo.toml', version: () => /^version\s*=\s*"([^"]+)"/m.exec(read('src-tauri/Cargo.toml'))?.[1] },
  // pubspec 是 `0.1.6+7`，`+` 后面是 Android 的 versionCode，不参与版本号比对。
  {
    name: 'mobile/pubspec.yaml',
    version: () => /^version:\s*([0-9.]+)(?:\+(\d+))?/m.exec(read('mobile/pubspec.yaml'))?.[1],
    build: () => /^version:\s*[0-9.]+\+(\d+)/m.exec(read('mobile/pubspec.yaml'))?.[1],
  },
]

const parsed = sources.map((s) => ({ name: s.name, version: s.version() }))
const missing = parsed.filter((p) => !p.version)
if (missing.length) {
  console.error(`读出版本号失败：${missing.map((m) => m.name).join('、')} —— 格式大概被改了`)
  process.exit(1)
}

const distinct = [...new Set(parsed.map((p) => p.version))]
const detail = parsed.map((p) => `${p.name} = ${p.version}`).join('\n  ')
if (distinct.length > 1) {
  console.error(`::error::四处版本不一致，产物会带着对不上的版本号发出去\n  ${detail}`)
  process.exit(1)
}

const version = distinct[0]
const rawTag = process.argv.find((a) => a.startsWith('--tag='))?.slice('--tag='.length)
const tag = rawTag && rawTag !== 'manual' ? rawTag.replace(/^v/, '') : undefined
if (tag && tag !== version) {
  console.error(`::error::tag 是 ${tag}，四处清单写的都是 ${version} —— 要么先提交一次版本抬升，要么别推这个 tag`)
  process.exit(1)
}

const pubspec = sources.find((s) => s.build)
const verdict = tag ? `与 tag v${tag} 对得上` : '（没给 tag，只查了四处一致）'
console.log(`版本一致：${version}（Android build ${pubspec?.build()}），${verdict}`)
