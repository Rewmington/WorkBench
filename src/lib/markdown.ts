import DOMPurify from 'dompurify'
import { marked } from 'marked'

/**
 * Markdown → 安全的 HTML（PLAN §3：v1 是「源码 textarea + 实时预览」双栏）。
 *
 * 笔记正文是**用户自己写的不可信输入**：它会经过 SQLite、备份文件、（v2）坚果云，
 * 任何一环被塞进 `<img onerror=...>` 都会在渲染时执行。所以这里用白名单而不是黑名单，
 * 并且刻意不用 DOMPurify 的默认 ALLOWED_TAGS —— 默认集比 Markdown 实际需要的宽得多，
 * 收紧到「marked 真会产出的那些标签」，多一个都不给。
 */

marked.setOptions({ gfm: true, breaks: false, async: false })

const ALLOWED_TAGS = [
  'a', 'p', 'br', 'hr',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'strong', 'b', 'em', 'i', 'del', 's',
  'code', 'pre', 'kbd', 'samp',
  'blockquote', 'ul', 'ol', 'li',
  'table', 'thead', 'tbody', 'tfoot', 'tr', 'th', 'td',
  'input', // GFM 任务列表的复选框，永远 disabled，纯展示
  'span', 'div',
]

const ALLOWED_ATTR = ['href', 'title', 'align', 'type', 'checked', 'disabled']

/** 只放行这几种协议。`javascript:` / `data:text/html` / `vbscript:` 一律不算安全。 */
export const SAFE_URI = /^(?:https?:|mailto:|ftp:|#|\/|\.\/|\.\.\/)/i

export function isSafeUri(value: string): boolean {
  return SAFE_URI.test(value.trim())
}

/**
 * 第二道防线：DOMPurify 已经会剥掉大部分危险协议，但「哪些协议算安全」是产品的决定，
 * 不该外包给库的默认值 —— 它哪天放宽，我们就跟着放宽了。
 */
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  for (const attr of ['href', 'src'] as const) {
    const value = node.getAttribute(attr)
    if (value !== null && !isSafeUri(value)) node.removeAttribute(attr)
  }
  if (node.nodeName === 'A' && node.getAttribute('href')) {
    // 外链不该能反向操纵宿主窗口
    node.setAttribute('rel', 'noopener noreferrer')
  }
  if (node.nodeName === 'INPUT') node.setAttribute('disabled', '')
})

export interface RenderedNote {
  html: string
  /**
   * 被剥掉的不安全链接数。>0 时预览区顶部要说明，而不是静默吃掉用户写的内容。
   *
   * 计数在**净化之前**扫原文：DOMPurify 会在自己的钩子之前就把 `javascript:` 的 href
   * 整个删掉，挂在后面的计数器根本看不见那一笔（第一版就栽在这里，测出来 blocked 恒 0）。
   */
  blocked: number
}

const UNSAFE_IN_RAW = /(?:href|src)\s*=\s*(?:"|')(?:\s*)(?:javascript|data|vbscript|file):/gi

export function renderMarkdown(source: string): RenderedNote {
  const raw = marked.parse(source ?? '') as string
  const blocked = (raw.match(UNSAFE_IN_RAW) ?? []).length
  const html = DOMPurify.sanitize(raw, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    // 连 style 都不留：外观由设计令牌说了算（§8.5「不受令牌控制的就是债」）
    ALLOW_DATA_ATTR: false,
  })
  return { html, blocked }
}

/**
 * 预览里的链接一律交给系统浏览器（§8.6 反网页感：原生软件不在自己窗口里开网页）。
 * 不安全的（已被剥掉 href 的）返回 null，调用方据此什么都不做。
 *
 * `#锚点` 也返回 null：它是页内跳转，交给 `ShellExecute` 只会拿到一句看不懂的失败，
 * 而在 webview 里跳它等于让笔记反向操纵宿主窗口。
 */
export function linkHref(target: EventTarget | null): string | null {
  const el = (target as HTMLElement | null)?.closest?.('a')
  const href = el?.getAttribute('href')
  if (!href || !isSafeUri(href) || href.startsWith('#')) return null
  return href
}
