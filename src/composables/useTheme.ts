import { computed, ref, watch } from 'vue'

import { api } from '../api'
import type { ThemeAccent, ThemeMode, ThemePreset } from '../api/types'

const KEYS = {
  mode: 'theme.mode',
  preset: 'theme.preset',
  accent: 'theme.accent',
  reducedMotion: 'ui.reducedMotion',
} as const

// 冻结的是暗色那一版（§8.5），所以默认暗色；auto 与 light 是同一根轴上的可选项
const mode = ref<ThemeMode>('dark')
const preset = ref<ThemePreset>('graphite')
const accent = ref<ThemeAccent>('green')
const reducedMotion = ref(false)

const systemLight = /* @__PURE__ */ matchMedia('(prefers-color-scheme: light)')
const resolved = computed<'dark' | 'light'>(() =>
  mode.value === 'auto' ? (systemLight.matches ? 'light' : 'dark') : mode.value,
)

// auto 跟随系统：WebView2 读的是 Windows 应用主题设置，不需要碰注册表
systemLight.addEventListener('change', () => {
  if (mode.value === 'auto') apply()
})

function apply() {
  const el = document.documentElement
  el.dataset.mode = resolved.value
  el.dataset.preset = preset.value
  el.dataset.accent = accent.value
  el.dataset.motion = reducedMotion.value ? 'reduced' : 'full'
}

let loaded = false

async function load() {
  if (loaded) return
  loaded = true
  const { values } = await api.settingsAll()
  mode.value = (values[KEYS.mode] as ThemeMode) ?? 'dark'
  preset.value = (values[KEYS.preset] as ThemePreset) ?? 'graphite'
  accent.value = (values[KEYS.accent] as ThemeAccent) ?? 'green'
  reducedMotion.value = (values[KEYS.reducedMotion] as boolean) ?? false
  apply()
}

function persist<K extends keyof typeof KEYS>(key: K, value: unknown) {
  void api.settingsSet(KEYS[key], value)
}

export function useTheme() {
  watch(mode, (v) => persist('mode', v))
  watch(preset, (v) => persist('preset', v))
  watch(accent, (v) => persist('accent', v))
  watch(reducedMotion, (v) => persist('reducedMotion', v))

  return {
    mode,
    preset,
    accent,
    reducedMotion,
    resolved,
    load,
    toggleMode() {
      mode.value = resolved.value === 'dark' ? 'light' : 'dark'
    },
  }
}
