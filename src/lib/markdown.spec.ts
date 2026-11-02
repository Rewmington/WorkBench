import { describe, expect, it } from 'vitest'

import { linkHref, renderMarkdown } from './markdown'

/**
 * 笔记正文是不可信输入：它要过 SQLite、备份文件、（v2）坚果云，
 * 任何一环被塞进可执行内容都会在预览时跑起来。这三条注入用例是 M6 的验收项。
 */
describe('Markdown 净化', () => {
  it('剥掉 <script>', () => {
    const { html } = renderMarkdown('# 标题\n\n<script>alert(1)</script>\n\n正文')
    expect(html).not.toContain('<script')
    expect(html).not.toContain('alert(1)')
    expect(html).toContain('<h1>标题</h1>')
    expect(html).toContain('正文')
  })

  it('剥掉 javascript: 链接并把它计入 blocked', () => {
    const { html, blocked } = renderMarkdown('[点我](javascript:alert(1))')
    expect(html).not.toMatch(/href/i)
    expect(html).not.toContain('alert(1)')
    expect(blocked).toBe(1)
    expect(html).toContain('点我') // 文字留着，只是不再是链接
  })

  it('剥掉 onerror 这类事件属性', () => {
    const { html } = renderMarkdown('<img src="x" onerror="alert(1)">\n\n![alt](https://ok.example/a.png)')
    expect(html.toLowerCase()).not.toContain('onerror')
    expect(html.toLowerCase()).not.toContain('alert(1)')
  })

  it('data: 与 vbscript: 协议同样不放行', () => {
    const a = renderMarkdown('[x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)')
    expect(a.html).not.toMatch(/href=["']?data:/i)
    const b = renderMarkdown('[y](vbscript:msgbox)')
    expect(b.html).not.toMatch(/href=["']?vbscript:/i)
    expect(a.blocked + b.blocked).toBe(2)
  })

  it('style 属性不留：外观归设计令牌', () => {
    const { html } = renderMarkdown('<p style="color:red">红</p>')
    expect(html.toLowerCase()).not.toContain('style=')
    expect(html).toContain('红')
  })

  it('正常 Markdown 该留的都留着', () => {
    const { html, blocked } = renderMarkdown(
      [
        '## 小节',
        '',
        '- 列表项 **粗** 与 *斜* 与 ~~删~~',
        '- `行内码`',
        '',
        '> 引用',
        '',
        '```ts',
        'const a = 1',
        '```',
        '',
        '| 甲 | 乙 |',
        '| -- | -- |',
        '| 1  | 2  |',
        '',
        '- [x] 已完成的任务',
        '',
        '[链接](https://wmddd.online)',
      ].join('\n'),
    )
    expect(blocked).toBe(0)
    expect(html).toContain('<h2>小节</h2>')
    expect(html).toContain('<strong>粗</strong>')
    expect(html).toContain('<em>斜</em>')
    expect(html).toContain('<del>删</del>')
    expect(html).toContain('<code>行内码</code>')
    // `class` 不在属性白名单里：放开它就等于允许用户往渲染结果里塞任意类名，
    // 而 v1 不做语法高亮，language-ts 这个类留着也没人消费。代码内容本身必须完好。
    expect(html).toContain('<pre><code>const a = 1')
    expect(html).toContain('<table>')
    expect(html).toContain('<blockquote>')
    expect(html).toContain('type="checkbox"')
    expect(html).toContain('href="https://wmddd.online"')
    expect(html).toContain('rel="noopener noreferrer"')
  })

  it('任务列表的复选框永远是 disabled（预览不是编辑器）', () => {
    const { html } = renderMarkdown('- [ ] 没做\n- [x] 做了')
    const boxes = html.match(/<input[^>]*>/g) ?? []
    expect(boxes).toHaveLength(2)
    for (const box of boxes) expect(box).toContain('disabled')
  })

  it('锚文本里的尖括号不会被当成标签吞掉后面的内容', () => {
    const { html } = renderMarkdown('比较 a < b 且 c > d')
    expect(html).toContain('a &lt; b')
    expect(html).toContain('c &gt; d')
  })
})

describe('预览里的链接提取', () => {
  const anchor = (href: string | null) => ({
    closest: () => (href === null ? null : { getAttribute: (name: string) => (name === 'href' ? href : null) }),
  })

  it('普通链接给出 href', () => {
    expect(linkHref(anchor('https://a.dev') as unknown as EventTarget)).toBe('https://a.dev')
    expect(linkHref(anchor('mailto:me@example.com') as unknown as EventTarget)).toBe('mailto:me@example.com')
  })

  it('不安全协议不返回，免得我们又把它打开', () => {
    // 渲染时 href 已被剥掉，这里模拟「属性还在但协议不对」的漏网情况
    expect(linkHref(anchor('javascript:alert(1)') as unknown as EventTarget)).toBeNull()
    expect(linkHref(anchor('data:text/html,<script>') as unknown as EventTarget)).toBeNull()
  })

  it('非链接元素返回 null', () => {
    expect(linkHref(anchor(null) as unknown as EventTarget)).toBeNull()
    expect(linkHref(null)).toBeNull()
  })

  it('页内锚点不返回：那是滚动，不是要开窗口的地址', () => {
    expect(linkHref(anchor('#section') as unknown as EventTarget)).toBeNull()
  })

  it('UNC 与相对路径照原样交出去，由命令那一侧拒绝', () => {
    // 不在这里静默吞掉 —— 前端吞了就又是「点了没反应」，正是 open_url 曾经整条哑掉的样子。
    // ShellExecute 会把 //host/share 当 UNC 路径拿当前凭据去访问，所以拒绝它的是 Rust。
    expect(linkHref(anchor('//host/share') as unknown as EventTarget)).toBe('//host/share')
    expect(linkHref(anchor('./sibling.md') as unknown as EventTarget)).toBe('./sibling.md')
  })
})
