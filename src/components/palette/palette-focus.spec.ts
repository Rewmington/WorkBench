/**
 * 命令面板的焦点契约（批次 9 的第 8 条：真缺陷，`role="dialog" aria-modal="true"` 那两句承诺）。
 *
 * 从前 `@keydown` 只绑在 `<input>` 上，也没有焦点陷阱：
 * Tab  once 出到列表按钮或遮罩后面，Esc 就不再关面板，而遮罩下的控件照样 Tab 得到 ——
 * 声明了 modal 却做不到 modal，读屏与只用键盘的用户会卡在面板里。
 * 这里钉四件事：打开时焦点进来、Tab 出不去、Shift+Tab 也出不去、焦点落在按钮上时 Esc 照样关、
 * 关掉时焦点回到打开前那把椅子上。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import { mount } from '@vue/test-utils'

import CommandPalette from './CommandPalette.vue'
import { usePalette } from '../../composables/usePalette'

const anyVal: any = new Proxy(function () {}, {
  get: (_t, prop) => {
    if (prop === Symbol.iterator) return function* () {}
    if (prop === 'length' || prop === 'size') return 0
    if (prop === 'then') return undefined
    return anyVal
  },
  apply: () => anyVal,
})

vi.mock('../../api', () => ({
  api: new Proxy({}, { get: () => async () => anyVal }),
  usingMock: true,
  runningInBrowser: false,
}))

const pal = () => document.querySelector<HTMLElement>('.pal')
const focusables = (root: HTMLElement) =>
  Array.from(root.querySelectorAll<HTMLElement>('input, button, [tabindex]:not([tabindex="-1"])'))

function press(el: HTMLElement, key: string, shift = false) {
  el.focus()
  const ev = new KeyboardEvent('keydown', { key, shiftKey: shift, bubbles: true, cancelable: true })
  el.dispatchEvent(ev)
  return ev
}

let w: ReturnType<typeof mount> | undefined

async function openPalette() {
  w = mount(CommandPalette)
  const { open } = usePalette()
  open.value = true
  await nextTick()
  await nextTick()
  return { open }
}

beforeEach(() => {
  usePalette().hide()
})

afterEach(() => {
  w?.unmount()
  w = undefined
  usePalette().hide()
  document.body.innerHTML = ''
})

describe('命令面板是它自己声明的那个 modal', () => {
  it('打开时焦点落在搜索框里', async () => {
    await openPalette()
    expect(pal()).not.toBeNull()
    expect(document.activeElement).toBe(pal()!.querySelector('input'))
  })

  it('Tab 到底部绕回第一个，Shift+Tab 到顶部绕到最后一个', async () => {
    await openPalette()
    const items = focusables(pal()!)
    expect(items.length, '面板里至少要有一个输入框和一行按钮').toBeGreaterThan(1)

    const atEnd = press(items[items.length - 1]!, 'Tab')
    expect(atEnd.defaultPrevented, '不拦 Tab 就是放行到遮罩后面').toBe(true)
    expect(document.activeElement).toBe(items[0])

    const atStart = press(items[0]!, 'Tab', true)
    expect(atStart.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(items[items.length - 1])
  })

  it('焦点在列表按钮上时，Esc 仍然关得掉面板', async () => {
    const { open } = await openPalette()
    const btn = pal()!.querySelector('button')!
    press(btn, 'Escape')
    expect(open.value, 'Esc 从前只绑在输入框上，焦点一走出输入框就失灵').toBe(false)
  })

  it('焦点在按钮上时回车按的是那一行，不是光标那一行', async () => {
    const { open } = await openPalette()
    const btn = pal()!.querySelector('button')!
    const ev = press(btn, 'Enter')
    expect(ev.defaultPrevented, '容器 handler 不该抢走按钮自己的 Enter').toBe(false)
    expect(open.value, '按钮的 click 没在这条测试里触发，面板不该被容器抢先关掉').toBe(true)
  })

  it('关掉时焦点回到打开前那把椅子上', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()
    expect(document.activeElement).toBe(trigger)

    const { open } = await openPalette()
    expect(document.activeElement).not.toBe(trigger)

    open.value = false
    await nextTick()
    expect(document.activeElement).toBe(trigger)
  })
})
