<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { CalendarRange, ChevronLeft, ChevronRight, Upload } from 'lucide-vue-next'

import { api } from '../api'
import type { ClassRow, TimetableImportReport, TimetableState } from '../api/types'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import { useClock } from '../composables/useClock'
import { useShell } from '../composables/useShell'
import { hhmm, MIN_GAP_MINUTES, nextFreeGap } from '../lib/timetable'
import { errText } from '../lib/err'

/**
 * 课表：教务导入的那张矩阵（PLAN §6.7）。
 *
 * 三件事决定这一页是不是有用：**节次列固定**（横滚时不跑掉）、**今天整列染色**、
 * **空档要显式写「空闲」** —— 最后一件最容易被省掉，而"什么时候有空"恰恰是
 * 这张表存在的理由，留白等于浪费一张网格。
 */
const shell = useShell()
const { now } = useClock()

const st = ref<TimetableState | null>(null)
const rows = ref<ClassRow[]>([])
const viewing = ref<number | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)
const notice = ref<string | null>(null)
const week1Draft = ref('')

const DAY_NAMES = ['周一', '周二', '周三', '周四', '周五', '周六', '周日']

async function load() {
  st.value = await api.timetableState()
  const w = viewing.value ?? st.value.currentWeek ?? 1
  viewing.value = w
  rows.value = await api.timetableWeek(w)
  week1Draft.value = st.value.week1Monday ?? ''
}

onMounted(() => {
  void load()
})

/** 本地星期 1–7，与 Rust 的 `days::weekday` 同一约定 */
const todayIdx = computed(() => {
  const g = new Date(now.value).getDay()
  return g === 0 ? 7 : g
})
const nowMin = computed(() => {
  const d = new Date(now.value)
  return d.getHours() * 60 + d.getMinutes()
})

/** 行 = 节次。骨架来自整学期（st.periods），不是"这一周的课" —— 见 types 里那条注释 */
const periods = computed(() => st.value?.periods ?? [])

const at = (day: number, start: number): ClassRow[] =>
  rows.value.filter((r) => r.day === day && r.startMin === start)

/** 这一列是不是"此刻"：今天 + 当前时间落在某节课的区间里 */
const liveStart = computed(() => {
  if (!st.value?.currentWeek || viewing.value !== st.value.currentWeek) return null
  const hit = rows.value.find((r) => r.day === todayIdx.value && r.startMin <= nowMin.value && nowMin.value < r.endMin)
  return hit ? hit.startMin : null
})

const todayGap = computed(() => {
  const todays = rows.value.filter((r) => r.day === todayIdx.value)
  if (!todays.length) return null
  return nextFreeGap(todays, nowMin.value)
})

/** 第 N 周周一的日期，用来给每列标日号 */
const weekDates = computed(() => {
  const base = st.value?.week1Monday
  const w = viewing.value ?? 1
  if (!base) return []
  const [y, m, d] = base.split('-').map(Number)
  const monday = new Date(y, m - 1, d + (w - 1) * 7)
  return DAY_NAMES.map((_, i) => {
    const day = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + i)
    return `${day.getMonth() + 1}/${day.getDate()}`
  })
})

const progress = computed(() => {
  const s = st.value
  if (!s?.currentWeek || !s.totalWeeks) return null
  return Math.round((s.currentWeek / s.totalWeeks) * 100)
})

function step(by: number) {
  const next = (viewing.value ?? 1) + by
  const total = st.value?.totalWeeks ?? 30
  viewing.value = Math.min(Math.max(next, 1), total)
  void load()
}

async function pick() {
  busy.value = true
  error.value = null
  notice.value = null
  try {
    const report: TimetableImportReport | null = await api.timetableImport()
    if (report) {
      notice.value = `已导入 ${report.term}：${report.courses} 门课 / ${report.rows} 个上课时段`
      viewing.value = null
      await load()
      if (report.needsWeek1) notice.value += '；还差开学日，填一下才能算出今天第几周'
    }
  } catch (e) {
    // 编码不对、格子形状不对都在这里现原话 —— 这两条提示都是能照着做完的
    error.value = errText(e)
  } finally {
    busy.value = false
  }
}

async function saveWeek1() {
  error.value = null
  try {
    st.value = await api.timetableSetWeek1(week1Draft.value)
    notice.value = week1Draft.value ? `已设定开学日，今天是第 ${st.value.currentWeek ?? '—'} 周` : '已清除开学日'
    viewing.value = null
    await load()
  } catch (e) {
    error.value = errText(e)
  }
}

const selected = computed(() => (shell.selection.value?.kind === 'class' ? shell.selection.value.cls : null))

function open(cls: ClassRow) {
  shell.select({ kind: 'class', cls, week: viewing.value ?? 1 })
}

/** 空网格时给个占位：没有课表数据就别画一张假表 */
const hasData = computed(() => (st.value?.rows ?? 0) > 0)
</script>

<template>
  <section class="y-view y-tt">
    <div class="bar">
      <h2>课表 <span class="sub">教务导入的副本</span></h2>
      <div v-if="hasData" class="wk">
        <button type="button" class="y-ghost nav" title="上一周" :disabled="(viewing ?? 1) <= 1" @click="step(-1)">
          <ChevronLeft :size="13" />
        </button>
        <button
          type="button"
          class="y-ghost wk-cur"
          :title="st?.currentWeek ? '回到本周' : '还没设开学日'"
          :disabled="!st?.currentWeek"
          @click="viewing = st?.currentWeek ?? null; load()"
        >
          <CalendarRange :size="12" />
          第 {{ viewing ?? '—' }} 周<span v-if="weekDates.length"> · {{ weekDates[0] }}–{{ weekDates[6] }}</span>
        </button>
        <button
          type="button"
          class="y-ghost nav"
          title="下一周"
          :disabled="(viewing ?? 1) >= (st?.totalWeeks ?? 1)"
          @click="step(1)"
        >
          <ChevronRight :size="13" />
        </button>
      </div>
      <span v-if="st?.totalWeeks" class="prog">学期 {{ st.currentWeek ?? '—' }}/{{ st.totalWeeks }} 周</span>
      <span v-if="progress !== null" class="pbar"><i :style="{ width: progress + '%' }"></i></span>
      <button type="button" class="y-ghost btn" :disabled="busy" @click="pick">
        <Upload :size="13" />
        {{ busy ? '正在读取…' : '导入 CSV' }}
      </button>
    </div>

    <CollapsedRow
      v-if="!hasData"
      label="课表 · 还没有导入过"
      detail="教务导出 → Excel 另存为「CSV UTF-8(逗号分隔)」→ 点右上「导入 CSV」"
      :busy="busy"
      @retry="pick()"
    />

    <div v-if="st && !st.week1Monday && hasData" class="need">
      <span>第 1 周的周一是哪天？</span>
      <input v-model="week1Draft" type="date" />
      <button type="button" class="y-ghost btn" @click="saveWeek1">保存</button>
      <em>CSV 里没有开学日，算不出"今天第几周"</em>
    </div>

    <p v-if="notice" class="ok">{{ notice }}</p>
    <p v-if="error" class="bad">{{ error }}</p>

    <div v-if="hasData" class="gridwrap scroller">
      <table>
        <thead>
          <tr>
            <th class="hour">节次</th>
            <th v-for="(d, i) in DAY_NAMES" :key="d" :class="{ today: i + 1 === todayIdx }">
              {{ d }}<span v-if="weekDates[i]" class="dt">{{ weekDates[i] }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in periods" :key="p.startMin">
            <th class="hour"><b>{{ p.label }}</b></th>
            <td v-for="(d, i) in DAY_NAMES" :key="d" :class="{ today: i + 1 === todayIdx, free: !at(i + 1, p.startMin).length }">
              <button
                v-for="c in at(i + 1, p.startMin)"
                :key="`${c.name}-${c.weeks}`"
                type="button"
                class="cls"
                :class="{ live: i + 1 === todayIdx && liveStart === p.startMin, sel: selected === c }"
                @click="open(c)"
              >
                <span class="n">{{ c.name }}</span>
                <span class="r">{{ c.room }}</span>
                <span class="w">{{ c.teacher }} · {{ c.weeks }}周</span>
              </button>
              <span v-if="!at(i + 1, p.startMin).length" class="f">空闲</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <p v-if="hasData" class="legend">
      {{ st?.courses ?? 0 }} 门课 · 本周 {{ rows.length }} 个时段
      <template v-if="todayGap"> · 最近空档 {{ hhmm(todayGap[0]) }}–{{ hhmm(todayGap[1]) }}</template>
      <template v-else-if="rows.some((r) => r.day === todayIdx)"> · 今天没有 ≥{{ MIN_GAP_MINUTES }} 分钟的空档</template>
      <em>点一格看教师、教室与周次</em>
    </p>
  </section>
</template>

<style scoped>
.y-tt {
  display: flex;
  flex-direction: column;
  gap: 11px;
  min-height: 0;
  flex: 1;
  overflow: hidden;
}
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
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
.wk {
  display: flex;
  align-items: center;
  gap: 4px;
}
.nav {
  width: 26px;
  height: 26px;
  padding: 0;
  display: grid;
  place-items: center;
}
.wk-cur {
  white-space: nowrap;
  height: 26px;
  width: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  font-weight: 700;
}
.prog,
.legend {
  font-size: 11px;
  color: var(--t3);
}
.legend {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.legend em {
  font-style: normal;
  margin-left: auto;
  color: var(--t3);
  opacity: 0.8;
}
.pbar {
  width: 92px;
  height: 4px;
  border-radius: 999px;
  background: var(--card-2);
  overflow: hidden;
}
.pbar i {
  display: block;
  height: 100%;
  background: var(--accent);
}
.btn {
  margin-left: auto;
  flex: none;
  height: 27px;
  width: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.need {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  font-size: 11.5px;
  color: var(--t2);
  background: color-mix(in srgb, var(--amber) 12%, transparent);
  border-radius: var(--r-sm);
  padding: 8px 11px;
}
.need input {
  height: 27px;
  padding: 0 8px;
  border-radius: var(--r-xs);
  border: 1px solid var(--line-2);
  background: var(--card);
  color: var(--t1);
  font-size: 12px;
  color-scheme: dark;
}
[data-mode='light'] .need input {
  color-scheme: light;
}
.need em {
  font-style: normal;
  color: var(--t3);
  font-size: 10.5px;
}
.ok,
.bad {
  font-size: 11px;
  line-height: 1.5;
  border-radius: var(--r-sm);
  padding: 7px 10px;
}
.ok {
  color: var(--t2);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
}
.bad {
  color: var(--t1);
  background: color-mix(in srgb, var(--red) 12%, transparent);
}

.gridwrap {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--line);
  border-radius: var(--r);
  background: var(--card);
}
table {
  border-collapse: separate;
  border-spacing: 0;
  width: 100%;
  min-width: 720px;
  table-layout: fixed;
}
th,
td {
  border-right: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
  vertical-align: top;
}
thead th {
  position: sticky;
  top: 0;
  z-index: 3;
  background: var(--card-2);
  padding: 7px 6px;
  font-size: 11.5px;
  font-weight: 750;
  color: var(--t2);
  text-align: center;
}
thead th.today {
  background: var(--accent-soft);
  color: var(--t1);
}
thead th .dt {
  display: block;
  font-size: 10px;
  color: var(--t3);
  font-weight: 600;
}
th.hour {
  position: sticky;
  left: 0;
  z-index: 4;
  background: var(--card-2);
  width: 78px;
  text-align: left;
  padding: 7px 8px;
  font-size: 10.5px;
  color: var(--t3);
  font-weight: 700;
  line-height: 1.35;
}
td {
  padding: 4px;
  height: 62px;
}
td.today {
  background: color-mix(in srgb, var(--accent) 4%, transparent);
}
td.free .f {
  font-size: 10.5px;
  color: var(--t3);
  opacity: 0.5;
  padding: 3px 5px;
}
.cls {
  display: block;
  width: 100%;
  border-radius: var(--r-xs);
  padding: 6px 7px;
  text-align: left;
  background: var(--card-2);
  border: 1px solid var(--line);
  margin-bottom: 4px;
  cursor: pointer;
}
.cls .n {
  display: -webkit-box;
  font-size: 12px;
  font-weight: 750;
  line-height: 1.25;
  color: var(--t1);
  /* 1400 宽下每列只有约 110px，长课名必须折行而不是被截成半个词 */
  overflow: hidden;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.cls .r,
.cls .w {
  display: block;
  font-size: 10.5px;
  color: var(--t2);
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cls .w {
  color: var(--t3);
}
/* 「此刻」= 描边，「选中」= 左色条：真数据上它们会撞在同一格，必须两种语言 */
.cls.live {
  border-color: var(--accent);
  box-shadow: inset 0 0 0 1px var(--accent);
}
/* 选中 = 左色条 + 抬一档底色；此刻 = 描边。同一格可以既是此刻又被选中，两种语言叠着显示 */
.cls.sel {
  border-left: 3px solid var(--accent);
  background: var(--accent-soft);
}
</style>
