<script setup lang="ts">
import { computed } from 'vue'

import type { TodayCounts } from '../../api/types'

/**
 * 今日构成 = gauge 不是 chart（§7）：中心放数字，环最多两段。
 *
 * 分母不是「目标数」—— 这个应用里没有目标数这个东西，编一个出来就是假指标。
 * 所以两段是「今天做掉的」与「今天还剩的」，加起来等于驾驶舱 TODAY + DONE 两张卡，
 * 因为它们读的是同一条 SQL。
 */
const props = defineProps<{ counts: TodayCounts }>()

const R = 34
const C = 2 * Math.PI * R

const total = computed(() => props.counts.open + props.counts.done)
const fraction = computed(() => (total.value > 0 ? props.counts.done / total.value : 0))
const dash = computed(() => `${(fraction.value * C).toFixed(2)} ${C.toFixed(2)}`)
</script>

<template>
  <div class="ring">
    <svg viewBox="0 0 88 88" width="88" height="88" aria-hidden="true">
      <circle class="track" :cx="44" :cy="44" :r="R" :stroke-width="9" fill="none" />
      <circle
        class="arc"
        :cx="44"
        :cy="44"
        :r="R"
        :stroke-width="9"
        :stroke-dasharray="dash"
        fill="none"
        stroke-linecap="butt"
        transform="rotate(-90 44 44)"
      />
    </svg>
    <div class="mid">
      <div class="num">{{ counts.done }}</div>
      <div class="cap">已清</div>
    </div>
  </div>
  <p class="foot">
    <template v-if="total === 0">今天没有到期的事</template>
    <template v-else-if="counts.open === 0">全清完了</template>
    <template v-else>还剩 <b>{{ counts.open }}</b> 件</template>
  </p>
</template>

<style scoped>
.ring {
  position: relative;
  width: 88px;
  height: 88px;
  flex: none;
}
.track {
  stroke: var(--card-2);
}
.arc {
  stroke: var(--accent);
}
.mid {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  text-align: center;
}
.num {
  font-size: 24px;
  font-weight: 800;
  line-height: 1;
  font-variant-numeric: tabular-nums;
}
.cap {
  font-size: 9px;
  font-weight: 800;
  letter-spacing: 0.1em;
  color: var(--t3);
  margin-top: 3px;
}
.foot {
  margin: 8px 0 0;
  font-size: 11px;
  color: var(--t3);
}
.foot b {
  color: var(--t1);
  font-weight: 800;
  font-variant-numeric: tabular-nums;
}
</style>
