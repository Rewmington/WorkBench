<script setup lang="ts">
import { computed } from 'vue'

import type { HourBar } from '../../api/types'

/**
 * 建待办的时段直方图（§7 里唯一「看数据量」的那张）。
 *
 * 洞察是「晚上建、早上还」，但不足门槛天数画出来的只是这几天的偶然，
 * 所以不够就只说门槛，不画一根歪歪扭扭的柱装样子。门槛由后端一起送来（而不是
 * 在这里抄一份常量），否则改了 Rust 侧的数、这句话还在说旧门槛。
 *
 * 用蓝色而不是绿的系：这张数的是**新建**，与其余四张数「完成」不是一回事。
 */
const props = defineProps<{
  hours: HourBar[]
  enough: boolean
  historyDays: number
  minDays: number
}>()

const max = computed(() => Math.max(1, ...props.hours.map((h) => h.created)))
const busiest = computed(() => {
  let best = props.hours[0]
  for (const h of props.hours) if (h.created > best.created) best = h
  return best
})

function heightOf(n: number): string {
  return n > 0 ? `${Math.max(3, Math.round((n / max.value) * 100))}%` : '0px'
}
</script>

<template>
  <div v-if="!enough" class="gate">
    需 ≥ {{ minDays }} 天，当前 {{ historyDays }} 天 —— 天数不够时这张图只是噪声
  </div>
  <template v-else>
    <div class="plot">
      <div
        v-for="h in hours"
        :key="h.hour"
        class="col"
        :class="{ top: h.hour === busiest.hour && h.created > 0 }"
        :title="`${h.hour}:00 前后 · 建了 ${h.created} 条`"
      >
        <i class="bar" :style="{ height: heightOf(h.created) }"></i>
        <span class="xl">{{ h.hour % 6 === 0 ? h.hour : '' }}</span>
      </div>
    </div>
    <p class="foot">
      建待办最多的是 <b>{{ busiest.hour }}:00</b> 那一个钟头（{{ busiest.created }} 条）·
      共 {{ hours.reduce((a, h) => a + h.created, 0) }} 条
    </p>
  </template>
</template>

<style scoped>
.gate {
  display: flex;
  align-items: center;
  gap: 6px;
  border: 1px dashed var(--line-2);
  border-radius: var(--r-xs);
  padding: 9px 11px;
  font-size: 11px;
  color: var(--t3);
}
.plot {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  height: 92px;
}
.col {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  height: 100%;
}
.bar {
  display: block;
  width: 100%;
  border-radius: 2px 2px 0 0;
  background: color-mix(in srgb, var(--blue) 45%, transparent);
}
.col.top .bar {
  background: var(--blue);
}
.xl {
  font-size: 9px;
  color: var(--t3);
  text-align: center;
  height: 12px;
  font-variant-numeric: tabular-nums;
}
.foot {
  margin: 6px 0 0;
  font-size: 10.5px;
  color: var(--t3);
}
.foot b {
  color: var(--t1);
  font-weight: 800;
  font-variant-numeric: tabular-nums;
}
</style>
