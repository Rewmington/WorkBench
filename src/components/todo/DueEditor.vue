<script setup lang="ts">
import { computed, ref } from 'vue'
import { CalendarDays, ChevronLeft, ChevronRight } from 'lucide-vue-next'

import { dueLabel } from '../../lib/reltime'

/**
 * 自绘日历弹层，不用原生 <input type=date>：原生控件不吃设计令牌，
 * 切主题时会出现一个跟整屏无关的白盒子（§8.5「不受令牌控制的就是债」）。
 */
const props = defineProps<{ modelValue: number | null; now: number }>()
const emit = defineEmits<{ 'update:modelValue': [number | null] }>()

const open = ref(false)
const view = ref(new Date(props.modelValue ?? props.now))
/** 选日期时保留一个钟点，否则每条截止都变成 00:00，等于「昨天就逾期了」 */
const hour = ref(props.modelValue ? new Date(props.modelValue).getHours() : 18)

const WEEK = ['一', '二', '三', '四', '五', '六', '日']
const dayMs = 86_400_000

const cells = computed(() => {
  const first = new Date(view.value.getFullYear(), view.value.getMonth(), 1)
  // 周一开头：(getDay() + 6) % 7 把周日挪到最后
  const lead = (first.getDay() + 6) % 7
  const start = first.getTime() - lead * dayMs
  return Array.from({ length: 42 }, (_, i) => {
    const d = new Date(start + i * dayMs)
    return {
      time: new Date(d.getFullYear(), d.getMonth(), d.getDate(), hour.value).getTime(),
      date: d.getDate(),
      inMonth: d.getMonth() === first.getMonth(),
      today: new Date(props.now).toDateString() === d.toDateString(),
      selected: props.modelValue != null && new Date(props.modelValue).toDateString() === d.toDateString(),
    }
  })
})

const monthLabel = computed(() => `${view.value.getFullYear()} 年 ${view.value.getMonth() + 1} 月`)
const label = computed(() => (props.modelValue ? dueLabel(props.now, props.modelValue) : '设截止'))

function shift(delta: number) {
  view.value = new Date(view.value.getFullYear(), view.value.getMonth() + delta, 1)
}

function pick(time: number) {
  emit('update:modelValue', time)
  open.value = false
}

function clear() {
  emit('update:modelValue', null)
  open.value = false
}
</script>

<template>
  <div class="wrap">
    <button type="button" class="due" :class="{ set: modelValue != null }" @click="open = !open">
      <CalendarDays :size="12" />
      {{ label }}
    </button>

    <div v-if="open" class="pop" @mouseleave="open = false">
      <div class="head">
        <button type="button" class="nav" aria-label="上个月" @click="shift(-1)">
          <ChevronLeft :size="14" />
        </button>
        <span class="ml">{{ monthLabel }}</span>
        <button type="button" class="nav" aria-label="下个月" @click="shift(1)">
          <ChevronRight :size="14" />
        </button>
      </div>

      <div class="grid wk">
        <span v-for="w in WEEK" :key="w">{{ w }}</span>
      </div>
      <div class="grid">
        <button
          v-for="cell in cells"
          :key="cell.time"
          type="button"
          class="d"
          :class="{ dim: !cell.inMonth, today: cell.today, sel: cell.selected }"
          @click="pick(cell.time)"
        >
          {{ cell.date }}
        </button>
      </div>

      <div class="foot">
        <div class="hrs">
          <button v-for="h in [9, 14, 20]" :key="h" type="button" :class="{ on: hour === h }" @click="hour = h">
            {{ h === 9 ? '上午' : h === 14 ? '下午' : '晚上' }}
          </button>
        </div>
        <button v-if="modelValue != null" type="button" class="clr" @click="clear">清除</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  position: relative;
}
.due {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10.5px;
  font-weight: 700;
  padding: 2.5px 7px;
  border-radius: 5px;
  border: 1px solid var(--line);
  color: var(--t3);
  background: var(--card-2);
}
.due.set {
  color: var(--t2);
  border-color: var(--line-2);
}
.pop {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 50;
  width: 246px;
  padding: 10px;
  background: var(--panel);
  border: 1px solid var(--line-2);
  border-radius: 12px;
  box-shadow: 0 18px 40px rgba(0, 0, 0, 0.42);
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}
.ml {
  font-size: 12px;
  font-weight: 750;
}
.nav {
  width: 22px;
  height: 22px;
  border-radius: 6px;
  color: var(--t3);
  display: grid;
  place-items: center;
}
.nav:hover {
  background: var(--card);
  color: var(--t1);
}
.grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 2px;
}
.wk span {
  font-size: 10px;
  color: var(--t3);
  text-align: center;
  padding-bottom: 3px;
}
.d {
  height: 26px;
  border-radius: 6px;
  font-size: 11.5px;
  color: var(--t1);
  font-variant-numeric: tabular-nums;
}
.d:hover {
  background: var(--card);
}
.d.dim {
  color: var(--t3);
  opacity: 0.5;
}
.d.today {
  box-shadow: inset 0 0 0 1px var(--blue);
}
.d.sel {
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 800;
}
.foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 9px;
  padding-top: 8px;
  border-top: 1px solid var(--line);
}
.hrs {
  display: flex;
  gap: 4px;
}
.hrs button {
  font-size: 10.5px;
  padding: 3px 7px;
  border-radius: 6px;
  border: 1px solid var(--line);
  color: var(--t2);
}
.hrs button.on {
  border-color: var(--line-2);
  background: var(--card);
  color: var(--t1);
  font-weight: 750;
}
.clr {
  font-size: 10.5px;
  color: var(--t3);
}
.clr:hover {
  color: var(--red);
}
</style>
