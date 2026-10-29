<script setup lang="ts">
import { computed } from 'vue'

import type { DayCount } from '../../api/types'

/**
 * 完成时间轴（§8.8.5）：月粒度一行日点 + 今天高亮 + 底部进度条。
 *
 * 数据不足 60 天时**不画任何趋势线**，只画点 —— 与 §7「折线趋势 ≥ 60 天才允许显示」
 * 是同一条诚实条款，而且门槛写在明面上，不藏进 tooltip。
 *
 * 本月的日列表由父级算：组件里不许出现 `new Date()`，时间只有一个来源（useClock 的 1 秒 tick）。
 */
const props = defineProps<{
  /** 本月的本地日整数 yyyymmdd，按顺序 */
  days: number[]
  /** 今天的 yyyymmdd */
  today: number
  timeline: DayCount[]
  historyDays: number
  done: number
  open: number
}>()
const emit = defineEmits<{ pick: [day: number] }>()

const byDay = computed(() => new Map(props.timeline.map((d) => [d.day, d.done])))
const label = computed(() => {
  const first = String(props.days[0] ?? props.today)
  return `${first.slice(0, 4)} 年 ${Number(first.slice(4, 6))} 月`
})

/** 进度条分母是「本月已完成 + 本月到期还没做」，不引入用户从没设过的目标数。 */
const pct = computed(() => {
  const total = props.done + props.open
  return total === 0 ? 0 : Math.round((props.done / total) * 100)
})
</script>

<template>
  <div class="y-time">
    <div class="y-timeh">
      <span class="t">完成时间轴</span>
      <span class="y-demo">{{ label }}</span>
      <span class="m">
        本月清掉 <b>{{ done }}</b> · 应清 {{ done + open }}
        <template v-if="historyDays < 60"> · 记了 {{ historyDays }} 天，不足 60 天不画趋势线</template>
      </span>
    </div>

    <div v-if="days.length" class="y-days scroller">
      <button
        v-for="d in days"
        :key="d"
        type="button"
        class="d"
        :class="{ now: d === today, dim: !((byDay.get(d) ?? 0) > 0) }"
        :title="`${String(d).slice(4, 6)}/${String(d).slice(6, 8)} 完成 ${byDay.get(d) ?? 0} 项`"
        @click="emit('pick', d)"
      >
        {{ String(d).slice(6, 8) }}
      </button>
    </div>
    <p v-else class="none">这个月还没有记录。</p>

    <div class="y-track"><i :style="{ width: pct + '%' }"></i></div>
  </div>
</template>

<style scoped>
.y-time {
  margin-top: 12px;
  border: 1px solid var(--line);
  border-radius: var(--r);
  background: var(--card);
  padding: 11px 12px;
}
.y-timeh {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-bottom: 9px;
}
.y-timeh .t {
  font-size: 12.5px;
  font-weight: 800;
}
.y-timeh .m {
  font-size: 11.5px;
  color: var(--t3);
  margin-left: auto;
}
.y-timeh .m b {
  color: var(--t1);
  font-variant-numeric: tabular-nums;
}
.y-days {
  display: flex;
  gap: 4px;
  align-items: center;
  overflow-x: auto;
}
.y-days .d {
  font-size: 10px;
  color: var(--t3);
  width: 21px;
  flex: none;
  text-align: center;
  font-variant-numeric: tabular-nums;
  padding: 2px 0;
  border-radius: 50%;
}
.y-days .d:hover {
  color: var(--t1);
}
.y-days .d.now {
  color: var(--accent-ink);
  background: var(--accent);
  font-weight: 800;
}
.y-days .d.dim {
  opacity: 0.38;
}
.y-track {
  height: 5px;
  border-radius: 3px;
  background: rgba(128, 140, 135, 0.2);
  margin-top: 9px;
  overflow: hidden;
}
.y-track i {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, var(--accent-2), var(--accent));
  transition: width 160ms ease-out;
}
.none {
  font-size: 11.5px;
  color: var(--t3);
  margin: 0 0 4px;
}
</style>
