<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'

import type { Todo } from '../api/types'
import Segmented from '../components/ui/Segmented.vue'
import { useClock } from '../composables/useClock'
import { useShell } from '../composables/useShell'
import { useTodos } from '../composables/useTodos'

/**
 * 待办的月 / 周视图（PLAN §4）。看的是「哪几天堆了东西」，
 * 所以格子放条目摘要而不是数字 —— 数字回答不了「那天我到底在干什么」。
 */
const shell = useShell()
const { now } = useClock()
const { all, refresh } = useTodos()

type Mode = 'month' | 'week'
const mode = ref<Mode>('month')
const anchor = ref(startOfDay(now.value))

const DAY = 86_400_000
const WEEK = ['一', '二', '三', '四', '五', '六', '日']

function startOfDay(ms: number) {
  const d = new Date(ms)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

function sameDay(a: number, b: number) {
  return startOfDay(a) === startOfDay(b)
}

const todos = computed<Todo[]>(() => all.value.filter((t) => t.dueAt && !t.completed))

onMounted(refresh)

const days = computed(() => {
  const base = new Date(anchor.value)
  if (mode.value === 'week') {
    const lead = (base.getDay() + 6) % 7
    const first = startOfDay(base.getTime() - lead * DAY)
    return Array.from({ length: 7 }, (_, i) => first + i * DAY)
  }
  const first = new Date(base.getFullYear(), base.getMonth(), 1).getTime()
  const lead = (new Date(first).getDay() + 6) % 7
  const start = startOfDay(first - lead * DAY)
  return Array.from({ length: 42 }, (_, i) => start + i * DAY)
})

const byDay = computed(() => {
  const map = new Map<number, Todo[]>()
  for (const t of days.value) map.set(t, [])
  for (const t of todos.value) {
    if (t.dueAt == null) continue
    const key = startOfDay(t.dueAt)
    if (!map.has(key)) continue
    map.get(key)!.push(t)
  }
  return map
})

const title = computed(() => {
  const d = new Date(anchor.value)
  return mode.value === 'month'
    ? `${d.getFullYear()} 年 ${d.getMonth() + 1} 月`
    : `${d.getMonth() + 1}月${d.getDate()}日 这一周`
})

function shift(delta: number) {
  const d = new Date(anchor.value)
  anchor.value =
    mode.value === 'month'
      ? new Date(d.getFullYear(), d.getMonth() + delta, 1).getTime()
      : startOfDay(anchor.value + delta * 7 * DAY)
}

const monthOf = (ms: number) => new Date(anchor.value).getMonth() === new Date(ms).getMonth()
const isToday = (ms: number) => sameDay(ms, now.value)
const isOverdue = (ms: number) => ms < startOfDay(now.value)
const dayLabel = (ms: number) => `${new Date(ms).getMonth() + 1}/${new Date(ms).getDate()}`
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <Segmented
        v-model="mode"
        :options="[
          { value: 'month', label: '月' },
          { value: 'week', label: '周' },
        ]"
      />
      <div class="nav">
        <button type="button" class="nb" aria-label="上一段" @click="shift(-1)">
          <ChevronLeft :size="14" />
        </button>
        <span class="tt">{{ title }}</span>
        <button type="button" class="nb" aria-label="下一段" @click="shift(1)">
          <ChevronRight :size="14" />
        </button>
      </div>
      <button type="button" class="y-ghost today" @click="anchor = startOfDay(now)">回到今天</button>
    </div>

    <div class="wkrow">
      <span v-for="(w, i) in WEEK" :key="i">{{ w }}</span>
    </div>

    <div class="grid" :class="mode">
      <div
        v-for="day in days"
        :key="day"
        class="cell"
        :class="{ dim: mode === 'month' && !monthOf(day), today: isToday(day), over: isOverdue(day) }"
      >
        <div class="ch">{{ dayLabel(day) }}</div>
        <button
          v-for="todo in byDay.get(day)?.slice(0, mode === 'week' ? 12 : 3)"
          :key="todo.id"
          type="button"
          class="ti"
          :class="`p${todo.priority}`"
          :title="todo.title"
          @click="shell.select({ kind: 'todo', id: todo.id })"
        >
          {{ todo.title }}
        </button>
        <div v-if="(byDay.get(day)?.length ?? 0) > 3 && mode === 'month'" class="more">
          +{{ (byDay.get(day)?.length ?? 0) - 3 }}
        </div>
      </div>
    </div>

    <p class="hint">只显示有截止时间且未完成的待办。灰色格子是别的月份的补位，不是空档。</p>
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 9px;
  min-height: 0;
  flex: 1;
}
.bar {
  display: flex;
  align-items: center;
  gap: 12px;
}
.nav {
  display: flex;
  align-items: center;
  gap: 8px;
}
.tt {
  font-size: 13px;
  font-weight: 800;
  min-width: 118px;
  text-align: center;
}
.nb {
  width: 24px;
  height: 24px;
  border-radius: 7px;
  color: var(--t3);
  display: grid;
  place-items: center;
}
.nb:hover {
  background: var(--card);
  color: var(--t1);
}
.today {
  margin-left: auto;
  flex: none;
  width: auto;
  padding: 0 11px;
  height: 26px;
}
.wkrow {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 6px;
  font-size: 10.5px;
  color: var(--t3);
  padding: 0 2px;
}
.wkrow span {
  text-align: center;
}
.grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 6px;
}
.grid.month {
  grid-template-rows: repeat(6, minmax(0, 1fr));
}
.grid.week {
  grid-template-rows: minmax(0, 1fr);
}
.cell {
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--card);
  padding: 6px 7px;
  display: flex;
  flex-direction: column;
  gap: 3px;
  overflow: hidden;
  min-width: 0;
}
.cell.dim {
  opacity: 0.42;
}
.cell.today {
  border-color: color-mix(in srgb, var(--accent) 34%, transparent);
}
.ch {
  font-size: 10.5px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.cell.over .ch {
  color: var(--red);
  font-weight: 800;
}
.ti {
  text-align: left;
  font-size: 11px;
  padding: 2.5px 6px;
  border-radius: 5px;
  background: var(--card-2);
  color: var(--t2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-left: 2px solid var(--line-2);
}
.ti.p0 {
  border-left-color: var(--red);
}
.ti.p1 {
  border-left-color: var(--accent);
}
.ti:hover {
  color: var(--t1);
  background: var(--panel);
}
.more {
  font-size: 10px;
  color: var(--t3);
  padding-left: 4px;
}
.hint {
  font-size: 11px;
  color: var(--t3);
}
</style>
