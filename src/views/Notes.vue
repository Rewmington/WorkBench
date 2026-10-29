<script setup lang="ts">
import { BookOpen, FilePlus2, Pin, RotateCcw, Scissors, Trash2 } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { api } from '../api'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import Segmented from '../components/ui/Segmented.vue'
import { useClock } from '../composables/useClock'
import { EDITOR_MODES, useNotes, type EditorMode } from '../composables/useNotes'
import { linkHref, renderMarkdown } from '../lib/markdown'
import { relTime } from '../lib/reltime'

/**
 * 轻量笔记（PLAN §4）：Markdown 源码 + 预览双栏。
 *
 * **正文即真相源** —— 库里只存 Markdown，预览每次现算，所以不存在「预览与源码不一致」
 * 这一类 bug。不做双链、不做图片上传（v1 明确不做）。
 */
const notes = useNotes()
const { now } = useClock()

const newFolder = ref('')
const confirmPurge = ref<string | null>(null)
const tagDraft = ref('')

const MODE_LABEL: Record<EditorMode, string> = { split: '双栏', source: '源码', preview: '预览' }
const modeOptions = EDITOR_MODES.map((value) => ({ value, label: MODE_LABEL[value] }))

const list = computed(() => (notes.scope.value.kind === 'trash' ? notes.trashed.value : notes.notes.value))
const current = computed(() => notes.selected() ?? null)
const isTrash = computed(() => notes.scope.value.kind === 'trash')

/** 预览每次从正文现算；textarea 显示的是防抖窗口里的本地值，两者必须同源。 */
const rendered = computed(() => {
  const note = current.value
  if (!note) return { html: '', blocked: 0 }
  return renderMarkdown(notes.bodyOf(note.id))
})

const tree = computed(() => {
  const roots = notes.folders.value.filter((f) => f.parentId === null)
  const kids = (id: string) => notes.folders.value.filter((f) => f.parentId === id)
  return { roots, kids }
})

function snippet(body: string): string {
  return body.replace(/[#>*`\-[\]()]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 46)
}

function onPreviewClick(event: MouseEvent) {
  const href = linkHref(event.target)
  // 链接一律交给系统浏览器：原生软件不在自己窗口里开网页（§8.6）
  if (href) void api.openUrl(href)
}

async function commitTitle(value: string) {
  const id = notes.selectedId.value
  if (!id) return
  notes.scheduleSave({ title: value || ' ' })
}

async function commitTags() {
  const raw = tagDraft.value
  tagDraft.value = ''
  const parts = raw.split(/[,，\s]+/).map((s) => s.trim()).filter(Boolean)
  if (!parts.length || !current.value) return
  await notes.setTags([...new Set([...current.value.tags, ...parts])])
}

async function dropTag(name: string) {
  if (!current.value) return
  await notes.setTags(current.value.tags.filter((t) => t !== name))
}

async function addFolder() {
  const name = newFolder.value.trim()
  if (!name) return
  newFolder.value = ''
  await notes.addFolder(name)
}

/**
 * 彻底删除用两步内联确认，不用原生 alert —— 与备份页「还原远端快照」同一套做法。
 * 理由：`confirm()` 在 WebView2 里是系统模态框，样式不受设计令牌控制（§8.5 的债），
 * 而且它会打断「这一步之后只能靠快照找回」这句话该有的分量。
 */
async function purge(id: string) {
  if (confirmPurge.value !== id) {
    confirmPurge.value = id
    window.setTimeout(() => {
      if (confirmPurge.value === id) confirmPurge.value = null
    }, 5000)
    return
  }
  confirmPurge.value = null
  await notes.purge(id)
}

let blurFlush: (() => void) | null = null
let visFlush: (() => void) | null = null

onMounted(async () => {
  await notes.load()
  blurFlush = () => void notes.flush()
  visFlush = () => {
    if (document.visibilityState === 'hidden') void notes.flush()
  }
  window.addEventListener('blur', blurFlush)
  document.addEventListener('visibilitychange', visFlush)
})

onBeforeUnmount(() => {
  window.removeEventListener('blur', blurFlush ?? (() => {}))
  document.removeEventListener('visibilitychange', visFlush ?? (() => {}))
  void notes.flush()
})
</script>

<template>
  <section class="y-view n-root">
    <aside class="tree">
      <div class="th">文件夹</div>
      <button type="button" class="ti" :class="{ on: notes.scope.value.kind === 'all' }" @click="notes.go({ kind: 'all' })">
        <BookOpen :size="13" /> 全部
        <span class="c">{{ notes.notes.value.length }}</span>
      </button>
      <template v-for="f in tree.roots" :key="f.id">
        <button
          type="button"
          class="ti"
          :class="{ on: notes.scope.value.kind === 'folder' && notes.scope.value.id === f.id }"
          @click="notes.go({ kind: 'folder', id: f.id })"
        >
          {{ f.name }}
          <span class="c">{{ f.noteCount }}</span>
        </button>
        <button
          v-for="k in tree.kids(f.id)"
          :key="k.id"
          type="button"
          class="ti kid"
          :class="{ on: notes.scope.value.kind === 'folder' && notes.scope.value.id === k.id }"
          @click="notes.go({ kind: 'folder', id: k.id })"
        >
          {{ k.name }}
          <span class="c">{{ k.noteCount }}</span>
        </button>
      </template>
      <button type="button" class="ti" :class="{ on: isTrash }" @click="notes.go({ kind: 'trash' })">
        <Trash2 :size="12" /> 回收站
        <span class="c">{{ notes.trashed.value.length }}</span>
      </button>
      <div class="newf">
        <input v-model="newFolder" class="txt" placeholder="新文件夹名" @keydown.enter.prevent="addFolder" />
      </div>
    </aside>

    <div class="list">
      <div class="lh">
        <span>{{ isTrash ? '回收站' : '笔记' }}</span>
        <button v-if="!isTrash" type="button" class="y-ghost tiny" @click="notes.create()">
          <FilePlus2 :size="12" /> 新建
        </button>
      </div>
      <CollapsedRow
        v-if="!list.length"
        :label="isTrash ? '回收站是空的' : '这里还没有笔记'"
        :detail="isTrash ? '删掉的笔记会先留在这里，随时能还原' : '按 Ctrl+K 或直接点「新建」开始写'"
      />
      <button
        v-for="n in list"
        :key="n.id"
        type="button"
        class="ni"
        :class="{ on: n.id === notes.selectedId.value }"
        @click="isTrash ? notes.open(n.id) : notes.open(n.id)"
      >
        <span class="nt">{{ n.title || '无标题' }}<Pin v-if="n.pinned" :size="10" class="pin" /></span>
        <span class="ns">{{ snippet(n.bodyMd) || '空' }}</span>
        <span class="nd">{{ relTime(now, isTrash ? (n.trashedAt ?? n.updatedAt) : n.updatedAt) }}</span>
      </button>
    </div>

    <div v-if="current" class="editor">
      <div class="eh">
        <input
          class="ttl"
          :value="notes.titleOf(current.id)"
          placeholder="标题"
          @input="commitTitle(($event.target as HTMLInputElement).value)"
        />
        <Segmented :options="modeOptions" :model-value="notes.mode.value" @update:model-value="notes.setMode($event)" />
        <button type="button" class="ib" :class="{ lit: current.pinned }" :title="current.pinned ? '取消置顶' : '置顶'" @click="notes.togglePin()">
          <Pin :size="13" />
        </button>
        <button v-if="!isTrash" type="button" class="ib" title="移入回收站" @click="notes.trash()">
          <Scissors :size="13" />
        </button>
      </div>

      <div class="tags">
        <span v-for="t in current.tags" :key="t" class="y-chip">
          {{ t }}
          <button type="button" class="x" :title="`去掉标签 ${t}`" @click="dropTag(t)">×</button>
        </span>
        <input v-model="tagDraft" class="tagin" placeholder="加标签，回车" @keydown.enter.prevent="commitTags" />
      </div>

      <p v-if="rendered.blocked" class="warn">
        有 {{ rendered.blocked }} 个链接因协议不安全被禁用（只放行 http / https / mailto）。
      </p>

      <div v-if="isTrash" class="trashbar">
        <span>这条在回收站里，还原后正文与标签一字不差。</span>
        <button type="button" class="y-ghost tiny" @click="notes.restore(current.id)">
          <RotateCcw :size="12" /> 还原
        </button>
        <button type="button" class="y-ghost tiny danger" :class="{ armed: confirmPurge === current.id }" @click="purge(current.id)">
          {{ confirmPurge === current.id ? '再点一次彻底删除' : '彻底删除' }}
        </button>
      </div>

      <div v-else class="panes" :class="notes.mode.value">
        <textarea
          v-if="notes.mode.value !== 'preview'"
          class="src selectable"
          spellcheck="false"
          :value="notes.bodyOf(current.id)"
          placeholder="# 从这里开始写 Markdown"
          @input="notes.scheduleSave({ bodyMd: ($event.target as HTMLTextAreaElement).value })"
        />
        <div
          v-if="notes.mode.value !== 'source'"
          class="pv scroller md-preview"
          @click="onPreviewClick"
          v-html="rendered.html"
        />
      </div>
    </div>

    <CollapsedRow v-else label="笔记 · 没选中任何一条" detail="左边挑一条，或点「新建」" />
  </section>
</template>

<style scoped>
.n-root {
  display: grid;
  grid-template-columns: 156px 208px minmax(0, 1fr);
  gap: 10px;
  min-height: 0;
  flex: 1;
}
.tree {
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--card);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  overflow-y: auto;
  min-height: 0;
}
.th {
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--t3);
  padding: 2px 6px 7px;
}
.ti {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 7px;
  border-radius: 8px;
  color: var(--t2);
  font-size: 12px;
  text-align: left;
}
.ti.kid {
  padding-left: 22px;
  font-size: 11.5px;
}
.ti:hover {
  background: var(--card-2);
  color: var(--t1);
}
.ti.on {
  background: var(--accent-soft);
  color: var(--t1);
  font-weight: 700;
}
.ti .c {
  margin-left: auto;
  font-size: 10.5px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}
.newf {
  margin-top: auto;
  padding-top: 8px;
}
.txt {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border-radius: 7px;
  border: 1px solid var(--line-2);
  background: var(--card-2);
  color: var(--t1);
  font-size: 11.5px;
  user-select: text;
}

.list {
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--card);
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.lh {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 10px;
  font-size: 12px;
  font-weight: 800;
  border-bottom: 1px solid var(--line);
  flex: none;
}
.lh .y-ghost.tiny {
  margin-left: auto;
  height: 24px;
  width: auto;
  padding: 0 8px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
}
.list > .y-empty {
  margin: 8px;
}
.ni {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--line);
  text-align: left;
  font: inherit;
  color: var(--t2);
  overflow: hidden;
}
.ni:hover {
  background: var(--card-2);
}
.ni.on {
  background: var(--card-2);
  box-shadow: inset 2px 0 0 var(--accent);
}
.nt {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 12.5px;
  font-weight: 700;
  color: var(--t1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.nt .pin {
  color: var(--amber);
  flex: none;
}
.ns {
  font-size: 11px;
  color: var(--t3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.nd {
  font-size: 10px;
  color: var(--t3);
  font-variant-numeric: tabular-nums;
}

.editor {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  min-height: 0;
}
.eh {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ttl {
  flex: 1;
  min-width: 0;
  height: 30px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--t1);
  font-size: 14px;
  font-weight: 800;
  padding: 0 6px;
  user-select: text;
}
.ttl:focus {
  outline: none;
  border-color: var(--line-2);
  background: var(--card);
}
.ib {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  border: 1px solid var(--line-2);
  background: var(--card);
  display: grid;
  place-items: center;
  color: var(--t3);
  flex: none;
}
.ib:hover {
  color: var(--t1);
}
.ib.lit {
  color: var(--amber);
  border-color: color-mix(in srgb, var(--amber) 40%, transparent);
}
.tags {
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
}
.tags .x {
  color: inherit;
  opacity: 0.6;
  font-size: 11px;
  margin-left: 3px;
}
.tags .x:hover {
  opacity: 1;
}
.tagin {
  width: 120px;
  height: 22px;
  padding: 0 7px;
  border-radius: 20px;
  border: 1px dashed var(--line-2);
  background: transparent;
  color: var(--t1);
  font-size: 11px;
  user-select: text;
}
.warn {
  margin: 0;
  font-size: 11px;
  color: var(--amber);
}
.trashbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--line-2);
  border-radius: var(--r-sm);
  background: var(--card);
  font-size: 12px;
  color: var(--t2);
}
.trashbar .y-ghost.tiny {
  height: 25px;
  width: auto;
  padding: 0 9px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
}
.trashbar .danger {
  margin-left: auto;
  color: var(--red);
}
.trashbar .danger.armed {
  background: color-mix(in srgb, var(--red) 14%, transparent);
  border-color: color-mix(in srgb, var(--red) 40%, transparent);
}

.panes {
  flex: 1;
  min-height: 0;
  display: grid;
  gap: 10px;
  grid-template-columns: 1fr 1fr;
}
.panes.source,
.panes.preview {
  grid-template-columns: 1fr;
}
.src {
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--card);
  color: var(--t1);
  font: 400 12.5px/1.7 ui-monospace, SFMono-Regular, Consolas, monospace;
  padding: 11px 12px;
  resize: none;
  min-height: 0;
  user-select: text;
}
.src:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
}
.pv {
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--card);
  padding: 11px 14px;
  overflow-y: auto;
  min-height: 0;
}

@media (max-width: 1080px) {
  .n-root {
    grid-template-columns: 140px minmax(0, 1fr);
  }
  .editor {
    grid-column: 1 / -1;
  }
}
@media (max-width: 720px) {
  .n-root {
    grid-template-columns: minmax(0, 1fr);
  }
  .panes {
    grid-template-columns: 1fr;
  }
}
</style>
