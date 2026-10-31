<script setup lang="ts">
import { computed } from 'vue'

/**
 * 用药三环：今日服药 / 本周依从 / 连续打卡。形状照药箱 App 今天屏那张三色环，
 * 数字全部由后端按药箱自己那段算法（`models.dart` 的 `computeStats`）算好后送来 ——
 * 同一个「本周依从」在两个程序里得是同一个数，所以这里不复述公式，只做显示。
 *
 * 三环都是「占满 = 好」的比例，因此没有目标线也没有坐标轴：这一屏没有「该吃几粒」
 * 这种我们编出来的指标，只有它自己记下的格子。
 */
const props = defineProps<{
  todayTaken: number
  todayOwed: number
  weekRate: number
  /** 近 7 天压根没有安排：那不是 0% 依从，得说「没有安排」而不是画一个空环骗人 */
  hasWeekPlan: boolean
  streakDays: number
}>()

const C = 48

const rings = computed(() => [
  {
    cls: 'today',
    r: 42,
    label: '今日服药',
    value: props.todayOwed > 0 ? `${props.todayTaken} / ${props.todayOwed}` : '没有安排',
    frac: props.todayOwed > 0 ? props.todayTaken / props.todayOwed : 0,
  },
  {
    cls: 'week',
    r: 32,
    label: '本周依从',
    value: props.hasWeekPlan ? `${Math.round(props.weekRate * 100)}%` : '没有安排',
    frac: props.hasWeekPlan ? props.weekRate : 0,
  },
  {
    cls: 'streak',
    r: 22,
    label: '连续打卡',
    value: `${props.streakDays} 天`,
    frac: Math.min(props.streakDays / 7, 1),
  },
])

function dash(r: number, frac: number) {
  const c = 2 * Math.PI * r
  return `${(Math.min(Math.max(frac, 0), 1) * c).toFixed(2)} ${c.toFixed(2)}`
}
</script>

<template>
  <div class="wrap">
    <svg viewBox="0 0 96 96" width="96" height="96" aria-hidden="true" class="rings">
      <template v-for="g in rings" :key="g.cls">
        <circle class="track" :class="g.cls" :cx="C" :cy="C" :r="g.r" fill="none" />
        <circle
          v-if="g.frac > 0"
          class="arc"
          :class="g.cls"
          :cx="C"
          :cy="C"
          :r="g.r"
          fill="none"
          stroke-linecap="round"
          :stroke-dasharray="dash(g.r, g.frac)"
          :transform="`rotate(-90 ${C} ${C})`"
        />
      </template>
    </svg>
    <ul class="legend">
      <li v-for="g in rings" :key="g.cls" class="row">
        <i class="dot" :class="g.cls"></i>
        <span class="lb">{{ g.label }}</span>
        <span class="n">{{ g.value }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  align-items: center;
  gap: 14px;
}
.rings {
  flex: none;
}
.track {
  stroke: var(--card-2);
  stroke-width: 8;
}
.arc {
  stroke-width: 8;
}
.today {
  stroke: var(--accent);
}
.week {
  stroke: var(--blue);
}
.streak {
  stroke: var(--amber);
}
.legend {
  flex: 1;
  min-width: 0;
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 7px;
}
.row {
  display: flex;
  align-items: center;
  gap: 7px;
}
.dot {
  width: 8px;
  height: 8px;
  flex: none;
  border-radius: 50%;
}
.dot.today {
  background: var(--accent);
}
.dot.week {
  background: var(--blue);
}
.dot.streak {
  background: var(--amber);
}
.lb {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--t2);
}
.n {
  font-size: 12px;
  font-weight: 750;
  color: var(--t1);
  font-variant-numeric: tabular-nums;
}
</style>
