<script setup lang="ts">
import type { Todo } from '../../api/types'

/**
 * 卡片环的降级形态（§8.8.1）：面数 < 3 时不画环 —— 两面的「环」只是两张背对背的卡，
 * 转起来没有任何信息，还占掉 268px 的高度。退化成横向 scroll-snap 列表。
 */
defineProps<{ cards: Todo[] }>()
defineEmits<{ pick: [index: number] }>()
</script>

<template>
  <div class="y-strip">
    <button
      v-for="(c, i) in cards"
      :key="c.id"
      type="button"
      class="y-card"
      @click="$emit('pick', i)"
    >
      <span class="id">TODO-{{ c.id.slice(-4).toUpperCase() }}</span>
      <span class="ti">{{ c.title }}</span>
      <span class="mt">{{ c.zone }}</span>
    </button>
  </div>
</template>

<style scoped>
.y-strip {
  display: flex;
  gap: 10px;
  overflow-x: auto;
  scroll-snap-type: x mandatory;
  padding: 11px;
  border: 1px solid var(--line);
  border-radius: var(--r);
  background: linear-gradient(180deg, var(--card), var(--card-2));
}
.y-card {
  scroll-snap-align: start;
  flex: none;
  width: 168px;
  border-radius: 11px;
  padding: 11px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  text-align: left;
  background: linear-gradient(165deg, #f7faf8, #cfd8d3);
  color: #16211c;
}
.y-card .id {
  font: 700 9px/1 ui-monospace, Consolas, monospace;
  letter-spacing: 0.1em;
  opacity: 0.55;
}
.y-card .ti {
  font-size: 12px;
  font-weight: 800;
  line-height: 1.35;
}
.y-card .mt {
  font-size: 10px;
  opacity: 0.6;
  margin-top: auto;
}
</style>
