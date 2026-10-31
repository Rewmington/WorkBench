// 把应用图标的美术源从「圆角深色块 + 白色背景」改成「圆角深色块 + 透明背景」。
//
// 为什么要有这个脚本：`tauri icon` 只是缩放与打包，源图四角是不透明白，
// 生成出来的 ico / png 就全都带白角 —— 任务栏与托盘上是四个白点。
// 抠图必须在源图上做一次，并把抗锯齿边缘的白边**反预乘**掉，否则深色块外沿会留一圈灰白光晕。
//
//   node tools/mkicon.mjs                 # 只报告，不写文件
//   node tools/mkicon.mjs --write         # 写出透明版源图与 tray.png
//
// PNG 编解码是手写的（本项目不引图像依赖）：只支持 8 位真彩 / RGBA / 调色板、非隔行，
// 这三样覆盖了源图与本仓库里所有图标。

import { inflateSync, deflateSync } from 'node:zlib'
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { resolve } from 'node:path'

const ROOT = resolve(import.meta.dirname, '..')
const SRC = resolve(ROOT, 'design/refs/anvil-app-icon.png')
const OUT = resolve(ROOT, 'design/refs/anvil-app-icon-transparent.png')
const TRAY = resolve(ROOT, 'src-tauri/icons/tray.png')
const WRITE = process.argv.includes('--write')
const CLEAN = process.argv.includes('--clean-watermark')

// 判定"背景"的松紧：越接近纯白越像背景。深色块本身最亮的绿也远不到 200。
const OPAQUE_MIN = 240 // min(R,G,B) >= 这个值且与外圈连通 → 完全透明
const FRINGE_MIN = 150 // 介于两者之间的是抗锯齿边缘，按覆盖度反预乘

function decodePng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error('不是 PNG')
  let off = 8
  let ihdr = null
  const idat = []
  let plte = null
  let trns = null
  while (off < buf.length) {
    const len = buf.readUInt32BE(off)
    const type = buf.toString('ascii', off + 4, off + 8)
    const data = buf.subarray(off + 8, off + 8 + len)
    if (type === 'IHDR') {
      ihdr = {
        w: data.readUInt32BE(0),
        h: data.readUInt32BE(4),
        depth: data[8],
        color: data[9],
        interlace: data[12],
      }
    } else if (type === 'IDAT') idat.push(data)
    else if (type === 'PLTE') plte = data
    else if (type === 'tRNS') trns = data
    else if (type === 'IEND') break
    off += 12 + len
  }
  if (!ihdr || ihdr.depth !== 8 || ihdr.interlace !== 0) {
    throw new Error(`只支持 8 位非隔行 PNG，拿到 ${JSON.stringify(ihdr)}`)
  }
  if (![0, 2, 3, 6].includes(ihdr.color)) throw new Error(`不支持的颜色类型 ${ihdr.color}`)

  const raw = inflateSync(Buffer.concat(idat))
  const ch = { 0: 1, 3: 1, 2: 3, 6: 4 }[ihdr.color]
  const stride = ihdr.w * ch
  const out = Buffer.alloc(ihdr.h * stride)
  let p = 0
  let prev = Buffer.alloc(stride)
  for (let y = 0; y < ihdr.h; y++) {
    const filter = raw[p++]
    const line = raw.subarray(p, p + stride)
    p += stride
    const cur = out.subarray(y * stride, (y + 1) * stride)
    for (let x = 0; x < stride; x++) {
      const a = x >= ch ? cur[x - ch] : 0
      const b = prev[x]
      const c = x >= ch ? prev[x - ch] : 0
      let v = line[x]
      if (filter === 1) v += a
      else if (filter === 2) v += b
      else if (filter === 3) v += (a + b) >> 1
      else if (filter === 4) {
        const pp = a + b - c
        const pa = Math.abs(pp - a)
        const pb = Math.abs(pp - b)
        const pc = Math.abs(pp - c)
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c
      }
      cur[x] = v & 0xff
    }
    prev = cur
  }

  // 统一成 RGBA
  const rgba = Buffer.alloc(ihdr.w * ihdr.h * 4)
  for (let i = 0, j = 0; i < ihdr.w * ihdr.h; i++) {
    let r, g, b, a
    if (ihdr.color === 6) {
      r = out[i * 4]
      g = out[i * 4 + 1]
      b = out[i * 4 + 2]
      a = out[i * 4 + 3]
    } else if (ihdr.color === 2) {
      r = out[i * 3]
      g = out[i * 3 + 1]
      b = out[i * 3 + 2]
      a = 255
    } else {
      const idx = ihdr.color === 0 ? out[i] : out[i]
      r = plte[idx * 3]
      g = plte[idx * 3 + 1]
      b = plte[idx * 3 + 2]
      a = trns && idx < trns.length ? trns[idx] : 255
    }
    rgba[j++] = r
    rgba[j++] = g
    rgba[j++] = b
    rgba[j++] = a
  }
  return { w: ihdr.w, h: ihdr.h, data: rgba }
}

const CRC_TABLE = (() => {
  const t = new Int32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    t[n] = c
  }
  return t
})()

function crc32(buf) {
  let c = -1
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8)
  return (c ^ -1) >>> 0
}

function chunk(type, data) {
  const head = Buffer.alloc(8)
  head.writeUInt32BE(data.length, 0)
  head.write(type, 4, 'ascii')
  const tail = Buffer.alloc(4)
  tail.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0)
  return Buffer.concat([head, data, tail])
}

function encodePng({ w, h, data }) {
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(w, 0)
  ihdr.writeUInt32BE(h, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // RGBA
  const raw = Buffer.alloc(h * (1 + w * 4))
  for (let y = 0; y < h; y++) {
    raw[y * (1 + w * 4)] = 0 // 每行都用 filter 0：图标这种平色图 deflate 后本来就不大
    data.copy(raw, y * (1 + w * 4) + 1, y * w * 4, (y + 1) * w * 4)
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

/** 从四条边同时往里漫填：只有与外圈连通的近白像素才算背景，块内部的浅色不会被误伤。 */
function findBackground({ w, h, data }) {
  const bg = new Uint8Array(w * h)
  const queue = new Int32Array(w * h)
  let q = 0
  const push = (i) => {
    if (bg[i]) return
    const p = i * 4
    if (Math.min(data[p], data[p + 1], data[p + 2]) < OPAQUE_MIN) return
    bg[i] = 1
    queue[q++] = i
  }
  for (let x = 0; x < w; x++) {
    push(x)
    push((h - 1) * w + x)
  }
  for (let y = 0; y < h; y++) {
    push(y * w)
    push(y * w + w - 1)
  }
  for (let head = 0; head < q; head++) {
    const i = queue[head]
    const x = i % w
    const y = (i / w) | 0
    if (x > 0) push(i - 1)
    if (x < w - 1) push(i + 1)
    if (y > 0) push(i - w)
    if (y < h - 1) push(i + w)
  }
  return bg
}

/**
 * 抠背景 + 去白边。
 *
 * 深色块是画在白底上的，所以边缘像素 = 块色 × a + 白 × (1 - a)。
 * 反预乘回不透明色并把 a 设成估算覆盖率，边缘就不会留一圈灰白。
 * 覆盖度用"离纯白还有多远"来估：min 通道越接近 255 越像背景。
 */
function cutout(img) {
  const { w, h, data } = img
  const bg = findBackground(img)
  let cleared = 0
  let fringed = 0
  for (let i = 0; i < w * h; i++) {
    const p = i * 4
    const r = data[p]
    const g = data[p + 1]
    const b = data[p + 2]
    if (bg[i]) {
      data[p + 3] = 0
      cleared++
      continue
    }
    const mn = Math.min(r, g, b)
    if (mn >= FRINGE_MIN && mn < OPAQUE_MIN) {
      // 与背景相邻才算边缘，块内部的浅色不动
      const x = i % w
      const y = (i / w) | 0
      const nearBg =
        (x > 0 && bg[i - 1]) || (x < w - 1 && bg[i + 1]) || (y > 0 && bg[i - w]) || (y < h - 1 && bg[i + w])
      if (!nearBg) continue
      const a = Math.min(255, Math.max(1, Math.round((255 - mn) * (255 / (255 - FRINGE_MIN)))))
      const unpremul = (c) => Math.min(255, Math.max(0, Math.round((c - 255 * (1 - a / 255)) / (a / 255))))
      data[p] = unpremul(r)
      data[p + 1] = unpremul(g)
      data[p + 2] = unpremul(b)
      data[p + 3] = a
      fringed++
    }
  }
  return { cleared, fringed }
}


/**
 * 美术源右下角有「Qoder AI生成」水印。只清那一块的浅色像素：
 * 深色背景与圆角透明度原样保留，水印不会跟着进 launcher。
 */
function eraseWatermark(img) {
  const { w, h, data } = img
  const sample = (900 * w + 820) * 4
  const [br, bg, bb] = [data[sample], data[sample + 1], data[sample + 2]]
  let n = 0
  for (let y = 950; y < h; y++) {
    for (let x = 830; x < w; x++) {
      const p = (y * w + x) * 4
      if (data[p + 3] === 0) continue
      data[p] = br
      data[p + 1] = bg
      data[p + 2] = bb
      n++
    }
  }
  return n
}

/** 面积平均缩放：图标这种小图不需要更讲究的滤波器。 */
function resize(img, size) {
  const { w, h, data } = img
  const out = Buffer.alloc(size * size * 4)
  for (let y = 0; y < size; y++) {
    const y0 = Math.floor((y * h) / size)
    const y1 = Math.max(y0 + 1, Math.floor(((y + 1) * h) / size))
    for (let x = 0; x < size; x++) {
      const x0 = Math.floor((x * w) / size)
      const x1 = Math.max(x0 + 1, Math.floor(((x + 1) * w) / size))
      let r = 0
      let g = 0
      let b = 0
      let a = 0
      let n = 0
      for (let sy = y0; sy < y1; sy++) {
        for (let sx = x0; sx < x1; sx++) {
          const p = (sy * w + sx) * 4
          const al = data[p + 3]
          // 预乘后再平均，否则透明像素会把颜色拉黑
          r += data[p] * al
          g += data[p + 1] * al
          b += data[p + 2] * al
          a += al
          n++
        }
      }
      const p = (y * size + x) * 4
      const av = a / n
      if (av === 0) {
        out[p] = out[p + 1] = out[p + 2] = out[p + 3] = 0
        continue
      }
      out[p] = Math.round(r / a)
      out[p + 1] = Math.round(g / a)
      out[p + 2] = Math.round(b / a)
      out[p + 3] = Math.round(av)
    }
  }
  return { w: size, h: size, data: out }
}

const corners = (img) => {
  const { w, h, data } = img
  const at = (x, y) => {
    const p = (y * w + x) * 4
    return `A=${data[p + 3]} rgb=${data[p]},${data[p + 1]},${data[p + 2]}`
  }
  return `${at(0, 0)} | ${at(w - 1, 0)} | ${at(0, h - 1)} | ${at(w - 1, h - 1)}`
}

if (!existsSync(SRC)) throw new Error(`找不到图标源文件：${SRC}`)

const src = decodePng(readFileSync(SRC))
const before = corners(src)
const { cleared, fringed } = cutout(src)
const erased = CLEAN ? eraseWatermark(src) : 0
const tray = resize(src, 24)

console.log(`源图 ${src.w}x${src.h}`)
console.log(`  改前四角  ${before}`)
console.log(`  改后四角  ${corners(src)}`)
console.log(`  抠掉 ${cleared} 个背景像素，反预乘 ${fringed} 个边缘像素`)
console.log(`  托盘 24px 四角  ${corners(tray)}  中心 A=${tray.data[(12 * 24 + 12) * 4 + 3]}`)

if (!WRITE) {
  console.log('\n只报告。加 --write 才落盘。')
} else {
  writeFileSync(OUT, encodePng(src))
  writeFileSync(TRAY, encodePng(tray))
  console.log(`\n✓ ${OUT}`)
  console.log(`✓ ${TRAY}`)
  console.log('剩下的尺寸交给：npx tauri icon design/refs/anvil-app-icon-transparent.png')
}
