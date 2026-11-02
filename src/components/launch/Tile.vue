<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'

import type { Resource } from '../../api/types'
import { assetUrl } from '../../lib/assets'

const props = defineProps<{ resource: Resource; running?: boolean }>()
const emit = defineEmits<{ launch: []; remove: []; rename: [string]; select: [] }>()

const renaming = ref(false)
const draft = ref('')

/**
 * 移除要两步内联确认：那个 × 只有 17px，误点一下就少一个磁贴，
 * 而数据层虽然能恢复（`resources.deleted_at` 墓碑），界面上却没有回收站。
 * 节奏与笔记页「彻底删除」那条一致（5 秒不点第二下就撤回）。
 */
const confirmingRemove = ref(false)
let confirmTimer: number | undefined

function askRemove() {
  if (confirmingRemove.value) {
    window.clearTimeout(confirmTimer)
    confirmingRemove.value = false
    emit('remove')
    return
  }
  confirmingRemove.value = true
  confirmTimer = window.setTimeout(() => {
    confirmingRemove.value = false
  }, 5000)
}

function startRename() {
  draft.value = props.resource.name
  renaming.value = true
}

function commitRename() {
  renaming.value = false
  const next = draft.value.trim()
  if (next && next !== props.resource.name) emit('rename', next)
}

// 等确认期间磁贴可能被删掉/换页：定时器留在外面继续跑，是在改一个已经没了的实例
onBeforeUnmount(() => window.clearTimeout(confirmTimer))

/**
 * 图标走 asset 协议读缓存的 PNG；提不到图标就用首字母 —— 缺一张图不该让功能缺一次。
 */
const iconUrl = computed(() => assetUrl(props.resource.iconPath))

const letter = computed(() => (props.resource.name.trim()[0] ?? '?').toUpperCase())
const kindHint = computed(() => (props.resource.kind === 'web' ? '网页' : props.resource.kind === 'app' ? '应用' : '文件'))
</script>

<template>
  <!-- 磁贴里有输入框，所以根不能是 <button>（button 里不许再放交互元素）；改成 div + role -->
  <div
    class="tile"
    role="button"
    tabindex="0"
    :title="resource.target"
    @click="emit('launch')"
    @keydown.enter.prevent="emit('launch')"
    @contextmenu.prevent="emit('select')"
  >
    <span class="ic">
      <img v-if="iconUrl" :src="iconUrl" alt="" width="34" height="34" />
      <span v-else class="lt">{{ letter }}</span>
    </span>
    <input
      v-if="renaming"
      v-model="draft"
      class="palette-query rn"
      aria-label="改名"
      @click.stop
      @keydown.enter.prevent="commitRename"
      @keydown.esc.prevent="renaming = false"
      @blur="commitRename"
    />
    <span v-else class="nm">{{ resource.name }}</span>
    <span class="mt">
      <i v-if="running" class="run" title="正在运行"></i>
      {{ kindHint }}
      <template v-if="resource.useCount"> · 用过 {{ resource.useCount }} 次</template>
    </span>
    <span class="tools" :class="{ shown: confirmingRemove }">
      <span class="t" role="button" tabindex="0" title="改名" @click.stop="startRename">A</span>
      <span
        class="t"
        :class="{ armed: confirmingRemove }"
        role="button"
        tabindex="0"
        :title="confirmingRemove ? '再点一次真的移除' : '移除'"
        @click.stop="askRemove"
        @keydown.enter.prevent="askRemove"
      >{{ confirmingRemove ? '?' : '×' }}</span>
    </span>
  </div>
</template>

<style scoped>
.rn {
  width: 100%;
  min-width: 0;
  font: inherit;
  font-size: 12px;
  text-align: center;
  border: 1px solid var(--line-2);
  border-radius: 6px;
  background: var(--card-2);
  color: var(--t1);
  padding: 2px 4px;
  outline: none;
}
.tile {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 13px 9px 11px;
  border-radius: 12px;
  border: 1px solid var(--line);
  background: var(--card);
  text-align: center;
  transition:
    border-color var(--dur-1) var(--ease),
    background var(--dur-1) var(--ease);
}
.tile:hover {
  border-color: var(--line-2);
  background: var(--card-2);
}
.ic {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  background: var(--card-2);
  border: 1px solid var(--line);
  overflow: hidden;
}
.lt {
  font-size: 17px;
  font-weight: 800;
  color: var(--t2);
}
.nm {
  font-size: 12px;
  font-weight: 650;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mt {
  font-size: 10px;
  color: var(--t3);
  display: flex;
  align-items: center;
  gap: 4px;
}
.run {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.tools {
  position: absolute;
  top: 5px;
  right: 5px;
  display: none;
  gap: 3px;
}
.tile:hover .tools {
  display: flex;
}
/* 等确认的那一下必须一直看得见：这一组按钮只在 hover 时才出现，
   指针一移开就看不见「armed」，用户回来再点一次会直接删掉。 */
.tools.shown {
  display: flex;
}
.t {
  width: 17px;
  height: 17px;
  border-radius: 5px;
  background: var(--panel);
  border: 1px solid var(--line-2);
  color: var(--t2);
  font-size: 10px;
  display: grid;
  place-items: center;
  cursor: pointer;
}
.t:hover {
  color: var(--t1);
}
/* 第二步的样子照笔记页「彻底删除」那一条：同一个 ×，armed 才变红 */
.t.armed {
  color: var(--red);
  background: color-mix(in srgb, var(--red) 14%, transparent);
  border-color: color-mix(in srgb, var(--red) 40%, transparent);
}
</style>
