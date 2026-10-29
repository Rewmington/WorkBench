<script setup lang="ts">
import { computed } from 'vue'

import type { HeatCell } from '../../api/types'
import { dayShort, isFirstOfMonth, monthLabel } from '../../composables/useStats'

/**
 * 完成热力图（§7 的 7×15）。回答的是「我有没有连着做」——
 * 柱状图给的是形状，看不出断在哪天，所以这张不能被替代。
 *
 * 格子是**固定 11px**，不随容器伸缩。上一版日历热力图吃过 `flex:1` + `aspect-ratio` 的亏：
 * 15 列铺满 900px 就变成 100px 大方块，整页被撑爆。
 */
const props = defineProps<{ cells: HeatCell[]; todayDay: number; streak: number; streakThrough: number | null }>()
const emit = defineEmits<{ pick: [number] }>()

const WEEK = ['一', '二', '三', '四', '五', '六', '日']
const max = computed(() => Math.max(1, ...props.cells.map((c) => c.done)))
const best = computed(() => Math.max(0, ...props.cells.map((c) => c.done)))
const activeDays = computed(() => props.cells.filter((c) => c.done > 0).length)

/** 四档色阶。最满的一档也只用 72% 的 accent —— 整片实心会抢走本页唯一的重心（§8.8.6）。 */
function level(done: number): number {
  if (done <= 0) return 0
  const r = done / max.value
  if (r <= 0.25) return 1
  if (r <= 0.5) return 2
  if (r <= 0.75) return 3
  return 4
}

/** 月份刻度：只在跨月的那一列出现，否则一行全是重复的"10 月"。 */
const months = computed(() => {
  const out: { col: number; label: string }[] = []
  props.cells.forEach((c, i) => {
    if (i % 7 === 0 && isFirstOfMonth(c.day)) out.push({ col: Math.floor(i / 7), label: monthLabel(c.day) })
  })
  return out
})

function titleOf(c: HeatCell): string {
  if (c.day > props.todayDay) return ''
  return c.done > 0 ? `${dayShort(c.day)} · 完成 ${c.done} 项` : `${dayShort(c.day)} · 没做`
}
</script>

<template>
  <div class="heat">
    <div class="wk">
      <span v-for="(w, i) in WEEK" :key="w">{{ i % 2 === 0 ? w : '' }}</span>
    </div>
    <div class="body">
      <div class="grid">
        <button
          v-for="c in cells"
          :key="c.day"
          type="button"
          class="cell"
          :data-l="level(c.done)"
          :class="{ future: c.day > todayDay, today: c.day === todayDay }"
          :title="titleOf(c)"
          :disabled="c.day > todayDay"
          @click="emit('pick', c.day)"
        ></button>
      </div>
      <div class="scale">
        <span class="lb">少</span>
        <i v-for="l in 5" :key="l" :data-l="l - 1"></i>
        <span class="lb">多</span>
      </div>
      <div class="ticks">
        <span v-for="m in months" :key="m.col" :style="{ gridColumn: m.col + 1 }">{{ m.label }}</span>
      </div>
    </div>

    <!-- 右边那一栏填的是数字而不是留白：热力图占满宽度会让格子被拉大，那就不再是热力图 -->
    <dl class="side">
      <div>
        <dt>连着做</dt>
        <dd v-if="streak > 0">
          <b>{{ streak }}</b> 天<span v-if="streakThrough" class="thru">截至 {{ dayShort(streakThrough) }}</span>
        </dd>
        <dd v-else>还没有</dd>
      </div>
      <div>
        <dt>活跃天数</dt>
        <dd><b>{{ activeDays }}</b> / 105</dd>
      </div>
      <div>
        <dt>单日最多</dt>
        <dd><b>{{ best }}</b> 项</dd>
      </div>
    </dl>
  </div>
</template>

<style scoped>
.heat {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}
.side {
  margin: 1px 0 0 auto;
  display: flex;
  flex-direction: column;
  gap: 7px;
  flex: none;
}
.side > div {
  display: flex;
  align-items: baseline;
  gap: 7px;
}
.side dt {
  font-size: 10px;
  color: var(--t3);
  width: 52px;
  flex: none;
}
.side dd {
  margin: 0;
  font-size: 11.5px;
  color: var(--t2);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.side b {
  font-size: 14px;
  font-weight: 800;
  color: var(--t1);
}
.thru {
  font-size: 10px;
  color: var(--t3);
  margin-left: 4px;
}
.wk {
  display: grid;
  grid-template-rows: repeat(7, 11px);
  gap: 3px;
  padding-top: 1px;
}
.wk span {
  font-size: 9.5px;
  line-height: 11px;
  color: var(--t3);
  width: 9px;
}
.body {
  flex: 1;
  min-width: 0;
}
.grid {
  display: grid;
  grid-template-rows: repeat(7, 11px);
  grid-auto-flow: column;
  grid-auto-columns: 11px;
  gap: 3px;
}
.cell {
  width: 11px;
  height: 11px;
  border-radius: 2.5px;
  background: var(--card-2);
  border: 1px solid var(--line);
  padding: 0;
}
.cell[data-l='1'] {
  background: color-mix(in srgb, var(--accent) 20%, var(--card-2));
  border-color: transparent;
}
.cell[data-l='2'] {
  background: color-mix(in srgb, var(--accent) 38%, var(--card-2));
  border-color: transparent;
}
.cell[data-l='3'] {
  background: color-mix(in srgb, var(--accent) 55%, var(--card-2));
  border-color: transparent;
}
.cell[data-l='4'] {
  background: color-mix(in srgb, var(--accent) 72%, var(--card-2));
  border-color: transparent;
}
.cell.future {
  visibility: hidden;
}
.cell.today {
  outline: 1.5px solid var(--blue);
  outline-offset: 0.5px;
}
.scale {
  display: flex;
  align-items: center;
  gap: 3px;
  margin-top: 8px;
}
.scale i {
  width: 11px;
  height: 11px;
  border-radius: 2.5px;
  background: var(--card-2);
  border: 1px solid var(--line);
}
.scale i[data-l='1'] {
  background: color-mix(in srgb, var(--accent) 20%, var(--card-2));
  border-color: transparent;
}
.scale i[data-l='2'] {
  background: color-mix(in srgb, var(--accent) 38%, var(--card-2));
  border-color: transparent;
}
.scale i[data-l='3'] {
  background: color-mix(in srgb, var(--accent) 55%, var(--card-2));
  border-color: transparent;
}
.scale i[data-l='4'] {
  background: color-mix(in srgb, var(--accent) 72%, var(--card-2));
  border-color: transparent;
}
.scale .lb {
  font-size: 9.5px;
  color: var(--t3);
  margin: 0 3px 0 6px;
}
.ticks {
  display: grid;
  grid-auto-flow: column;
  grid-auto-columns: 14px;
  margin-top: 4px;
}
.ticks span {
  font-size: 9.5px;
  color: var(--t3);
  white-space: nowrap;
}
</style>
