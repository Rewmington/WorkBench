<script setup lang="ts">
import { useUndo } from '../../composables/useUndo'

const { entry, trigger, dismiss } = useUndo()
</script>

<template>
  <Transition name="slide">
    <div v-if="entry" class="undobar" role="status">
      <span class="msg">{{ entry.label }}</span>
      <button type="button" class="act" @click="trigger">撤销</button>
      <button type="button" class="x" aria-label="关闭" @click="dismiss">×</button>
    </div>
  </Transition>
</template>

<style scoped>
.undobar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px 8px 12px;
  border-radius: var(--r-sm);
  background: var(--card);
  border: 1px solid var(--line-2);
  font-size: 12px;
  color: var(--t2);
  flex: none;
}
.msg {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.act {
  font-size: 11.5px;
  font-weight: 800;
  color: var(--accent);
  padding: 3px 9px;
  border-radius: 6px;
  border: 1px solid color-mix(in srgb, var(--accent) 26%, transparent);
}
.act:hover {
  background: var(--accent-soft);
}
.x {
  color: var(--t3);
  font-size: 15px;
  line-height: 1;
}
.slide-enter-active,
.slide-leave-active {
  transition:
    opacity var(--dur-2) var(--ease),
    transform var(--dur-2) var(--ease);
}
.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  transform: translateY(4px);
}
</style>
