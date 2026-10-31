<script setup lang="ts">
import { computed } from 'vue'

import type { MedicineTodayItem } from '../../api/types'

/**
 * 各药还能吃几天的横条，顺序就是传进来的顺序（后端已按天数升序，快没的在上面）。
 *
 * 条长折算到这一列里最久的那支，不是绝对刻度 —— 所以图上不标「多少天为一格」，
 * 天数一律写在右边的数字里。条只回答一件事：谁跟谁比更紧。
 */
const props = defineProps<{ items: MedicineTodayItem[] }>()

/** 没设用量算不出天数；它不该被画成「0 天」，也不该把刻度顶到天上去 */
const scale = computed(() => Math.max(1, ...props.items.map((m) => m.daysLeft ?? 0)))

function pct(m: MedicineTodayItem) {
  if (m.isOut || m.daysLeft == null) return '0%'
  return Math.max(3, Math.round((m.daysLeft / scale.value) * 100)) + '%'
}

function tone(m: MedicineTodayItem) {
  if (m.isOut || m.isLow) return 'risk'
  return m.daysLeft == null ? 'unknown' : 'ok'
}

function label(m: MedicineTodayItem) {
  if (m.isOut) return '已用完'
  return m.daysLeft == null ? '未设用量' : `剩 ${m.daysLeft} 天`
}
</script>

<template>
  <div class="wrap">
    <div class="bars">
      <div v-for="m in items" :key="m.id" class="row">
        <span class="nm">{{ m.name }}</span>
        <span class="rail" :title="`${m.name}：${label(m)}`">
          <i class="fill" :class="tone(m)" :style="{ width: pct(m) }"></i>
        </span>
        <span class="n" :class="tone(m)">{{ label(m) }}</span>
      </div>
    </div>
    <p class="foot">条长按这一列里最久的那支折算，看的是谁更紧，不是绝对天数。</p>
  </div>
</template>

<style scoped>
.wrap {
  min-width: 0;
}
.bars {
  display: flex;
  flex-direction: column;
  gap: 7px;
  min-width: 0;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.nm {
  width: 88px;
  flex: none;
  font-size: 11.5px;
  color: var(--t2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.rail {
  flex: 1;
  min-width: 0;
  height: 9px;
  border-radius: 3px;
  background: var(--card-2);
  overflow: hidden;
}
.fill {
  display: block;
  height: 100%;
  border-radius: 3px;
}
/* 偏低与用完同一种红：这一屏上「该去补了」只有一种语气，分两档颜色是让人猜 */
.fill.risk {
  background: var(--red);
}
.fill.ok {
  background: var(--accent);
}
.fill.unknown {
  background: var(--line);
}
.n {
  width: 74px;
  flex: none;
  text-align: right;
  font-size: 11px;
  font-weight: 700;
  color: var(--t1);
  font-variant-numeric: tabular-nums;
}
.n.risk {
  color: var(--red);
}
.n.unknown {
  color: var(--t3);
  font-weight: 500;
}
.foot {
  margin: 8px 0 0;
  font-size: 10.5px;
  line-height: 1.6;
  color: var(--t3);
}
</style>
