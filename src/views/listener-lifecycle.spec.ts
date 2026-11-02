import { describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import type { Component } from 'vue'

import Notes from './Notes.vue'
import Progress from './Progress.vue'

/**
 * 视图挂载/卸载与 window 监听的配对（批次 8 从盲区审计里翻出来的一类）。
 *
 * 原来 Notes / Progress 都是 `onMounted(async () => { await load(); 再 addEventListener })`：
 * 载入没回来就切走时，卸载钩子拆的是空函数（或干脆没拆），随后 onMounted 的续体照样把监听挂上 ——
 * 每次快切漏一对，而失焦时那些监听还会去 flush 一个已经卸载的模型。
 * jsdom 里 `await` 的续体照常执行，所以这条**能**在单测里钉住（不像导航那类只能真窗口验）。
 */

// 任何 api 方法都给一个"什么形状都接得住"的值：能迭代（空）、能取属性（还是它自己）、
// 能调用（还是它自己）。测试要的是"挂载时那条 await 还没回来"，不是响应内容。
const anyVal: any = new Proxy(function () {}, {
  get: (_t, prop) => {
    if (prop === Symbol.iterator) return function* () {}
    if (prop === 'length' || prop === 'size') return 0
    if (prop === 'then') return undefined
    return anyVal
  },
  apply: () => anyVal,
})

vi.mock('../api', () => ({
  api: new Proxy({}, { get: () => async () => anyVal }),
  usingMock: true,
  runningInBrowser: false,
}))

async function switchAwayWhileLoading(component: Component) {
  const winAdd = vi.spyOn(window, 'addEventListener')
  const winDel = vi.spyOn(window, 'removeEventListener')
  const docAdd = vi.spyOn(document, 'addEventListener')
  const docDel = vi.spyOn(document, 'removeEventListener')

  const w = mount(component)
  // 不 await：此刻 onMounted 里那条 `await api.…()` 还挂在 promise 上，正是"载入没回来就切走"。
  const addsAtUnmount = winAdd.mock.calls.length + docAdd.mock.calls.length
  w.unmount()
  const removedWin = winDel.mock.calls.filter(([t]) => t === 'blur').map(([, fn]) => fn)
  const removedDoc = docDel.mock.calls.filter(([t]) => t === 'visibilitychange').map(([, fn]) => fn)

  await flushPromises()
  await flushPromises()

  const addedAfterUnmount = winAdd.mock.calls.length + docAdd.mock.calls.length - addsAtUnmount
  const addedWinBlur = winAdd.mock.calls.filter(([t]) => t === 'blur').map(([, fn]) => fn)
  const addedDocVis = docAdd.mock.calls.filter(([t]) => t === 'visibilitychange').map(([, fn]) => fn)
  for (const s of [winAdd, winDel, docAdd, docDel]) s.mockRestore()
  return { addedWinBlur, addedDocVis, removedWin, removedDoc, addedAfterUnmount }
}

describe('切走时窗口监听拆干净', () => {
  it.each([
    ['笔记页', Notes],
    ['部件页', Progress],
  ])('%s：挂载时就挂上、卸载时按同一引用拆掉、续体不许补挂', async (_name, component) => {
    const got = await switchAwayWhileLoading(component as Component)

    // 1) 监听在挂载时就位（不是等数据回来才挂）
    expect(got.addedWinBlur, 'window blur 应在挂载时挂上').toHaveLength(1)
    expect(got.addedDocVis, 'document visibilitychange 应在挂载时挂上').toHaveLength(1)

    // 2) 卸载时拆掉的是**同一个函数引用** —— 拆别的东西等于没拆
    expect(got.removedWin).toEqual(got.addedWinBlur)
    expect(got.removedDoc).toEqual(got.addedDocVis)

    // 3) 载入续体放行之后，不能再补挂任何监听
    expect(got.addedAfterUnmount, '卸载后续体不该再挂监听').toBe(0)
  })
})
