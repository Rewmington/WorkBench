/**
 * jsdom 不实现 matchMedia，而 `useTheme` 在模块加载时就订阅 prefers-color-scheme
 * （任何 import 到 useTheme 的组件都会连带它进来）。不补这一刀，所有组件级测试
 * 都在 import 阶段就炸，测不到任何东西。
 */
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }),
})
