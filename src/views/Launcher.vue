<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { Plus } from 'lucide-vue-next'

import { api, usingMock } from '../api'
import type { Resource, Zone } from '../api/types'
import Tile from '../components/launch/Tile.vue'
import { useShell } from '../composables/useShell'
import { errText } from '../lib/err'

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
  // 分区清单读不出来时说出来：静默变成「只有全部/未分区两颗」看起来就像分区被人删光了
  // （真窗口里撞过一条：库里 sort 列类型不对，zones_list 直接报错）。
  try {
    zones.value = await api.zonesList()
    error.value = null
  } catch (e) {
    zones.value = []
    error.value = errText(e)
  }
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

onBeforeUnmount(() => {
  unlistenDrop?.()
  window.clearTimeout(zoneTimer)
})

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
  { id: 'all', name: '全部', count: resources.value.length, deletable: false },
  ...zones.value.map((z) => ({ id: z.id, name: z.name, count: z.itemCount, deletable: true })),
  { id: 'none', name: '未分区', count: resources.value.filter((r) => !r.zoneId).length, deletable: false },
])

/**
 * 分区能建不能删（2026-11-02 用户报的）：`zone_delete` 命令与 `api.zoneDelete` 一直都在，
 * 只是界面上没有入口 —— 有命令没入口和有能力没命令是同一类账。
 *
 * 删分区**不动里面的速达**：后端把它们的 `zone_id` 置空，它们退回「未分区」那一格。
 * 所以这一步不是不可逆的数据丢失，但也不该一按就没了：沿用速达磁贴那条两步内联确认（同一个 5 秒）。
 */
const armedZone = ref<string | null>(null)
let zoneTimer: number | undefined

async function askZoneDelete(id: string) {
  if (armedZone.value !== id) {
    armedZone.value = id
    window.clearTimeout(zoneTimer)
    zoneTimer = window.setTimeout(() => (armedZone.value = null), 5000)
    return
  }
  armedZone.value = null
  window.clearTimeout(zoneTimer)
  await api.zoneDelete(id)
  // 正在看这个分区时别停在一个已经不存在的筛选上
  if (activeZone.value === id) activeZone.value = 'all'
  await reload()
}
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <div class="chips scroller">
        <span v-for="chip in zoneChips" :key="chip.id" class="chipwrap" :class="{ real: chip.deletable }">
          <button
            type="button"
            class="chip"
            :class="{ on: activeZone === chip.id }"
            @click="activeZone = chip.id"
          >
            {{ chip.name }}<span v-if="chip.count" class="c">{{ chip.count }}</span>
          </button>
          <!-- 那个 × 不能做成 <button>：外面这颗已经是按钮了，按钮里不许再放交互元素。
               与速达磁贴同一形状：平时不占视线，悬停/待确认时才出现 -->
          <span
            v-if="chip.deletable"
            role="button"
            tabindex="0"
            class="kill"
            :class="{ armed: armedZone === chip.id }"
            :title="
              armedZone === chip.id
                ? '再点一次删除这个分区；里面的速达不会丢，会退回「未分区」'
                : '删除分区（里面的速达退回未分区）'
            "
            @click.stop="askZoneDelete(chip.id)"
            @keydown.enter.prevent="askZoneDelete(chip.id)"
            @keydown.esc.prevent="armedZone = null"
          >{{ armedZone === chip.id ? '?' : '×' }}</span>
        </span>
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

    <p class="hint">拖进来即可添加</p>
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 10px;
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
/* 分区那颗胶囊 + 挂在它右边的 ×。胶囊本身是 <button>，所以 × 只能是它的**兄弟**
   （按钮里不许再放交互元素），靠这个 wrapper 把它们粘成一枚看起来一体的控件 */
.chipwrap {
  display: inline-flex;
  align-items: center;
  flex: none;
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
.kill {
  flex: none;
  width: 15px;
  height: 15px;
  margin-left: -6px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  font-size: 11px;
  line-height: 1;
  color: var(--t3);
  background: var(--card-2);
  border: 1px solid var(--line-2);
  cursor: pointer;
  /* 平时不占视线；悬停或待确认时才出现。armed 必须常驻可见，
     否则指针一移开就看不见「再点一次」，回来再点等于误删 */
  opacity: 0;
}
.chipwrap.real:hover .kill,
.kill.armed,
.kill:focus-visible {
  opacity: 1;
}
.kill.armed {
  color: var(--red);
  background: color-mix(in srgb, var(--red) 14%, transparent);
  border-color: color-mix(in srgb, var(--red) 40%, transparent);
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
