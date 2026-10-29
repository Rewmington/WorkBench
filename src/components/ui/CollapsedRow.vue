<script setup lang="ts">
import { RefreshCw } from 'lucide-vue-next'

import { useClock } from '../../composables/useClock'
import { relTime } from '../../lib/reltime'

/**
 * 空 / 不可用的部件折叠成一行虚线条，不留大块死白（PLAN §8.3.3）。
 * 拿不到就是拿不到：写清楚原因与最后一次成功的时间，不转圈、不伪装已同步。
 */
defineProps<{
  label: string
  detail?: string
  fetchedAt?: number | null
  busy?: boolean
  /** 「重试」对某些空态没有意义（比如隐藏的部件、没到期的环），关掉它换 action 插槽。 */
  retryable?: boolean
}>()

const emit = defineEmits<{ retry: [] }>()
const { now } = useClock()
</script>

<template>
  <div class="y-empty collapsed">
    <span class="lb">{{ label }}</span>
    <span v-if="detail" class="dt">{{ detail }}</span>
    <span v-if="fetchedAt" class="ag">最后成功于 {{ relTime(now, fetchedAt) }}</span>
    <span class="act"><slot name="action" /></span>
    <button v-if="retryable !== false" type="button" class="rt" :disabled="busy" :title="'再试一次'" @click="emit('retry')">
      <RefreshCw :size="12" :class="{ spin: busy }" />
    </button>
  </div>
</template>

<style scoped>
.collapsed {
  gap: 8px;
  padding: 10px 13px;
}
.lb {
  color: var(--t2);
  font-weight: 650;
}
.dt,
.ag {
  color: var(--t3);
  font-size: 11px;
}
.act {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 6px;
}
.rt {
  color: var(--t3);
  display: grid;
  place-items: center;
}
.rt:hover {
  color: var(--t1);
}
.rt:disabled {
  opacity: 0.5;
}
.spin {
  animation: turn 1.2s linear infinite;
}
@keyframes turn {
  to {
    transform: rotate(360deg);
  }
}
</style>
