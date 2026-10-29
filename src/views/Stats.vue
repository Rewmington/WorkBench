<script setup lang="ts">
import { onMounted } from 'vue'

import CollapsedRow from '../components/ui/CollapsedRow.vue'
import DayBars from '../components/stats/DayBars.vue'
import HeatGrid from '../components/stats/HeatGrid.vue'
import HourHist from '../components/stats/HourHist.vue'
import TagBars from '../components/stats/TagBars.vue'
import TodayRing from '../components/stats/TodayRing.vue'
import { useShell } from '../composables/useShell'
import { useStats } from '../composables/useStats'

/**
 * 统计视图（PLAN §7）。独立一页而不是塞进工作台：统计是回顾性的，
 * 工作台是行动性的，混在一起会让首页变成仪表盘。
 *
 * 五张图一次取数、各说自己的窗口。数据不足时出空态并写明门槛，
 * 不拿两周噪声画一条上扬曲线。
 */
const { report, error, load } = useStats()
const shell = useShell()

onMounted(load)

/** 热力图与柱状图点某一天：复用右栏常驻详情面板看那天的明细（§8.8.5），不另做弹窗。 */
function pickDay(day: number) {
  if (!report.value || day > report.value.todayDay) return
  shell.select({ kind: 'day', day })
}
</script>

<template>
  <section class="y-view stats">
    <div class="bar">
      <h2>
        完成与新建
        <span class="sub">
          <template v-if="report">记了 {{ report.historyDays }} 天 · 全部来自本机 SQLite</template>
          <template v-else>正在算</template>
        </span>
      </h2>
    </div>

    <p v-if="error" class="err">统计读不出来：{{ error }}</p>

    <div v-if="!report && !error" class="cards">
      <div v-for="i in 5" :key="i" class="card">
        <div class="skel">
          <div v-for="j in 3" :key="j" class="sk" :style="{ width: 90 - j * 13 + '%' }"></div>
        </div>
      </div>
    </div>

    <div v-else-if="report" class="cards">
      <div class="card ring">
        <div class="h">
          <h3>今天</h3>
          <span class="win">今天 00:00 起</span>
        </div>
        <TodayRing :counts="report.gauge" />
        <p class="why">环只有两段：做掉的与还剩的。没有「目标数」这一说 —— 这个应用里没人定过目标。</p>
      </div>

      <div class="card heat">
        <div class="h">
          <h3>完成热力图</h3>
          <span class="win">近 15 周</span>
        </div>
        <HeatGrid
          v-if="report.hasActivity"
          :cells="report.heatmap.cells"
          :today-day="report.todayDay"
          :streak="report.heatmap.streak"
          :streak-through="report.heatmap.streakThrough"
          @pick="pickDay"
        />
        <CollapsedRow v-else label="还没有完成记录" detail="去待办勾掉一条，这里才会开始有形状" />
        <p class="why">看的是断点，不是总量 —— 柱状图给形状，给不出「有没有连着做」。</p>
      </div>

      <div class="card bars">
        <div class="h">
          <h3>每日完成</h3>
          <span class="win">近 30 天 · 堆叠区分按时与补做</span>
        </div>
        <DayBars v-if="report.hasActivity" :bars="report.bars" :today-day="report.todayDay" @pick="pickDay" />
        <CollapsedRow v-else label="近 30 天没有完成记录" detail="这一页要的是回顾，不是预测" />
        <p class="why">「补做」那截是前一天就该做完的。它变高，说明账在往后堆。</p>
      </div>

      <div class="card tags">
        <div class="h">
          <h3>标签上的完成量</h3>
          <span class="win">近 90 天 · 前 8</span>
        </div>
        <TagBars v-if="report.hasActivity" :tags="report.tags" :total="report.tagsTotal" />
        <CollapsedRow v-else label="还没有完成记录" detail="给待办挂个标签，这张图才有得可分" />
      </div>

      <div class="card hours">
        <div class="h">
          <h3>什么时候在建待办</h3>
          <span class="win">全部历史 · 按小时</span>
        </div>
        <HourHist
          :hours="report.hours"
          :enough="report.hoursEnough"
          :history-days="report.historyDays"
          :min-days="report.hourMinDays"
        />
      </div>
    </div>
  </section>
</template>

<style scoped>
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
.err {
  margin: 10px 0 0;
  font-size: 11.5px;
  color: var(--red);
}
.cards {
  display: grid;
  grid-template-columns: 226px minmax(0, 1fr);
  gap: 10px;
  margin-top: 10px;
  align-items: start;
}
.ring {
  grid-column: 1;
  grid-row: 1;
}
.heat {
  grid-column: 2;
  grid-row: 1;
}
.bars {
  grid-column: 1 / -1;
}
.tags {
  grid-column: 1;
}
.hours {
  grid-column: 2;
}
.card {
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: var(--r-sm);
  padding: 11px 12px 12px;
  min-width: 0;
}
.h {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 9px;
}
h3 {
  margin: 0;
  font-size: 12px;
  font-weight: 750;
  color: var(--t2);
}
.win {
  font-size: 10px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.why {
  margin: 8px 0 0;
  font-size: 10.5px;
  line-height: 1.6;
  color: var(--t3);
}
.skel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 0;
}
.sk {
  height: 9px;
  border-radius: 5px;
  background: var(--card-2);
}
@media (max-width: 1080px) {
  .cards {
    grid-template-columns: minmax(0, 1fr);
  }
  .ring,
  .heat,
  .bars,
  .tags,
  .hours {
    grid-column: 1;
    grid-row: auto;
  }
}
</style>
