<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'

import { api } from '../api'
import type { Todo, TodoZone } from '../api/types'
import TodoRow from '../components/todo/TodoRow.vue'
import Segmented from '../components/ui/Segmented.vue'
import UndoBar from '../components/ui/UndoBar.vue'
import { useClock } from '../composables/useClock'
import { useDragSort } from '../composables/useDragSort'
import { useShell } from '../composables/useShell'
import { useTodos } from '../composables/useTodos'
import { useUndo } from '../composables/useUndo'

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
      error.value = String(e)
    }
  },
)

async function reload() {
  loading.value = true
  try {
    rows.value = await api.todosList(zone.value)
    // 徽章走共享读模型，避免「列表 3 条、徽章 5 条」
    await todosModel.refresh()
    error.value = null
  } catch (e) {
    error.value = String(e)
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
    error.value = String(e)
  }
}

async function toggle(todo: Todo) {
  try {
    await api.todosSetDone(todo.id, !todo.completed)
    await reload()
  } catch (e) {
    error.value = String(e)
  }
}

async function patch(todo: Todo, part: Record<string, unknown>) {
  try {
    await api.todosUpdate(todo.id, part)
    await reload()
  } catch (e) {
    error.value = String(e)
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
    error.value = String(e)
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

    <div v-if="visible.length" ref="listEl" class="list scroller">
      <TodoRow
        v-for="todo in visible"
        :key="todo.id"
        :data-row-id="todo.id"
        :todo="todo"
        :now="now"
        :child-count="counts.get(todo.id) ?? 0"
        :class="{ dragging: draggingId === todo.id, over: overId === todo.id }"
        @drag-start="dragStart($event, todo.id)"
        @toggle="toggle(todo)"
        @patch="patch(todo, $event)"
        @remove="remove(todo)"
        @select="shell.select({ kind: 'todo', id: todo.id })"
      />
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
.list :deep(.over) {
  box-shadow: inset 0 2px 0 var(--accent);
}
.list :deep(.dragging) {
  opacity: 0.55;
}
.err {
  font-size: 11.5px;
  color: var(--red);
}
</style>
