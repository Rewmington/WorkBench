<script setup lang="ts">
import { Minus, Square, X } from 'lucide-vue-next'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { usingMock } from '../../api'

// 浏览器里跑 mock 验 UI 时没有真窗口，按钮整块不出现（不是禁用——禁用会让人以为功能坏了）
const win = usingMock ? null : getCurrentWindow()

async function minimize() {
  await win?.minimize()
}

async function toggleMax() {
  await win?.toggleMaximize()
}

async function close() {
  // 交给窗口的 CloseRequested：收进托盘而不是退出（§8.6 + PLAN §4 托盘驻留）
  await win?.close()
}
</script>

<template>
  <div class="y-tbar" data-tauri-drag-region @dblclick="toggleMax">
    <div class="brand">
      <span class="b">A</span>
      <span>Anvil · 工作台</span>
    </div>

    <div v-if="win" class="winbtns">
      <button type="button" title="最小化" @click="minimize">
        <Minus :size="14" />
      </button>
      <button type="button" title="最大化 / 还原" @click="toggleMax">
        <Square :size="12" />
      </button>
      <button type="button" class="close" title="收进托盘" @click="close">
        <X :size="14" />
      </button>
    </div>
  </div>
</template>

<style scoped>
/* 按钮自己不能当拖拽区，否则点按钮会先触发 startDragging */
.winbtns {
  -webkit-user-select: none;
  user-select: none;
}
</style>
