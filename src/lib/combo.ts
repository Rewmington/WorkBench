/**
 * 组合键的「捕获 → 序列化 → 显示」三件事。
 *
 * 序列化出来的字符串要能被 Rust 侧 `HotKey::from_str` 吃下去，那套文法有两条约束
 * 直接决定了这里的写法：
 * 1. 主键必须排在最后（`Ctrl+C+Shift` 会报格式错），所以这里先拼修饰键再拼主键；
 * 2. 分隔符是 `+`，所以 `+` 永远不可能作为主键出现 —— 想绑它只能绑物理 `=` 键。
 *
 * 主键取 `KeyboardEvent.code`（物理键位）而不是 `key`：全局快捷键绑的是「按哪颗键」，
 * 切了输入法、开了大写都不该让它变成另一个键。
 */

/** 修饰键的序列化顺序，与显示顺序一致。 */
const MODIFIERS = [
  { token: 'Ctrl', codes: ['ControlLeft', 'ControlRight'] },
  { token: 'Alt', codes: ['AltLeft', 'AltRight'] },
  { token: 'Shift', codes: ['ShiftLeft', 'ShiftRight'] },
  { token: 'Super', codes: ['MetaLeft', 'MetaRight'] },
] as const

const MODIFIER_CODES: Set<string> = new Set(MODIFIERS.flatMap((m) => m.codes))

/** 这几个键单独按下去不构成快捷键，捕获时要忽略而不是报错。 */
export function isModifierOnly(code: string): boolean {
  return MODIFIER_CODES.has(code)
}

/** 没有修饰键的组合（比如单按 `A`）会吃掉全局那个字母，必须挡下来。 */
export function hasModifier(e: { ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }): boolean {
  return e.ctrlKey || e.altKey || e.shiftKey || e.metaKey
}

/** 不参与绑定的键：它们要么被系统抢走，要么按下去等于关掉自己的界面。 */
const BLOCKED = new Set(['CapsLock', 'ScrollLock', 'NumLock', 'ContextMenu'])

/** 少数键位的全局快捷键根本收不到，或收到也唤不醒窗口 —— 提前拦住比让用户试半天好。 */
const WINDOWS_HELD = new Set(['Escape'])

/** `code` → 序列化 token。字母与数字去掉前缀，其余用 W3C 的名字。 */
function tokenOf(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code)
  if (letter) return letter[1]
  const digit = /^Digit([0-9])$/.exec(code)
  if (digit) return digit[1]
  if (code === 'Backquote') return 'Backquote'
  if (BLOCKED.has(code) || WINDOWS_HELD.has(code)) return null
  return code
}

/**
 * 把一次 keydown 变成序列化好的组合键。
 * 返回 null 表示这次按键不构成一个可绑的快捷键（纯修饰键、被禁的键、没有修饰键）。
 */
export function captureCombo(e: {
  code: string
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  metaKey: boolean
}): string | null {
  if (isModifierOnly(e.code)) return null
  const token = tokenOf(e.code)
  if (!token) return null
  if (!hasModifier(e)) return null
  const pressed: Record<string, boolean> = {
    Ctrl: e.ctrlKey,
    Alt: e.altKey,
    Shift: e.shiftKey,
    Super: e.metaKey,
  }
  const mods = MODIFIERS.map((m) => m.token).filter((t) => pressed[t])
  return [...mods, token].join('+')
}

const TOKEN_LABELS: Record<string, string> = {
  Ctrl: 'Ctrl',
  Alt: 'Alt',
  Shift: 'Shift',
  Super: 'Win',
  Space: '空格',
  Enter: '回车',
  NumpadEnter: '小键盘回车',
  Tab: 'Tab',
  Backspace: '退格',
  Delete: 'Del',
  Escape: 'Esc',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
  Home: 'Home',
  End: 'End',
  Insert: 'Ins',
  PrintScreen: 'PrtSc',
  Backquote: '`',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
  IntlBackslash: '\\',
  NumpadAdd: '+（小键盘）',
  NumpadSubtract: '-（小键盘）',
  NumpadMultiply: '*（小键盘）',
  NumpadDivide: '/（小键盘）',
  NumpadDecimal: '.（小键盘）',
}

/** 存储串 → 给人看的串。认不出的 token 原样显示：宁可写得生涩，也不显示错。 */
export function displayCombo(combo: string): string {
  return combo
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => TOKEN_LABELS[p] ?? p)
    .join(' + ')
}

/** 比较两个绑定是否同一组合：token 顺序与大小写都不该算成不同。 */
export function sameCombo(a: string, b: string): boolean {
  const norm = (s: string) =>
    s
      .split('+')
      .map((p) => p.trim().toLowerCase())
      .filter(Boolean)
      .sort()
      .join('+')
  return norm(a) === norm(b)
}

/**
 * 应用内已经占用的组合键。
 * 把它绑成全局快捷键不会注册失败 —— 撞的是自己，两个动作会一起触发，所以要说一句。
 */
export const IN_APP_COMBOS: { combo: string; what: string }[] = [{ combo: 'Ctrl+K', what: '命令面板' }]

export function clashWithInApp(combo: string): string | null {
  return IN_APP_COMBOS.find((c) => sameCombo(c.combo, combo))?.what ?? null
}

/** 给「默认值」按钮用的展示串，与 Rust 侧 DEFAULT_SHOW 对齐。 */
export const DEFAULT_COMBO = 'Alt+Shift+A'
