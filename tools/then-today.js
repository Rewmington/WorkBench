(async () => {
  const s = (ms) => new Promise((r) => setTimeout(r, ms))
  const set = (el, v) => {
    Object.getOwnPropertyDescriptor(el.constructor.prototype, 'value').set.call(el, v)
    el.dispatchEvent(new Event('input', { bubbles: true }))
  }
  const nav = (v) => {
    const b = [...document.querySelectorAll('.y-nav button, .y-rail button')].find((x) =>
      (x.textContent || '').includes(v),
    )
    if (!b) throw new Error('no-nav:' + v)
    b.click()
  }

  // 1) 开学日走课表页那个真输入框：不设就没有"第几周"，空档行也不该出现
  nav('课表')
  await s(500)
  const need = document.querySelector('.need input')
  if (need) {
    set(need, '2026-09-07')
    await s(80)
    const save = [...document.querySelectorAll('.need button')].find((b) => b.textContent.includes('保存'))
    save.click()
    await s(600)
  }

  // 2) 回待办造三条：两条有截止（一晚一午）、一条没时间 —— 两条带都要有内容才看得出形状
  nav('待办')
  await s(600)
  const inp = document.querySelector('.composer input')
  const titles = () => [...document.querySelectorAll('.row .t')].map((x) => x.textContent.trim())
  const add = async (t) => {
    set(inp, t)
    inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await s(500)
  }
  for (const t of ['交实验报告', '读论文', '整理报销']) if (!titles().includes(t)) await add(t)

  const rowOf = (t) => [...document.querySelectorAll('.row')].find((r) => r.querySelector('.t')?.textContent.trim() === t)
  const pickDue = async (t, part) => {
    const r = rowOf(t)
    if (!r) throw new Error('no-row:' + t)
    r.querySelector('.due').click()
    await s(300)
    const pop = document.querySelector('.pop')
    const h = [...pop.querySelectorAll('.foot .hrs button')].find((b) => b.textContent.includes(part))
    h.click()
    await s(150)
    pop.querySelector('.d.today').click()
    await s(550)
  }
  // 每次都重设：同样的值，幂等，但保证三档宽度拍到的是同一个状态
  await pickDue('交实验报告', '晚上')
  await pickDue('读论文', '上午')
  const gap = document.querySelector('.gap')
  return 'gap=' + (gap ? gap.textContent.trim() : 'none') + ' bands=' + document.querySelectorAll('.bandh').length
})()
