import { describe, expect, it } from 'vitest'

import { ranked, score } from './fuzzy'

describe('fuzzy score', () => {
  it('中文靠包含，不靠首字母缩写', () => {
    expect(score('牙医', '预约牙医')).toBeGreaterThan(score('牙医', '牙刷牙线'))
    expect(score('买猫', '买猫粮（大袋）')).toBeGreaterThan(0)
    expect(score('猫粮', '买猫粮')).toBeGreaterThan(score('猫粮', '猫 粮 分开写'))
  })

  it('词首与开头命中更值钱', () => {
    expect(score('vs', 'VS Code')).toBeGreaterThan(score('co', 'VS Code'))
    expect(score('code', 'VS Code')).toBeGreaterThan(score('code', 'undocumented'))
  })

  it('不匹配就是 0，不靠低分混进结果', () => {
    expect(score('xyz', '预约牙医')).toBe(0)
    expect(score('', '任何东西')).toBe(0)
  })

  it('大小写不影响', () => {
    expect(score('VS', 'vs code')).toBeGreaterThan(0)
  })

  it('ranked 按分数排并截断', () => {
    const items = [{ n: '买猫粮' }, { n: '预约牙医' }, { n: '牙刷牙线' }]
    const out = ranked('牙', items, (x) => x.n, 2)
    expect(out).toHaveLength(2)
    expect(out[0]?.n).toContain('牙')
  })
})
