<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'

import { api } from '../api'
import type { ClassRow, Todo, TodoZone } from '../api/types'
import TodoRow from '../components/todo/TodoRow.vue'
import Segmented from '../components/ui/Segmented.vue'
import UndoBar from '../components/ui/UndoBar.vue'
import { useClock } from '../composables/useClock'
import { useDragSort } from '../composables/useDragSort'
import { useShell } from '../composables/useShell'
import { useTodos } from '../composables/useTodos'
import { useUndo } from '../composables/useUndo'
import { errText } from '../lib/err'
import { hhmm, MIN_GAP_MINUTES, nextFreeGap } from '../lib/timetable'

const ZONES: { value: TodoZone; label: string }[] = [
  { value: 'today', label: '今天' },
  { value: 'inbox', label: '收件箱' },
  { value: 'later', label: '以后' },
  { value: 'someday', label: '某天' },
]

const shell = useShell()
const undo = useUndo()
const todosModel = useTodos()
const { now } = useClock()

const zone = ref<TodoZone>('today')
const rows = ref<Todo[]>([])
const loading = ref(true)
const draft = ref('')
const composer = ref<HTMLInputElement | null>(null)
const listEl = ref<HTMLElement | null>(null)
const error = ref<string | null>(null)

const { start: dragStart, draggingId, overId } = useDragSort(
  () => listEl.value,
  async (target) => {
    try {
      await api.todosReorder(target.id, target.prevId, target.nextId)
      await reload()
    } catch (e) {
      error.value = errText(e)
    }
  },
)

async function reload() {
  loading.value = true
  try {
    rows.value = await api.todosList(zone.value)
    // 徽章走共享读模型，避免「列表 3 条、徽章 5 条」
    await todosModel.refresh()
    await loadGap()
    error.value = null
  } catch (e) {
    error.value = errText(e)
  } finally {
    loading.value = false
  }
}

async function add() {
  const title = draft.value.trim()
  if (!title) return
  draft.value = ''
  try {
    await api.todosCreate({ title, zone: zone.value })
    await reload()
  } catch (e) {
    error.value = errText(e)
  }
}

async function toggle(todo: Todo) {
  try {
    await api.todosSetDone(todo.id, !todo.completed)
    await reload()
  } catch (e) {
    error.value = errText(e)
  }
}

async function patch(todo: Todo, part: Record<string, unknown>) {
  try {
    await api.todosUpdate(todo.id, part)
    await reload()
  } catch (e) {
    error.value = errText(e)
  }
}

async function remove(todo: Todo) {
  try {
    const affected = await api.todosDelete(todo.id)
    await reload()
    undo.offer({
      label: `已删除「${todo.title}」${affected.length > 1 ? ` 等 ${affected.length} 项` : ''}`,
      run: async () => {
        await api.todosRestore(affected)
        await reload()
      },
    })
  } catch (e) {
    error.value = errText(e)
  }
}

const counts = computed(() => {
  const map = new Map<string, number>()
  for (const t of rows.value) {
    if (t.parentId) map.set(t.parentId, (map.get(t.parentId) ?? 0) + 1)
  }
  return map
})

/** 子项跟在父项下面显示；父项被删时子项一起消失（后端已打墓碑，这里只是不再渲染） */
const visible = computed(() => {
  const known = new Set(rows.value.map((t) => t.id))
  const top = rows.value.filter((t) => !t.parentId || !known.has(t.parentId))
  const out: Todo[] = []
  for (const t of top) {
    out.push(t)
    out.push(...rows.value.filter((c) => c.parentId === t.id))
  }
  return out
})

/**
 * 「今天」档按时间分带（PLAN §8.8 第 8 条）：这一屏要回答的是"几点到几点有空"，
 * 一条一行给不出这个。其余档保持一条一行 —— 那里没有这个问题要回答。
 *
 * 子项跟着父项走，不单独按自己的 dueAt 跳带走：把子任务从父任务下面摘出去，
 * 比它排在错的时间带更难查。
 *
 * 已完成的单独一条带、且排最后：按 dueAt 排会让三个月前做完的旧账顶到最前面
 * （90 天历史一灌进来，今天要做的就被挤出屏幕了）。
 */
const todayBands = computed(() => {
  if (zone.value !== 'today') return null
  const known = new Set(rows.value.map((t) => t.id))
  const tops = rows.value.filter((t) => !t.parentId || !known.has(t.parentId))
  const kids = (id: string) => rows.value.filter((c) => c.parentId === id)
  const open = tops.filter((t) => !t.completed)
  const withTime = open
    .filter((t) => t.dueAt != null)
    .sort((a, b) => (a.dueAt as number) - (b.dueAt as number))
  const whenever = open.filter((t) => t.dueAt == null)
  const done = tops.filter((t) => t.completed)
  const withKids = (list: Todo[]) => list.flatMap((t) => [t, ...kids(t.id)])
  return { withTime: withKids(withTime), whenever: withKids(whenever), done: withKids(done) }
})

/** 有分带时列表顺序由时间决定，拖动排序会立刻被 reload 覆盖 —— 所以那条路要关掉 */
const shaped = computed(() => todayBands.value !== null)

/**
 * 渲染只有一条路径：其余档是"一条没有标题的带"，「今天」档是至多三条有标题的带。
 * 空带不占位 —— 「今天」档里全是没时间的待办时，不该看到一行"按截止时间 0"。
 */
const bands = computed<{ label: string | null; rows: Todo[] }[]>(() => {
  const b = todayBands.value
  if (!b) return [{ label: null, rows: visible.value }]
  const out: { label: string | null; rows: Todo[] }[] = []
  if (b.withTime.length) out.push({ label: '按截止时间', rows: b.withTime })
  if (b.whenever.length) out.push({ label: '随时', rows: b.whenever })
  if (b.done.length) out.push({ label: '已完成', rows: b.done })
  return out
})

/**
 * 今天几点到几点有空。读的是课表那份导入副本，没导入过就不显示这行（不编"全天有空"）。
 *
 * 存的是那一天的课，文案由 `now` 现算：空档的左端就是"现在"，写死在 load 那一刻
 * 会与课表页跳着走 —— §9 M9c 要两边是同一个值。
 */
const todayClasses = ref<ClassRow[] | null>(null)

async function loadGap() {
  if (zone.value !== 'today') {
    todayClasses.value = null
    return
  }
  const st = await api.timetableState()
  if (!st.rows || st.currentWeek == null) {
    todayClasses.value = null
    return
  }
  const week = await api.timetableWeek(st.currentWeek)
  const g = new Date(now.value).getDay()
  todayClasses.value = week.filter((c) => c.day === (g === 0 ? 7 : g))
}

const gapLine = computed(() => {
  const today = todayClasses.value
  if (today === null) return null
  if (!today.length) return '今天没课'
  const d = new Date(now.value)
  const gap = nextFreeGap(today, d.getHours() * 60 + d.getMinutes())
  return gap
    ? `今天 ${today.length} 节课 · 最近空档 ${hhmm(gap[0])}–${hhmm(gap[1])}`
    : `今天 ${today.length} 节课 · 没有 ≥${MIN_GAP_MINUTES} 分钟的空档`
})

watch(zone, reload)
onMounted(async () => {
  await reload()
  watch(
    () => shell.composerTick.value,
    async () => {
      if (shell.composerTarget.value !== 'todo') return
      await nextTick()
      composer.value?.focus()
    },
  )
})
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <Segmented v-model="zone" :options="ZONES" />
      <span class="cnt">{{ rows.filter((t) => !t.completed).length }} 项未完成</span>
    </div>

    <div class="composer">
      <input
        ref="composer"
        v-model="draft"
        class="palette-query"
        :placeholder="`在「${ZONES.find((z) => z.value === zone)?.label}」里新建待办，回车确认`"
        @keydown.enter.prevent="add"
      />
    </div>

    <p v-if="error" class="err">{{ error }}</p>

    <!-- 空档行在列表外面：今天档一条待办都没有时，这一屏仍然要回答"几点有空" -->
    <p v-if="gapLine" class="gap">{{ gapLine }}</p>

    <div v-if="visible.length" ref="listEl" class="list scroller">
      <div v-for="band in bands" :key="band.label ?? 'flat'" class="band">
        <div v-if="band.label" class="bandh">
          <span>{{ band.label }}</span>
          <span class="n">{{ band.rows.length }}</span>
        </div>
        <TodoRow
          v-for="todo in band.rows"
          :key="todo.id"
          :data-row-id="todo.id"
          :todo="todo"
          :now="now"
          :child-count="counts.get(todo.id) ?? 0"
          :sortable="!shaped"
          :class="{ dragging: draggingId === todo.id, 'drag-over': overId === todo.id }"
          @drag-start="dragStart($event, todo.id)"
          @toggle="toggle(todo)"
          @patch="patch(todo, $event)"
          @remove="remove(todo)"
          @select="shell.select({ kind: 'todo', id: todo.id })"
        />
      </div>
    </div>
    <div v-else class="y-empty">
      {{ loading ? '读取中…' : '这个清单是空的。上面输入框直接打字就能加一条。' }}
    </div>

    <UndoBar />
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
  flex: 1;
}
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
.cnt {
  font-size: 11px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.composer input {
  width: 100%;
  box-sizing: border-box;
  padding: 9px 12px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--t1);
  font-size: 12.5px;
  outline: none;
}
.composer input:focus {
  border-color: var(--line-2);
  box-shadow: inset 0 0 0 1px var(--accent-soft);
}
.list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 4px 11px;
}
.list :deep(.drag-over) {
  box-shadow: inset 0 2px 0 var(--accent);
}
.list :deep(.dragging) {
  opacity: 0.55;
}
.gap {
  margin: 0;
  font-size: 11px;
  color: var(--t2);
  font-variant-numeric: tabular-nums;
}
.band + .band {
  margin-top: 4px;
}
.bandh {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 0 1px;
  font-size: 10.5px;
  font-weight: 800;
  letter-spacing: 0.04em;
  color: var(--t3);
}
.bandh .n {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.err {
  font-size: 11.5px;
  color: var(--red);
}
</style>
