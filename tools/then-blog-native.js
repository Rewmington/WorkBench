(async () => {
  const s = (ms) => new Promise((r) => setTimeout(r, ms))
  const nav = [...document.querySelectorAll('.y-nav button, .y-rail button')].find((x) =>
    (x.textContent || '').includes('博客'),
  )
  if (!nav) return 'no-nav'
  nav.click()
  await s(1500)
  const retry = [...document.querySelectorAll('button')].find((b) => b.textContent.includes('重新读取'))
  retry?.click()
  // 七个清单并发抓，站点冷启实测 ~8 s
  await s(11000)
  const blocks = [...document.querySelectorAll('.blk h3')].map((x) =>
    x.textContent.replace(/\s+/g, ' ').trim(),
  )
  return JSON.stringify({
    降级横幅: (document.querySelector('.stale')?.textContent ?? '（无）').replace(/\s+/g, ' ').trim().slice(0, 70),
    各块标题: blocks,
    项目卡数: document.querySelectorAll('.cards .card').length,
    顶部时间: (document.querySelector('.ago')?.textContent ?? '').replace(/\s+/g, ' ').trim(),
  })
})()
