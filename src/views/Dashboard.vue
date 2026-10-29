<script setup lang="ts">
import { ChevronRight, Lightbulb } from 'lucide-vue-next'
import { computed, onMounted, ref } from 'vue'

import { api } from '../api'
import type { DashSummary, Kpi, Suggestion, Todo } from '../api/types'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import CompletionTimeline from '../components/dash/CompletionTimeline.vue'
import FocusRing from '../components/dash/FocusRing.vue'
import { useClock } from '../composables/useClock'
import { useDashboard } from '../composables/useDashboard'
import { useShell } from '../composables/useShell'
import { useTodos } from '../composables/useTodos'
import { clockDate, clockTime, dayInt, dueLabel, monthDays } from '../lib/reltime'

/**
 * 候选一的工作台（PLAN §4 + §8.8）。六个视觉区：KPI 行 / 队列 / 卡片环 /
 * 完成时间轴 / 详情面板 / 下一步建议。
 *
 * 密度是这一版的代价，所以 §8.8.6 的两条在这里是硬约束：
 * **一屏只允许一个视觉重心**（KPI 里只有一张翻亮，环上只有一张 `.hot`），
 * 以及空的东西不许占地方（每个区都有自己的折叠态）。
 */
const shell = useShell()
const { now } = useClock()
const todosModel = useTodos()
const dash = useDashboard()

const summary = computed(() => dash.summary.value)

/** 选中哪一张 KPI 卡：它决定队列列显示什么。默认「今天要做的」。 */
const active = ref<'today' | 'inbox' | 'done' | 'late'>('today')

const CELLS: { key: 'today' | 'inbox' | 'done' | 'late'; en: string; cn: string; glow: number }[] = [
  { key: 'today', en: 'TODAY', cn: '今天要做的', glow: 0 },
  { key: 'late', en: 'LATE', cn: '逾期未清', glow: 3 },
  { key: 'done', en: 'DONE', cn: '今天完成', glow: 2 },
  { key: 'inbox', en: 'INBOX', cn: '待整理', glow: 1 },
]

function kpiOf(key: string): Kpi | null {
  const s = summary.value
  if (!s) return null
  return s[key as keyof DashSummary] as Kpi
}

function unitOf(key: string): string {
  return key === 'inbox' ? '条' : '项'
}

/** 变化量与次级说明互斥：delta 只在 DONE 上有，其余显示 note（理由见 repos/dashboard.rs）。 */
function footOf(k: Kpi): { text: string; emphasis: boolean; down: boolean } {
  if (k.delta != null && k.delta !== 0) {
    return { text: `较昨日 ${k.delta > 0 ? '↑' : '↓'} ${Math.abs(k.delta)}`, emphasis: true, down: k.delta < 0 }
  }
  if (k.delta != null) return { text: '与昨日持平', emphasis: false, down: false }
  return { text: k.note ?? '', emphasis: false, down: false }
}

/**
 * 队列列：三个分组一次全列（照参考图的密度），KPI 点谁就把那一组标成当前组。
 * 每组最多铺 4 行 —— 驾驶舱是「看一眼然后动手」的地方，不是列表页，
 * 剩下的用「还有 N 条」跳回待办视图，而不是把首屏拉成三屏。
 */
const CAP = 4

const groups = computed(() => {
  const s = summary.value
  if (!s) return []
  const out = s.groups
    .filter((g) => g.items.length > 0)
    .map((g) => ({ key: g.key as string, label: g.label, dot: g.key as string, items: g.items, rest: g.items.length - CAP }))
  if (active.value === 'done') {
    const done = todosModel.doneToday.value
    if (done.length) out.unshift({ key: 'done', label: '今天完成', dot: 'done', items: done, rest: done.length - CAP })
  }
  return out.filter((g) => g.items.length > 0)
})

const queueTotal = computed(() => groups.value.reduce((n, g) => n + g.items.length, 0))

const month = computed(() => monthDays(dayInt(now.value)))
const todayInt = computed(() => dayInt(now.value))

/** 仪表环的分母是「本月已完成 + 本月到期还没做」，不是用户从没设过的目标数。 */
function gaugePct(s: DashSummary): number {
  const total = s.month.done + s.month.open
  return total === 0 ? 0 : Math.round((s.month.done / total) * 100)
}

async function toggle(t: Todo) {
  await api.todosSetDone(t.id, !t.completed)
  await Promise.all([dash.refresh(), todosModel.refresh()])
}

function pick(t: Todo) {
  shell.select({ kind: 'todo', id: t.id })
}

function follow(s: Suggestion) {
  if (s.todoId) shell.select({ kind: 'todo', id: s.todoId })
  if (s.view) shell.go(s.view)
}

onMounted(async () => {
  await Promise.all([dash.refresh(), todosModel.refresh()])
})
</script>

<template>
  <section class="y-view y-dash">
    <div class="y-kpis">
      <div class="y-clock">
        <span class="en">NOW</span>
        <div class="num">{{ clockTime(now) }}</div>
        <div class="d">{{ clockDate(now) }}</div>
      </div>

      <button
        v-for="c in CELLS"
        :key="c.key"
        type="button"
        class="y-kpi"
        :class="{ sel: active === c.key }"
        :style="{ '--glow': `var(--glow-${c.glow})` }"
        @click="active = c.key"
      >
        <span class="en">{{ c.en }}</span><span class="cn">{{ c.cn }}</span>
        <div class="num">
          {{ kpiOf(c.key)?.value ?? '—' }}<span>{{ unitOf(c.key) }}</span>
        </div>
        <div v-if="kpiOf(c.key) && footOf(kpiOf(c.key)!).text" class="d" :class="{ down: footOf(kpiOf(c.key)!).down }">
          <i v-if="footOf(kpiOf(c.key)!).emphasis">{{ footOf(kpiOf(c.key)!).text }}</i>
          <template v-if="!footOf(kpiOf(c.key)!).emphasis">{{ footOf(kpiOf(c.key)!).text }}</template>
        </div>
      </button>
    </div>

    <div class="y-mid">
      <div class="y-col">
        <div class="y-sech">
          <h2>待办队列</h2>
          <span class="y-demo">{{ queueTotal }}</span>
        </div>

        <!-- 首屏读本地库通常在一帧内完成；真慢下来时给骨架屏而不是转圈（§8.6） -->
        <div v-if="!summary" class="skel">
          <div v-for="i in 3" :key="i" class="sk" :style="{ width: 92 - i * 11 + '%' }"></div>
        </div>
        <CollapsedRow
          v-else-if="!groups.length"
          label="队列是空的"
          detail="空的不占地方 —— 按 Ctrl+K 新建一条，或去待办视图看已完成"
        />
        <template v-else>
          <div v-for="g in groups" :key="g.key" class="y-q" :class="{ on: active === g.key }">
            <div class="h">
              <i class="dot" :class="g.dot"></i>
              {{ g.label }}
              <span class="c">{{ g.items.length }}</span>
            </div>
            <div v-for="t in g.items.slice(0, CAP)" :key="t.id" class="y-qr" @click="pick(t)">
              <span class="cb" :class="{ on: t.completed }" @click.stop="toggle(t)"></span>
              <span class="t">{{ t.title }}</span>
              <span v-if="t.dueAt != null" class="tm">{{ dueLabel(now, t.dueAt) }}</span>
            </div>
            <button v-if="g.rest > 0" type="button" class="more" @click="shell.go('todos')">
              还有 {{ g.rest }} 条 · 去待办 <ChevronRight :size="11" />
            </button>
          </div>
        </template>

        <div v-if="dash.suggestions.value.length" class="y-ai">
          <div class="h">
            <Lightbulb :size="12" />
            下一步建议
            <span class="no">规则 · 非 AI</span>
          </div>
          <ul>
            <li v-for="s in dash.suggestions.value" :key="s.rule">
              <button type="button" class="link" @click="follow(s)">{{ s.headline }}</button>
              <span class="why">{{ s.detail }}</span>
            </li>
          </ul>
        </div>
      </div>

      <div class="y-stage">
        <CollapsedRow
          v-if="summary && !summary.focus.length"
          label="今日焦点环 · 没有到期的事"
          detail="环不画空转的圈 —— 有逾期或今天到期的就会出现在这里"
        />
        <FocusRing v-else :cards="summary?.focus ?? []">
          <!-- 仪表卡交给环自己定位：降级成列表时它不该压在列表第一张卡上 -->
          <template #overlay>
            <div class="y-gauge">
              <div class="k">本月清掉</div>
              <div class="v">{{ summary ? gaugePct(summary) : '—' }}%</div>
              <div class="m">{{ summary?.month.done ?? 0 }} / {{ (summary?.month.done ?? 0) + (summary?.month.open ?? 0) }} 项</div>
              <div class="bar"><i :style="{ width: (summary ? gaugePct(summary) : 0) + '%' }"></i></div>
            </div>
          </template>
        </FocusRing>
      </div>
    </div>

    <CompletionTimeline
      :days="month"
      :today="todayInt"
      :timeline="summary?.timeline ?? []"
      :history-days="summary?.historyDays ?? 0"
      :done="summary?.month.done ?? 0"
      :open="summary?.month.open ?? 0"
      @pick="(d) => shell.select({ kind: 'day', day: d })"
    />
  </section>
</template>

<style scoped>
.y-dash {
  display: flex;
  flex-direction: column;
  gap: 0;
}
/* KPI 行：内容自适应换行，不给它留「凑满四格」的空位 */
.y-kpis {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(148px, 1fr));
  gap: 11px;
  margin-bottom: 14px;
}
.y-clock,
.y-kpi {
  position: relative;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 13px 14px;
  min-height: 106px;
  background: var(--card);
  text-align: left;
}
/* §8.8.2：照片底换成预烘焙多停点渐变。零模糊、零额外合成层。 */
.y-kpi::after {
  content: '';
  position: absolute;
  inset: 0;
  background: var(--glow, transparent);
  opacity: var(--glow-opacity);
  pointer-events: none;
}
.y-kpi > * {
  position: relative;
}
.y-kpi .en {
  font: 800 9.5px/1 ui-monospace, Consolas, monospace;
  letter-spacing: 0.13em;
  color: var(--t2);
}
.y-kpi .cn {
  font-size: 11.5px;
  color: var(--t3);
  font-weight: 650;
  margin-left: 6px;
}
.y-kpi .num,
.y-clock .num {
  font-size: 34px;
  font-weight: 850;
  letter-spacing: -0.045em;
  line-height: 1.05;
  margin-top: 8px;
  font-variant-numeric: tabular-nums;
}
.y-kpi .num span {
  font-size: 11.5px;
  font-weight: 650;
  opacity: 0.72;
  margin-left: 5px;
  letter-spacing: 0;
}
.y-kpi .d,
.y-clock .d {
  font-size: 11px;
  color: var(--t2);
  font-weight: 650;
  margin-top: 7px;
}
.y-kpi .d i {
  font-style: normal;
  color: var(--accent);
}
.y-kpi .d.down i {
  color: var(--red);
}
/* §8.1.3 选中卡翻亮：亮底上文字必须压深才够对比度（§8.5 冻结值） */
.y-kpi.sel {
  background: var(--card-sel);
  color: var(--sel-ink);
  border-color: transparent;
  box-shadow: 0 16px 36px rgba(0, 0, 0, 0.32);
}
.y-kpi.sel .en,
.y-kpi.sel .cn,
.y-kpi.sel .d {
  color: var(--sel-muted);
}
.y-kpi.sel .d i {
  color: var(--sel-delta);
}
/* 亮色下卡本身就是白的，「翻亮」翻不出差别 —— 用描边把重心找回来。
   1.5px 的环不是实心填充，不违反 §8.8.6 那一屏一个重心的约束。 */
[data-mode='light'] .y-kpi.sel {
  box-shadow: 0 0 0 1.5px var(--sel-delta), 0 10px 26px rgba(20, 30, 26, 0.13);
}
.y-clock .en {
  font: 800 9.5px/1 ui-monospace, Consolas, monospace;
  letter-spacing: 0.13em;
  color: var(--t3);
}

.y-mid {
  display: grid;
  grid-template-columns: 268px minmax(0, 1fr);
  gap: 12px;
  /* 不拉伸：左列比环高时，右栏跟着撑开会露出一整块死白（§2 原则 3） */
  align-items: start;
}
.y-col {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}
.y-sech {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 2px;
}
.y-sech h2 {
  margin: 0;
  font-size: 13.5px;
  font-weight: 800;
  letter-spacing: -0.015em;
}
.y-q {
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: 12px;
  padding: 4px 11px 8px;
}
.skel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 11px;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--card);
}
.sk {
  height: 12px;
  border-radius: 6px;
  background: var(--card-2);
}
.y-q.on {
  border-color: var(--line-2);
}
.y-q .h {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  font-weight: 750;
  padding: 8px 0 2px;
}
.y-q .h .c {
  color: var(--t3);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.y-q .h .dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex: none;
  background: var(--t3);
}
.y-q .h .dot.late {
  background: var(--red);
}
.y-q .h .dot.today {
  background: var(--accent);
}
.y-q .h .dot.later {
  background: var(--blue);
}
.y-q .more {
  display: flex;
  align-items: center;
  gap: 4px;
  font: inherit;
  font-size: 11px;
  font-weight: 700;
  color: var(--t3);
  padding: 7px 0 1px;
  border-top: 1px solid var(--line);
  margin-top: 2px;
  width: 100%;
  text-align: left;
}
.y-q .more:hover {
  color: var(--t1);
}
.y-qr {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 0;
  border-top: 1px solid var(--line);
  font-size: 12px;
  cursor: pointer;
}
.y-qr .cb {
  width: 13px;
  height: 13px;
  border-radius: 4px;
  border: 1.5px solid var(--line-2);
  flex: none;
}
.y-qr .cb.on {
  background: var(--accent);
  border-color: transparent;
}
.y-qr .t {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 550;
}
.y-qr .tm {
  font-size: 10.5px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
  flex: none;
}

.y-stage {
  position: relative;
  min-width: 0;
  border: 1px solid var(--line);
  border-radius: var(--r);
  background: linear-gradient(180deg, var(--card), var(--card-2));
  padding: 11px;
}
.y-gauge {
  position: absolute;
  left: 14px;
  top: 14px;
  z-index: 3;
  width: 128px;
  border-radius: 12px;
  padding: 12px 13px;
  background: var(--panel);
  border: 1px solid var(--line-2);
  /* §8.8.1：动效面上禁止 blur —— 参考图这块是 backdrop-filter，这里换成不透明底 */
}
.y-gauge .k {
  font-size: 11px;
  color: var(--t2);
  font-weight: 650;
}
.y-gauge .v {
  font-size: 27px;
  font-weight: 850;
  letter-spacing: -0.04em;
  margin-top: 5px;
  font-variant-numeric: tabular-nums;
}
.y-gauge .m {
  font-size: 10.5px;
  color: var(--t3);
  margin-top: 2px;
}
.y-gauge .bar {
  height: 4px;
  border-radius: 3px;
  background: rgba(128, 140, 135, 0.25);
  margin-top: 9px;
  overflow: hidden;
}
.y-gauge .bar i {
  display: block;
  height: 100%;
  background: var(--accent);
  transition: width 160ms ease-out;
}

.y-ai {
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--card);
  padding: 11px 12px;
}
.y-ai .h {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 800;
  margin-bottom: 8px;
  color: var(--accent);
}
.y-ai .h .no {
  margin-left: auto;
  font-size: 9.5px;
  font-weight: 800;
  letter-spacing: 0.04em;
  color: var(--amber);
  border: 1px solid var(--line-2);
  padding: 2px 6px;
  border-radius: 5px;
}
.y-ai ul {
  margin: 0;
  padding-left: 15px;
  color: var(--t2);
  font-size: 12px;
  line-height: 1.7;
}
.y-ai li {
  margin-bottom: 6px;
}
.y-ai li:last-child {
  margin-bottom: 0;
}
.y-ai .link {
  font: inherit;
  font-weight: 750;
  color: var(--t1);
  text-align: left;
  padding: 0;
}
.y-ai .link:hover {
  text-decoration: underline;
  text-underline-offset: 2px;
}
.y-ai .why {
  display: block;
  font-size: 11px;
  color: var(--t3);
}

@media (max-width: 1080px) {
  .y-mid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
