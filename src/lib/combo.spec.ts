import { describe, expect, it } from 'vitest'

import {
  captureCombo,
  clashWithInApp,
  displayCombo,
  isModifierOnly,
  sameCombo,
} from './combo'

/** 造一次 keydown：只填 combo.ts 真正读的那几个字段。 */
function key(code: string, mods: { ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean } = {}) {
  return {
    code,
    ctrlKey: !!mods.ctrl,
    altKey: !!mods.alt,
    shiftKey: !!mods.shift,
    metaKey: !!mods.meta,
  }
}

describe('captureCombo 的序列化', () => {
  it('修饰键在前、主键在后，字母去掉 Key 前缀', () => {
    expect(captureCombo(key('KeyK', { ctrl: true, shift: true }))).toBe('Ctrl+Shift+K')
  })

  it('数字键去掉 Digit 前缀', () => {
    expect(captureCombo(key('Digit3', { ctrl: true }))).toBe('Ctrl+3')
  })

  it('顺序固定，与按下顺序无关', () => {
    // 先按 Shift 再按 Ctrl，出来的串必须和反过来的那次一致，否则同一次绑定会写出两个值
    expect(captureCombo(key('KeyA', { shift: true, alt: true }))).toBe('Alt+Shift+A')
    expect(captureCombo(key('KeyA', { alt: true, shift: true }))).toBe('Alt+Shift+A')
  })

  it('纯修饰键与裸键都不构成快捷键', () => {
    expect(captureCombo(key('ControlLeft'))).toBeNull()
    expect(captureCombo(key('KeyA'))).toBeNull()
    expect(isModifierOnly('AltRight')).toBe(true)
    expect(isModifierOnly('KeyA')).toBe(false)
  })

  it('输不进全局快捷键的键直接拒绝，而不是给一个绑不上的串', () => {
    expect(captureCombo(key('Escape', { ctrl: true }))).toBeNull()
    expect(captureCombo(key('CapsLock', { ctrl: true }))).toBeNull()
  })

  it('恰好一个非修饰键 token：分隔符是 + ，所以主键只能是最后一位', () => {
    const combo = captureCombo(key('F5', { ctrl: true, alt: true }))
    expect(combo).toBe('Ctrl+Alt+F5')
    const mods = new Set(['Ctrl', 'Alt', 'Shift', 'Super'])
    const parts = combo!.split('+')
    expect(parts.filter((p) => !mods.has(p))).toEqual([parts.at(-1)])
  })

  it('加号键只能以物理 = 键的形式绑上', () => {
    // `+` 是分隔符，永远 parse 不进去；Shift+Equal 才是那个物理动作
    expect(captureCombo(key('Equal', { ctrl: true, shift: true }))).toBe('Ctrl+Shift+Equal')
  })
})

describe('displayCombo', () => {
  it('把 token 换成人看得懂的话', () => {
    expect(displayCombo('Ctrl+Shift+A')).toBe('Ctrl + Shift + A')
    expect(displayCombo('Ctrl+Alt+Space')).toBe('Ctrl + Alt + 空格')
    expect(displayCombo('Ctrl+ArrowUp')).toBe('Ctrl + ↑')
    expect(displayCombo('Ctrl+Equal')).toBe('Ctrl + =')
  })

  it('认不出的 token 原样留着，不假装成功', () => {
    expect(displayCombo('Ctrl+KeyQ')).toBe('Ctrl + KeyQ')
  })
})

describe('sameCombo', () => {
  it('大小写与顺序都不算差异', () => {
    expect(sameCombo('shift+alt+a', 'Alt+Shift+A')).toBe(true)
    expect(sameCombo('Alt+Shift+A', 'Alt+Shift+B')).toBe(false)
  })
})

describe('clashWithInApp', () => {
  it('认出和应用内快捷键撞车的绑定', () => {
    expect(clashWithInApp('ctrl+k')).toBe('命令面板')
    expect(clashWithInApp('Ctrl+Shift+K')).toBeNull()
  })
})
