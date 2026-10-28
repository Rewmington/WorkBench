<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { RefreshCw } from 'lucide-vue-next'

import { api } from '../api'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import UndoBar from '../components/ui/UndoBar.vue'
import { useClock } from '../composables/useClock'
import { useLinked } from '../composables/useLinked'
import { useShell } from '../composables/useShell'
import { useUndo } from '../composables/useUndo'
import { relTime } from '../lib/reltime'

/**
 * 药箱是**只读源**：复制它的库再打开，绝不写回去。
 * 它的同步是「每设备全量快照 + 按 updated_at 行级 LWW」且只在启动/手动时跑，
 * 我们写进去的东西会被下一次合并覆盖掉 —— 所以「补买」是往我们自己的待办里写。
 */
const linked = useLinked()
const shell = useShell()
const undo = useUndo()
const { now } = useClock()

const snap = computed(() => linked.medicineSnap.value)
const data = computed(() => linked.medicine.value)
const items = computed(() => data.value?.items ?? [])
const meta = computed(() => data.value?.meta ?? {})

const lowOnes = computed(() => items.value.filter((m) => m.isLow || m.isOut))

function stockTone(daysLeft: number | null, lowDays: number) {
  if (daysLeft == null) return 'unknown'
  if (daysLeft <= 0) return 'out'
  if (daysLeft <= lowDays) return 'low'
  return 'ok'
}

async function restock(name: string, daysLeft: number | null) {
  const todo = await api.linkedRestockTodo(name, daysLeft)
  await shell.go('todos')
  undo.offer({
    label: `已建「${todo.title}」`,
    run: async () => {
      const affected = await api.todosDelete(todo.id)
      await api.todosRestore(affected)
    },
  })
}

onMounted(async () => {
  if (!linked.state.value) await linked.load()
  void linked.refreshMedicine()
})
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>药箱联动 <span class="sub">只读副本 · 不写回</span></h2>
      <span v-if="snap" class="ago">副本读取于 {{ relTime(now, snap.fetchedAt) }}</span>
      <button type="button" class="y-ghost btn" :disabled="linked.busy.value.medicine" @click="linked.refreshMedicine()">
        <RefreshCw :size="13" />
        重新读取
      </button>
    </div>

    <CollapsedRow
      v-if="!items.length"
      :label="`药箱联动 · ${snap?.status === 'missing' ? '没找到药箱的库' : snap?.status === 'error' ? '副本读不了' : '还没有药品'}`"
      :detail="snap?.detail"
      :fetched-at="snap?.fetchedAt"
      :busy="linked.busy.value.medicine"
      @retry="linked.refreshMedicine()"
    />

    <template v-else>
      <div class="sum">
        <span class="s">
          今日时段
          <b>{{ meta.todayTaken ?? 0 }}/{{ meta.todayOwed ?? 0 }}</b>
          已记
        </span>
        <span v-if="lowOnes.length" class="s warn">
          余量偏低 <b>{{ lowOnes.length }}</b> 种
        </span>
        <span class="s">共 {{ items.length }} 种药</span>
      </div>

      <div v-for="m in items" :key="m.id" class="med">
        <div class="mh">
          <span class="nm">{{ m.name }}</span>
          <span class="y-chip" :class="stockTone(m.daysLeft, m.lowDays) === 'low' ? 'r' : stockTone(m.daysLeft, m.lowDays) === 'out' ? 'r' : 'a'">
            {{ m.daysLeft == null ? '未设用量' : m.isOut ? '已用完' : `剩 ${m.daysLeft} 天` }}
          </span>
          <button
            v-if="m.isLow || m.isOut"
            type="button"
            class="y-ghost buy"
            @click="restock(m.name, m.daysLeft)"
          >
            补买
          </button>
        </div>

        <div class="slots">
          <span v-for="s in m.slots" :key="s.minute" class="slot" :class="s.state">
            <i class="dot"></i>{{ s.label }}
            <em>{{ s.state === 'taken' ? '已记' : s.state === 'pending' ? '待记' : s.state === 'skipped' ? '没吃' : '稍后' }}</em>
          </span>
        </div>

        <div class="foot">
          <span>{{ m.remaining }} {{ m.unit }}<template v-if="m.boxes"> · 约 {{ m.boxes }} 盒</template></span>
          <span v-if="m.dailyUse">每天 {{ m.dailyUse }} {{ m.unit }}</span>
          <span>预警线 {{ m.lowDays }} 天</span>
        </div>
      </div>

      <p class="note">
        这些数来自 <code>{{ linked.state.value?.paths.medicineDb }}</code> 的一次性副本，读完即删。
        工作台不会写药箱的库，也不会写它的 <code>medicine-sync/</code> 目录。
      </p>
    </template>

    <UndoBar />
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 11px;
  min-height: 0;
  flex: 1;
  overflow-y: auto;
}
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
h2 {
  font-size: 13.5px;
  font-weight: 800;
}
.sub {
  font-size: 11px;
  font-weight: 500;
  color: var(--t3);
}
.ago {
  margin-left: auto;
  font-size: 11px;
  color: var(--t3);
}
.btn {
  flex: none;
  height: 27px;
  width: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.sum {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.s {
  font-size: 11.5px;
  color: var(--t2);
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: 9px;
  padding: 6px 11px;
}
.s b {
  font-size: 13px;
  font-weight: 800;
  color: var(--t1);
}
.s.warn b {
  color: var(--amber);
}
.med {
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: var(--r);
  padding: 12px 13px;
  display: flex;
  flex-direction: column;
  gap: 9px;
}
.mh {
  display: flex;
  align-items: center;
  gap: 8px;
}
.nm {
  font-size: 13px;
  font-weight: 750;
}
.buy {
  margin-left: auto;
  flex: none;
  width: auto;
  height: 24px;
  padding: 0 10px;
  font-size: 11.5px;
}
.slots {
  display: flex;
  gap: 7px;
  flex-wrap: wrap;
}
.slot {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  padding: 4px 9px;
  border-radius: 7px;
  background: var(--card-2);
  border: 1px solid var(--line);
  color: var(--t2);
}
.slot .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--t3);
}
.slot em {
  font-style: normal;
  color: var(--t3);
  font-size: 10px;
}
.slot.taken {
  border-color: color-mix(in srgb, var(--accent) 30%, transparent);
}
.slot.taken .dot {
  background: var(--accent);
}
.slot.taken em {
  color: var(--accent);
}
/* 待记：不只换个颜色，文字与位置都在 —— 状态不只靠颜色（§2 原则 3） */
.slot.pending {
  background: rgba(248, 113, 113, 0.1);
}
.slot.pending .dot {
  background: var(--red);
}
.slot.pending em {
  color: var(--red);
  font-weight: 800;
}
.slot.skipped .dot {
  background: transparent;
  border: 1px solid var(--t3);
}
.slot.upcoming {
  opacity: 0.62;
}
.foot {
  display: flex;
  gap: 12px;
  font-size: 10.5px;
  color: var(--t3);
}
.note {
  font-size: 11px;
  color: var(--t3);
  line-height: 1.6;
}
code {
  font-family: ui-monospace, monospace;
  font-size: 10.5px;
  color: var(--t2);
  word-break: break-all;
}
</style>
