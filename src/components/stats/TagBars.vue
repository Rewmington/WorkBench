<script setup lang="ts">
import { computed } from 'vue'

import type { TagBar } from '../../api/types'

/**
 * 分类投入横向条形（§7）。横向比长度准，中文标签还能直接写在条上。
 *
 * 标题不叫「时间花在哪」：这个应用里没有记录每条待办花了多久，
 * 拿条数冒充时间就是 §2 原则 5 禁止的那类假指标。所以这张图回答的是
 * 「哪类事情占的条数最多」。
 */
const props = defineProps<{ tags: TagBar[]; total: number }>()

const max = computed(() => Math.max(1, ...props.tags.map((t) => t.done)))
const shown = computed(() => props.tags.reduce((a, t) => a + t.done, 0))
</script>

<template>
  <div class="rows">
    <div v-for="t in tags" :key="t.tag" class="row">
      <span class="lb">{{ t.tag }}</span>
      <span class="rail">
        <i class="fill" :style="{ width: Math.round((t.done / max) * 100) + '%' }"></i>
      </span>
      <span class="n">{{ t.done }}</span>
    </div>
  </div>
  <p class="foot">
    近 90 天完成 {{ total }} 项；一条待办可以挂多个标签，所以这些条加起来（{{ shown }}）会超过总数。
  </p>
</template>

<style scoped>
.rows {
  display: flex;
  flex-direction: column;
  gap: 7px;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.lb {
  width: 62px;
  flex: none;
  font-size: 11px;
  color: var(--t2);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.rail {
  flex: 1;
  min-width: 0;
  height: 10px;
  border-radius: 3px;
  background: var(--card-2);
  overflow: hidden;
}
.fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--accent-2);
}
.n {
  width: 26px;
  flex: none;
  font-size: 11px;
  font-weight: 750;
  color: var(--t1);
  font-variant-numeric: tabular-nums;
}
.foot {
  margin: 9px 0 0;
  font-size: 10.5px;
  line-height: 1.6;
  color: var(--t3);
}
</style>
