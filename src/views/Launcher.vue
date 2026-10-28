<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { Plus } from 'lucide-vue-next'

import { api, usingMock } from '../api'
import type { Resource, Zone } from '../api/types'
import Tile from '../components/launch/Tile.vue'
import { useShell } from '../composables/useShell'

const shell = useShell()

const resources = ref<Resource[]>([])
const zones = ref<Zone[]>([])
const activeZone = ref<string>('all')
const error = ref<string | null>(null)
const dropping = ref(false)
const webForm = ref<{ name: string; url: string } | null>(null)

let unlistenDrop: (() => void) | null = null

async function reload() {
  resources.value = await api.resourcesList()
  zones.value = await api.zonesList()
  shell.setCount('launcher', resources.value.length)
}

onMounted(async () => {
  await reload()
  if (!usingMock) {
    // 从 Explorer 直接拖 exe / lnk 进来 —— 速达的核心交互，不用文件对话框绕一圈
    const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    const stop = await getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'over' || event.payload.type === 'enter') {
        dropping.value = true
      } else if (event.payload.type === 'leave') {
        dropping.value = false
      } else if (event.payload.type === 'drop') {
        dropping.value = false
        void addPaths(event.payload.paths)
      }
    })
    unlistenDrop = () => stop()
  }
})

onBeforeUnmount(() => unlistenDrop?.())

async function addPaths(paths: string[]) {
  const zoneId = activeZone.value === 'all' || activeZone.value === 'none' ? null : activeZone.value
  for (const path of paths) {
    try {
      await api.resourceAddFromPath(path, zoneId)
    } catch (e) {
      error.value = String(e)
    }
  }
  await reload()
}

async function pickOne() {
  error.value = null
  const zoneId = activeZone.value === 'all' || activeZone.value === 'none' ? null : activeZone.value
  const added = await api.resourcePick(zoneId)
  if (added) await reload()
}

async function addWeb() {
  const form = webForm.value
  if (!form) return
  const url = form.url.trim()
  const name = form.name.trim() || url
  if (!url) {
    webForm.value = null
    return
  }
  const zoneId = activeZone.value === 'all' || activeZone.value === 'none' ? null : activeZone.value
  try {
    await api.resourceAddWeb(name, url, zoneId)
    webForm.value = null
    await reload()
  } catch (e) {
    error.value = String(e)
  }
}

async function launch(r: Resource) {
  error.value = null
  try {
    await api.resourceLaunch(r.id)
    await reload()
  } catch (e) {
    // 目标被移走是常见事：一行说明 + 磁贴还在，不弹对话框
    error.value = String(e)
  }
}

async function rename(r: Resource, name: string) {
  await api.resourceRename(r.id, name)
  await reload()
}

async function remove(r: Resource) {
  await api.resourceDelete(r.id)
  await reload()
}

const zoneDraft = ref<string | null>(null)

async function addZone() {
  const name = (zoneDraft.value ?? '').trim()
  zoneDraft.value = null
  if (!name) return
  await api.zoneAdd(name)
  await reload()
}

const filtered = computed(() => {
  if (activeZone.value === 'all') return resources.value
  if (activeZone.value === 'none') return resources.value.filter((r) => !r.zoneId)
  return resources.value.filter((r) => r.zoneId === activeZone.value)
})

const zoneChips = computed(() => [
  { id: 'all', name: '全部', count: resources.value.length },
  ...zones.value.map((z) => ({ id: z.id, name: z.name, count: z.itemCount })),
  { id: 'none', name: '未分区', count: resources.value.filter((r) => !r.zoneId).length },
])
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <div class="chips scroller">
        <button
          v-for="chip in zoneChips"
          :key="chip.id"
          type="button"
          class="chip"
          :class="{ on: activeZone === chip.id }"
          @click="activeZone = chip.id"
        >
          {{ chip.name }}<span v-if="chip.count" class="c">{{ chip.count }}</span>
        </button>
        <input
          v-if="zoneDraft !== null"
          v-model="zoneDraft"
          class="palette-query zinput"
          placeholder="分区名，回车"
          @keydown.enter.prevent="addZone"
          @keydown.esc.prevent="zoneDraft = null"
          @blur="addZone"
        />
        <button v-else type="button" class="chip add" title="新建分区" @click="zoneDraft = ''">
          <Plus :size="12" />
        </button>
      </div>
      <div class="acts">
        <button type="button" class="y-ghost" @click="pickOne">选择程序…</button>
        <button type="button" class="y-ghost" @click="webForm = { name: '', url: '' }">加网页</button>
      </div>
    </div>

    <div v-if="webForm" class="webform">
      <input v-model="webForm.name" class="palette-query" placeholder="名字，比如「博客」" />
      <input v-model="webForm.url" class="palette-query" placeholder="https://…" @keydown.enter.prevent="addWeb" />
      <button type="button" class="y-solid" @click="addWeb">加上</button>
      <button type="button" class="y-ghost" @click="webForm = null">取消</button>
    </div>

    <p v-if="error" class="err">{{ error }}</p>

    <div class="grid scroller" :class="{ dropping }" @dragover.prevent @drop.prevent>
      <Tile
        v-for="r in filtered"
        :key="r.id"
        :resource="r"
        @launch="launch(r)"
        @rename="(name: string) => rename(r, name)"
        @remove="remove(r)"
        @select="shell.select({ kind: 'resource', id: r.id })"
      />
      <div v-if="!filtered.length" class="y-empty full">
        还没有条目。把 exe 或快捷方式直接拖进这个窗口就行 —— 图标会自动提取。
      </div>
    </div>

    <p class="hint">拖进来即可添加；已运行的程序会直接把窗口带到前台，而不是再开一个。</p>
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
  flex: 1;
}
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
.chips {
  display: flex;
  gap: 5px;
  overflow-x: auto;
  flex: 1;
  min-width: 0;
  padding-bottom: 2px;
}
.chip {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 10px;
  border-radius: 8px;
  border: 1px solid var(--line);
  background: var(--card-2);
  color: var(--t2);
  font-size: 11.5px;
  font-weight: 650;
}
.chip.on {
  background: var(--accent-soft);
  border-color: color-mix(in srgb, var(--accent) 26%, transparent);
  color: var(--accent);
  font-weight: 800;
}
.chip .c {
  font-size: 10px;
  opacity: 0.75;
  font-variant-numeric: tabular-nums;
}
.chip.add {
  padding: 5px 7px;
}
.zinput {
  flex: none;
  width: 108px;
  padding: 4px 8px;
  border-radius: 8px;
  border: 1px solid var(--line-2);
  background: var(--card);
  color: var(--t1);
  font-size: 11.5px;
  outline: none;
}
.acts {
  display: flex;
  gap: 6px;
  flex: none;
}
.acts .y-ghost {
  flex: none;
  padding: 0 11px;
  height: 28px;
}
.webform {
  display: flex;
  gap: 6px;
  align-items: center;
}
.webform input {
  padding: 6px 9px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--t1);
  font-size: 12px;
  outline: none;
  min-width: 0;
}
.webform .y-solid,
.webform .y-ghost {
  flex: none;
  padding: 0 12px;
  height: 28px;
}
.grid {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  gap: 9px;
  align-content: start;
  padding: 3px;
  border-radius: var(--r);
  border: 1px dashed transparent;
}
.grid.dropping {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.full {
  grid-column: 1 / -1;
}
.err {
  font-size: 11.5px;
  color: var(--red);
}
.hint {
  font-size: 11px;
  color: var(--t3);
}
</style>
