<script setup lang="ts">
import { Sparkles } from 'lucide-vue-next'
</script>

<template>
  <!-- 常驻详情面板（§8.1.1）：x-hub 全靠弹窗，选中条目看不到全貌，这里反过来 -->
  <section class="y-detail">
    <header class="y-dh">
      <Sparkles :size="14" />
      <span class="t">详情</span>
    </header>
    <div class="y-db scroller">
      <div class="y-empty">
        选中任何待办、笔记或联动条目，这里显示它的全貌与操作。空的时候不占地方。
      </div>
    </div>
  </section>
</template>

<style scoped>
.y-dh {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 13px 14px 10px;
  color: var(--accent);
  flex: none;
}
.y-dh .t {
  font-size: 12.5px;
  font-weight: 800;
  color: var(--t1);
}
.y-db {
  flex: 1;
  min-height: 0;
  padding: 0 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
