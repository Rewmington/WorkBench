<script setup lang="ts">
import type { ThemeAccent, ThemeMode, ThemePreset } from '../api/types'
import Segmented from '../components/ui/Segmented.vue'
import { useTheme } from '../composables/useTheme'

/**
 * 设置页 v1 只放**已经在生效**的开关：主题三轴、降低动效、数据位置说明。
 *
 * 不放没实现的东西（自动更新、扩展市场这些 PLAN §4 明确不做），
 * 也不放「同步」—— 一台电脑一个写端，v1 没有同步可设（PLAN §10）。
 */
const theme = useTheme()

const MODES: { value: ThemeMode; label: string }[] = [
  { value: 'auto', label: '跟随系统' },
  { value: 'dark', label: '暗色' },
  { value: 'light', label: '亮色' },
]
const PRESETS: { value: ThemePreset; label: string }[] = [
  { value: 'graphite', label: '石墨' },
  { value: 'moss', label: '苔绿' },
  { value: 'ink', label: '墨黑' },
]
const ACCENTS: { value: ThemeAccent; label: string }[] = [
  { value: 'green', label: '绿' },
  { value: 'blue', label: '蓝' },
  { value: 'amber', label: '琥珀' },
]
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>外观与动效 <span class="sub">三轴独立 · 当前解析为 {{ theme.resolved.value }}</span></h2>
    </div>

    <div class="blk">
      <h3>外观</h3>
      <div class="row">
        <span class="k">模式</span>
        <Segmented :options="MODES" :model-value="theme.mode.value" @update:model-value="theme.mode.value = $event" />
      </div>
      <div class="row">
        <span class="k">明度预设</span>
        <Segmented :options="PRESETS" :model-value="theme.preset.value" @update:model-value="theme.preset.value = $event" />
      </div>
      <div class="row">
        <span class="k">强调色</span>
        <Segmented :options="ACCENTS" :model-value="theme.accent.value" @update:model-value="theme.accent.value = $event" />
      </div>
      <p class="note">
        三轴各自独立：预设改的是底与卡的明度阶梯，强调色只改 <code>--accent</code> 一族。
        亮色下强调色自动降到 <code>#059669</code>，绿底白字那种对比度不够的写法不会出现。
      </p>
    </div>

    <div class="blk">
      <h3>动效</h3>
      <label class="row chk">
        <input v-model="theme.reducedMotion.value" type="checkbox" />
        <span class="k">降低动效</span>
        <span class="v">卡片环立刻停止巡航（不是只暂停 CSS 动画）</span>
      </label>
    </div>

    <div class="blk">
      <h3>数据位置</h3>
      <p class="note">
        库文件、图标缓存与备份快照的绝对路径原样写在备份页与侧栏底部 ——
        「数据只在这台机器上」这句话，要能被看见才算数。
      </p>
    </div>
  </section>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
h2 {
  font-size: 13.5px;
  font-weight: 800;
}
.sub {
  font-size: 11px;
  font-weight: 500;
  color: var(--t3);
}
.blk {
  display: flex;
  flex-direction: column;
  gap: 9px;
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: var(--r-sm);
  padding: 11px 12px;
}
.blk h3 {
  margin: 0;
  font-size: 12px;
  font-weight: 750;
  color: var(--t2);
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.row .k {
  font-size: 11.5px;
  color: var(--t3);
  width: 74px;
  flex: none;
}
.row .v {
  font-size: 11px;
  color: var(--t3);
}
.chk {
  cursor: pointer;
}
.chk input {
  accent-color: var(--accent);
}
.note {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.65;
  color: var(--t3);
}
.note code {
  font: 500 11px/1.5 ui-monospace, Consolas, monospace;
  color: var(--t2);
}
</style>
