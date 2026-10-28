<script setup lang="ts" generic="T extends string | number">
defineProps<{
  options: { value: T; label: string; badge?: number }[]
  modelValue: T
}>()
const emit = defineEmits<{ 'update:modelValue': [T] }>()
</script>

<template>
  <div class="seg" role="tablist">
    <button
      v-for="opt in options"
      :key="String(opt.value)"
      type="button"
      role="tab"
      class="s"
      :class="{ on: opt.value === modelValue }"
      :aria-selected="opt.value === modelValue"
      @click="emit('update:modelValue', opt.value)"
    >
      {{ opt.label }}
      <span v-if="opt.badge" class="bg">{{ opt.badge }}</span>
    </button>
  </div>
</template>

<style scoped>
/* 候选一的 .y-seg：轨道 + 实心选中，选中同时改字重（状态不只靠颜色） */
.seg {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border-radius: 9px;
  background: rgba(128, 140, 135, 0.14);
}
.s {
  padding: 4px 10px;
  border-radius: 7px;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--t2);
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.s.on {
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 800;
}
.bg {
  font-size: 10px;
  opacity: 0.8;
  font-variant-numeric: tabular-nums;
}
</style>
