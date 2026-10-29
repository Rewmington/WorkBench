<script setup lang="ts">
import { ChevronDown, EyeOff } from 'lucide-vue-next'
import { ref } from 'vue'

/**
 * 「进行」视图里部件的统一外壳：可折叠、可隐藏、可排序。
 *
 * 只给「显示/隐藏与排序」，不给自由拖拽调宽高 —— PLAN §4 明确划的边界：
 * 一旦开了自由布局，就得存坐标、处理重叠与断点回退，而个人工作台没人要这个。
 */
withDefaults(
  defineProps<{
    title: string
    count?: number | string
    first?: boolean
    last?: boolean
  }>(),
  { count: undefined, first: false, last: false },
)
const emit = defineEmits<{ up: []; down: []; hide: [] }>()
const open = ref(true)
</script>

<template>
  <section class="w" :class="{ closed: !open }">
    <header class="h">
      <button type="button" class="tw" :aria-expanded="open" @click="open = !open">
        <ChevronDown :size="13" class="chev" :class="{ up: !open }" />
        <span class="t">{{ title }}</span>
        <span v-if="count !== undefined" class="c">{{ count }}</span>
      </button>
      <span class="sp"></span>
      <button type="button" class="ib" :disabled="first" title="上移" @click="emit('up')">↑</button>
      <button type="button" class="ib" :disabled="last" title="下移" @click="emit('down')">↓</button>
      <button type="button" class="ib" title="隐藏这个部件" @click="emit('hide')">
        <EyeOff :size="12" />
      </button>
      <slot name="head" />
    </header>
    <div v-show="open" class="b">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.w {
  border: 1px solid var(--line);
  border-radius: var(--r);
  background: var(--card);
  overflow: hidden;
}
.w.closed .b {
  display: none;
}
.h {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px 10px;
}
.tw {
  display: flex;
  align-items: center;
  gap: 7px;
  font: inherit;
  color: inherit;
  padding: 2px 4px;
}
.tw .t {
  font-size: 12.5px;
  font-weight: 800;
}
.tw .c {
  font-size: 11px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.chev {
  color: var(--t3);
  transition: transform var(--dur-1) var(--ease);
}
.chev.up {
  transform: rotate(-90deg);
}
.sp {
  flex: 1;
}
.ib {
  width: 22px;
  height: 22px;
  border-radius: 6px;
  display: grid;
  place-items: center;
  color: var(--t3);
  font-size: 11px;
}
.ib:hover:not(:disabled) {
  background: var(--card-2);
  color: var(--t1);
}
.ib:disabled {
  opacity: 0.3;
}
.b {
  padding: 2px 11px 11px;
}
/* 折叠态：整个部件收成一行（§8.3.3），标题旁留个提示说明它没消失 */
.w.closed .h::after {
  content: '已折叠';
  font-size: 10.5px;
  color: var(--t3);
  margin-left: 2px;
}
</style>
