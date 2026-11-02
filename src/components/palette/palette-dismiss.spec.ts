/**
 * 命令面板执行动作后要立刻关掉（2026-11-02 扫这一路时撞见的）。
 *
 * 从前 `runRow` 是 `await row.run()` 再 close：于是「立即备份到坚果云」按下去之后，
 * 这层模态要一直压到整份加密上传结束（几十秒），底下的备份页——也就是唯一会写出
 * 结果与流水的那一页——完全看不见。面板的任务只是把这一脚踢出去，不是替它按完倒计时。
 *
 * 面板是 `<Teleport>` 到 body 的，所以取元素走 document（与同目录那份焦点测试同一做法）。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'

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

const release: { current: (() => void) | null } = { current: null }

vi.mock('../../composables/useActions', () => ({
  useActions: () => [
    {
      id: 'backup.now',
      label: '立即备份到坚果云',
      group: '系统',
      // 手动可控：这样才证得了「面板关在动作回来之前」
      run: () => new Promise<void>((res) => (release.current = res)),
    },
  ],
}))

const CommandPalette = (await import('./CommandPalette.vue')).default
const { usePalette } = await import('../../composables/usePalette')

const pal = () => document.querySelector<HTMLElement>('.pal')
const rowOf = () =>
  Array.from(pal()?.querySelectorAll<HTMLButtonElement>('button') ?? []).find((b) =>
    (b.textContent ?? '').includes('立即备份'),
  )

let w: VueWrapper | null = null

beforeEach(() => {
  release.current = null
})

afterEach(() => {
  // 面板是 Teleport 到 body 的：不清掉 wrapper 就会把上一份留在页面上，
  // 而直接 innerHTML = '' 会让还在挂载的组件下次渲染时找不到父节点（insertBefore of null）。
  w?.unmount()
  w = null
  usePalette().hide()
})

describe('执行与关闭', () => {
  it('点一下就把面板关掉，不等那个动作跑完', async () => {
    const palette = usePalette()
    w = mount(CommandPalette)
    palette.show()
    await flushPromises()

    const row = rowOf()
    expect(row, '动作行得在').toBeTruthy()
    row!.click()
    await new Promise((r) => setTimeout(r, 0))

    expect(pal(), '动作还没回来，面板就该已经关了').toBeNull()
    expect(release.current, '动作本身确实被踢出去了').toBeTruthy()
    release.current!()
  })

  it('Shift 执行保留面板（连着做同一件事的口径不变）', async () => {
    const palette = usePalette()
    w = mount(CommandPalette)
    palette.show()
    await flushPromises()

    const row = rowOf()
    expect(row).toBeTruthy()
    row!.dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: true }))
    await new Promise((r) => setTimeout(r, 0))

    expect(pal(), 'Shift 那一路仍然要留着面板').toBeTruthy()
    expect(rowOf(), '而且列表不能被这一次执行清掉').toBeTruthy()
    release.current?.()
  })
})
