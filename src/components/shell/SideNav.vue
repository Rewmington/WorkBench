<script setup lang="ts">
import { computed, onMounted } from 'vue'

import { useBackup } from '../../composables/useBackup'
import { useClock } from '../../composables/useClock'
import { NAV, useShell } from '../../composables/useShell'
import { relTime } from '../../lib/reltime'

const { view, counts, go } = useShell()
const backup = useBackup()
const { now } = useClock()

/**
 * 上次成功时间只有一个来源：backup_log（useBackup 的 status）。
 * 早先这里读 settings 的 backup.lastSuccessAt，那个键从没被写过，于是页面说「上次成功 4 分钟前」、
 * 侧栏同时说「未配置」—— 两张脸对不上，正是原则 5 要避免的。
 *
 * 绝对路径不在这块常驻卡上了（§8.9：那是诊断，不是状态）—— 它在设置页「数据与诊断」里，
 * 完整值、可复制，比这里截尾显示更好使。
 */
const backupCard = computed(() => {
  const st = backup.status.value
  if (!st?.configured) return { v: '未配置', sub: '每天自动快照到坚果云' }
  if (!st.lastSuccessAt) return { v: '还没成功过', sub: '云端还没有第一份快照' }
  return { v: relTime(now.value, st.lastSuccessAt), sub: null }
})

onMounted(() => {
  if (!backup.status.value) void backup.load()
})
</script>

<template>
  <aside class="y-side">
    <div class="y-logo">
      <span class="b">A</span>
      <span class="n">Anvil</span>
      <span class="s">LOCAL FIRST</span>
    </div>

    <nav class="y-nav scroller">
      <template v-for="(group, gi) in NAV" :key="gi">
        <div v-if="group.title" class="grp">{{ group.title }}</div>
        <button
          v-for="item in group.items"
          :key="item.key"
          type="button"
          class="it"
          :aria-current="view === item.view ? 'page' : undefined"
          @click="go(item.view)"
        >
          <component :is="item.icon" :size="15" />
          <span>{{ item.label }}</span>
          <span v-if="counts[item.key] != null" class="n">{{ counts[item.key] }}</span>
        </button>
      </template>
    </nav>

    <!-- 一张状态卡：备份这件事的最近事实。诊断（绝对路径）在设置页，不常驻在这儿（§8.9） -->
    <div class="y-sidefoot">
      <div class="y-picker">
        <span class="k">坚果云备份</span>
        <span class="v">{{ backupCard.v }}</span>
        <span v-if="backupCard.sub" class="sub">{{ backupCard.sub }}</span>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.y-logo {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px 12px;
}
.y-logo .b {
  width: 26px;
  height: 26px;
  border-radius: 7px;
  display: grid;
  place-items: center;
  background: linear-gradient(150deg, var(--accent), #0a7a45);
  color: var(--accent-ink);
  font-size: 13px;
  font-weight: 800;
}
.y-logo .n {
  font-size: 15px;
  font-weight: 800;
  letter-spacing: -0.02em;
}
.y-logo .s {
  margin-left: auto;
  font-size: 9px;
  font-weight: 800;
  letter-spacing: 0.08em;
  color: var(--t3);
}
.y-nav {
  display: flex;
  flex-direction: column;
  gap: 1px;
  overflow-y: auto;
  min-height: 0;
}
.y-picker .v {
  display: block;
  word-break: break-all;
}
.y-picker .sub {
  display: block;
  margin-top: 3px;
  font-size: 10.5px;
  color: var(--t3);
}
</style>
