(async () => {
  const s = (ms) => new Promise((r) => setTimeout(r, ms))
  const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '清空')
  if (!btn) return 'no-清空-button'
  btn.click()
  await s(250)
  return '确认行=' + !!document.querySelector('.askline')
})()
