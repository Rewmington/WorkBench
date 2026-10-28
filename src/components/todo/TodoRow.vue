<script setup lang="ts">
import { ChevronDown, Pin } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import type { Priority, Todo } from '../../api/types'
import { dueLabel, overdueLabel } from '../../lib/reltime'
import DueEditor from './DueEditor.vue'

const props = defineProps<{ todo: Todo; now: number; childCount: number }>()
const emit = defineEmits<{
  toggle: []
  patch: [Partial<Pick<Todo, 'title' | 'note' | 'priority' | 'dueAt' | 'pinned'>>]
  remove: []
  select: []
  dragStart: [PointerEvent]
}>()

const editing = ref(false)
const draft = ref('')

const PRIORITY_LABEL: Record<Priority, string> = { 0: '紧急', 1: '高', 2: '普通', 3: '低' }

const overdue = computed(() => overdueLabel(props.now, props.todo.dueAt))
const due = computed(() => dueLabel(props.now, props.todo.dueAt))

function startEdit() {
  draft.value = props.todo.title
  editing.value = true
}

function commit() {
  const title = draft.value.trim()
  editing.value = false
  if (title && title !== props.todo.title) emit('patch', { title })
}

/** 点圆点循环优先级：0→1→2→3→0。比弹菜单少一步，也不用记哪个颜色是啥 */
function cyclePriority() {
  const next = ((props.todo.priority + 1) % 4) as Priority
  emit('patch', { priority: next })
}
</script>

<template>
  <div
    class="row"
    :class="{ done: todo.doneAt, sel: false, over: !!overdue }"
    @click="emit('select')"
  >
    <span class="grip" title="拖动排序" @pointerdown="emit('dragStart', $event)">⠿</span>

    <button
      type="button"
      class="cb"
      :aria-checked="!!todo.doneAt"
      role="checkbox"
      @click.stop="emit('toggle')"
    >
      <span v-if="todo.doneAt" class="tick">✓</span>
    </button>

    <button type="button" class="pri" :class="`p${todo.priority}`" :title="`优先级：${PRIORITY_LABEL[todo.priority]}，点一下换`" @click.stop="cyclePriority">
      {{ PRIORITY_LABEL[todo.priority] }}
    </button>

    <div class="body">
      <input
        v-if="editing"
        v-model="draft"
        class="edit palette-query"
        @keydown.enter.prevent="commit"
        @keydown.esc.prevent="editing = false"
        @blur="commit"
      />
      <span v-else class="t" @dblclick="startEdit" :title="todo.note || '双击改标题'">{{ todo.title }}</span>

      <span v-if="childCount" class="kids">
        <ChevronDown :size="11" />
        {{ childCount }} 个子项
      </span>
      <span v-for="tag in todo.tags" :key="tag" class="tag">{{ tag }}</span>
    </div>

    <div class="meta" @click.stop>
      <DueEditor
        :model-value="todo.dueAt"
        :now="now"
        @update:model-value="(v) => emit('patch', { dueAt: v })"
      />
      <span v-if="overdue" class="over-txt">{{ overdue }}</span>
      <span v-else-if="todo.dueAt" class="due-txt">{{ due }}</span>
      <button
        type="button"
        class="pin"
        :class="{ on: todo.pinned }"
        title="置顶"
        @click.stop="emit('patch', { pinned: !todo.pinned })"
      >
        <Pin :size="12" />
      </button>
      <button type="button" class="del" title="删除（可撤销）" @click.stop="emit('remove')">×</button>
    </div>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 0;
  border-top: 1px solid var(--line);
  min-width: 0;
}
.row:first-child {
  border-top: none;
}
.grip {
  color: var(--t3);
  cursor: grab;
  font-size: 12px;
  width: 12px;
  flex: none;
  opacity: 0;
  transition: opacity var(--dur-1) var(--ease);
}
.row:hover .grip {
  opacity: 1;
}
.cb {
  width: 15px;
  height: 15px;
  flex: none;
  border-radius: 5px;
  border: 1.5px solid var(--line-2);
  display: grid;
  place-items: center;
  color: var(--accent-ink);
}
.cb:hover {
  border-color: var(--accent);
}
.tick {
  font-size: 11px;
  font-weight: 900;
}
.done .cb {
  background: var(--accent);
  border-color: transparent;
}
.pri {
  flex: none;
  font-size: 10px;
  font-weight: 800;
  padding: 2px 6px;
  border-radius: 5px;
  background: var(--card-2);
  color: var(--t3);
  border: 1px solid var(--line);
}
.pri.p0 {
  background: rgba(248, 113, 113, 0.16);
  border-color: transparent;
  color: var(--red);
}
.pri.p1 {
  background: var(--accent-soft);
  border-color: transparent;
  color: var(--accent);
}
.pri.p3 {
  opacity: 0.7;
}
.body {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 7px;
}
.t {
  font-size: 12px;
  font-weight: 550;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.done .t {
  color: var(--t3);
  text-decoration: line-through;
  text-decoration-color: var(--line-2);
}
.edit {
  flex: 1;
  min-width: 0;
  font: inherit;
  background: var(--card-2);
  border: 1px solid var(--line-2);
  border-radius: 6px;
  color: var(--t1);
  padding: 3px 6px;
  outline: none;
}
.kids {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: 10px;
  color: var(--t3);
  flex: none;
}
.tag {
  font-size: 10px;
  padding: 1.5px 6px;
  border-radius: 20px;
  border: 1px solid var(--line);
  color: var(--t3);
  flex: none;
}
.meta {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}
.due-txt {
  font-size: 10.5px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
/* 逾期不只变红：文字本身换了说法与位置（状态不只靠颜色，§2 原则 3） */
.over-txt {
  font-size: 10.5px;
  font-weight: 800;
  color: var(--red);
}
.pin,
.del {
  width: 20px;
  height: 20px;
  border-radius: 5px;
  display: grid;
  place-items: center;
  color: var(--t3);
  opacity: 0;
  transition: opacity var(--dur-1) var(--ease);
}
.pin.on {
  opacity: 1;
  color: var(--accent);
}
.del {
  font-size: 15px;
  line-height: 1;
}
.row:hover .pin,
.row:hover .del {
  opacity: 1;
}
.del:hover {
  color: var(--red);
}
</style>
