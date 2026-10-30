(async () => {
  const s = (ms) => new Promise((r) => setTimeout(r, ms))
  const set = (el, v) => {
    Object.getOwnPropertyDescriptor(el.constructor.prototype, 'value').set.call(el, v)
    el.dispatchEvent(new Event('input', { bubbles: true }))
  }
  const mail = document.querySelector('input[type=email]')
  const pass = document.querySelector('input[type=password]')
  if (!mail || !pass) throw new Error('没找到凭据表单')
  set(mail, 'me@example.com')
  await s(60)
  set(pass, 'app-password')
  await s(60)
  const save = [...document.querySelectorAll('button')].find((b) => b.textContent.includes('保存并立即读一次'))
  save.click()
  await s(1200)
  // 存完之后应用自己把密码框清空 —— 这就是要拍的"换页回来看着像丢了"那一幕
  return (
    'pass 框=' + (pass.value === '' ? '空' : '有字') +
    ' 占位=' + pass.placeholder +
    ' 已存提示=' + [...document.querySelectorAll('.why')].some((p) => p.textContent.includes('medicine-webdav')) +
    ' 保存键可点=' + ![...document.querySelectorAll('button')].find((b) => b.textContent.includes('保存并立即读一次')).disabled
  )
})()
