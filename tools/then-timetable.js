(async () => {
  const s = (ms) => new Promise((r) => setTimeout(r, ms))
  const set = (el, v) => {
    Object.getOwnPropertyDescriptor(el.constructor.prototype, 'value').set.call(el, v)
    el.dispatchEvent(new Event('input', { bubbles: true }))
  }
  const need = document.querySelector('.need input')
  if (need) {
    set(need, '2026-09-07')
    await s(80)
    ;[...document.querySelectorAll('.need button')].find((b) => b.textContent.includes('保存')).click()
    await s(600)
  }
  const cell = document.querySelector('.cls')
  if (!cell) throw new Error('课表里没有可点的格子')
  cell.click()
  await s(400)
  const dl = document.querySelector('.y-detail .y-dl')
  return (
    'panel=' + (document.querySelector('.y-detail .dt')?.textContent ?? 'none') +
    ' 明细=' + (dl ? dl.textContent.replace(/\s+/g, ' ').slice(0, 60) : 'none')
  )
})()
