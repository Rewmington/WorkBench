<script setup lang="ts">
import { computed } from 'vue'

import type { DayBar } from '../../api/types'
import { dayShort } from '../../composables/useStats'

/**
 * 每日完成柱状图（§7）。回答「哪几天在硬扛」——
 * 所以**必须分档**：全是同一种绿，就只是在数数，看不出哪天在补昨天的账。
 *
 * 三档而不是两档：没设截止的待办既不算按时也不算补做，硬塞进任何一档都是谎。
 */
const props = defineProps<{ bars: DayBar[]; todayDay: number }>()
const emit = defineEmits<{ pick: [number] }>()

const totals = computed(() => props.bars.map((b) => b.onTime + b.late + b.noDue))
const max = computed(() => Math.max(1, ...totals.value))
const sum = computed(() => ({
  onTime: props.bars.reduce((a, b) => a + b.onTime, 0),
  late: props.bars.reduce((a, b) => a + b.late, 0),
  noDue: props.bars.reduce((a, b) => a + b.noDue, 0),
}))

function heightOf(total: number): string {
  return total > 0 ? `${Math.round((total / max.value) * 100)}%` : '0px'
}

/** 刻度只标每 5 天与最后一根：30 个日期全标等于没标。 */
function showLabel(i: number): boolean {
  return i % 5 === 0 || i === props.bars.length - 1
}
</script>

<template>
  <div class="wrap">
    <div class="plot">
      <div
        v-for="(b, i) in bars"
        :key="b.day"
        class="col"
        :title="`${dayShort(b.day)} · 按时 ${b.onTime} / 补做 ${b.late} / 无截止 ${b.noDue}`"
        @click="emit('pick', b.day)"
      >
        <div class="stack" :style="{ height: heightOf(totals[i]) }">
          <i v-if="b.late" class="s late" :style="{ flexGrow: b.late }"></i>
          <i v-if="b.noDue" class="s nodue" :style="{ flexGrow: b.noDue }"></i>
          <i v-if="b.onTime" class="s ontime" :style="{ flexGrow: b.onTime }"></i>
        </div>
        <span class="xl">{{ showLabel(i) ? dayShort(b.day) : '' }}</span>
      </div>
    </div>
    <div class="legend">
      <span><i class="s ontime"></i>按时 {{ sum.onTime }}</span>
      <span><i class="s late"></i>补做 {{ sum.late }}</span>
      <span><i class="s nodue"></i>无截止 {{ sum.noDue }}</span>
      <span class="peak">最高一天 {{ max }} 项</span>
    </div>
  </div>
</template>

<style scoped>
.plot {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  height: 116px;
}
.col {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  height: 100%;
  cursor: pointer;
}
.stack {
  display: flex;
  flex-direction: column;
  width: 100%;
  border-radius: 2px 2px 0 0;
  overflow: hidden;
}
.s {
  display: block;
  width: 100%;
  min-height: 2px;
}
.ontime {
  background: var(--accent-2);
}
.late {
  background: var(--amber);
}
.nodue {
  /* 暗底上 40% 的 t3 几乎看不见，会被读成"柱子缺了一截"。它是真实的一档，得看得见。 */
  background: color-mix(in srgb, var(--t2) 42%, transparent);
}
.xl {
  font-size: 9px;
  color: var(--t3);
  text-align: center;
  height: 12px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.legend {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 4px;
  font-size: 10.5px;
  color: var(--t3);
}
.legend span {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.legend .s {
  width: 8px;
  height: 8px;
  border-radius: 2px;
}
.legend .peak {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
}
</style>
