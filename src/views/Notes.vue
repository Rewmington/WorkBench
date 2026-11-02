<script setup lang="ts">
import { BookOpen, FilePlus2, Pin, RotateCcw, Scissors, Trash2 } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { api } from '../api'
import type { NoteFolder } from '../api/types'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import Segmented from '../components/ui/Segmented.vue'
import { useClock } from '../composables/useClock'
import { EDITOR_MODES, useNotes, type EditorMode } from '../composables/useNotes'
import { errText } from '../lib/err'
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
/** 点了个交不出去预览的链接：说一句就完，别让它静默 —— 这条命令曾经整条是哑的。 */
const linkError = ref<string | null>(null)

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

/**
 * 文件夹树摊平成 `{ f, depth }`。以前模板只写死「根 + 直接子级」两层，
 * 而这次要补的「移到…」一旦把 A 挪进 B（B 本身是子级），A 就成了第三层 ——
 * 老模板会直接把它渲染不出来，看起来就像数据没了。
 * `depth < 8` 是兜底：造环后端已经拦（`note_folders::move_to`），但库里万一已经有一条环，
 * 不能让笔记页把自己转死。
 */
const treeRows = computed(() => {
  const all = notes.folders.value
  const out: { f: NoteFolder; depth: number }[] = []
  const walk = (parent: string | null, depth: number) => {
    for (const f of all.filter((x) => x.parentId === parent)) {
      out.push({ f, depth })
      if (depth < 8) walk(f.id, depth + 1)
    }
  }
  walk(null, 0)
  return out
})

/** 能被选中的父级：排除它自己与它的整棵子树，否则一点就把树拧成环（后端也会拒）。 */
function legalTargets(id: string): NoteFolder[] {
  const banned = new Set<string>([id])
  const grow = (parent: string) => {
    for (const f of notes.folders.value.filter((x) => x.parentId === parent)) {
      if (banned.has(f.id)) continue
      banned.add(f.id)
      grow(f.id)
    }
  }
  grow(id)
  return notes.folders.value.filter((f) => !banned.has(f.id))
}

function snippet(body: string): string {
  return body.replace(/[#>*`\-[\]()]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 46)
}

function onPreviewClick(event: MouseEvent) {
  const href = linkHref(event.target)
  if (!href) return
  // 链接一律交给系统浏览器：原生软件不在自己窗口里开网页（§8.6）。
  // 这一句 `preventDefault` 是这条规则的兑现点：预览里的 `<a>` 带着真 href，不拦下来
  // WebView2 会自己导航过去，整个应用窗口就被带走 —— 真窗口里点一下才发现，jsdom 看不见导航。
  event.preventDefault()
  linkError.value = null
  api.openUrl(href).catch((e) => {
    linkError.value = errText(e)
  })
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
 * 文件夹的改名 / 移到 / 删除（2026-11-02 扫出来的整片盲区：`folder_rename`、`folder_delete`、
 * `folder_move` 三条命令与 composable 都写好了，模板里一个按钮都没有 —— 于是文件夹只能建，
 * 建歪了没法改也没法删。与速达分区「能建不能删」同一类账。）
 *
 * 一次只开一行；删除这一步要确认：删的是组织结构，笔记本身后端会退回上一层，不会被删掉。
 */
const folderMenu = ref<string | null>(null)
const renamingId = ref<string | null>(null)
const renameDraft = ref('')
const movingId = ref<string | null>(null)
const confirmDel = ref<string | null>(null)
const folderNotice = ref<string | null>(null)

function toggleFolderMenu(id: string) {
  folderNotice.value = null
  renamingId.value = null
  movingId.value = null
  confirmDel.value = null
  folderMenu.value = folderMenu.value === id ? null : id
}

function startRename(f: NoteFolder) {
  renamingId.value = f.id
  renameDraft.value = f.name
}

async function commitRename(id: string) {
  const name = renameDraft.value.trim()
  renamingId.value = null
  // 空名字后端会拒；名字没变就别写 —— 白涨一条 rev 会被同步当成一次真实改动带给手机
  const current = notes.folders.value.find((f) => f.id === id)?.name
  if (!name || name === current) return
  try {
    await notes.renameFolder(id, name)
  } catch (e) {
    folderNotice.value = errText(e)
  }
}

async function moveFolder(id: string, parentId: string | null) {
  movingId.value = null
  try {
    await notes.moveFolder(id, parentId)
    folderNotice.value = parentId ? '已挪进所选文件夹' : '已挪到顶层'
  } catch (e) {
    folderNotice.value = errText(e)
  }
}

/** 两步：第一下把这一行变成「确认删除」，第二下才真删（与分区删除、速达删除同一口径）。 */
async function askDeleteFolder(id: string) {
  if (confirmDel.value !== id) {
    confirmDel.value = id
    return
  }
  confirmDel.value = null
  folderMenu.value = null
  try {
    const moved = await notes.removeFolder(id)
    folderNotice.value = moved
      ? `已删除 · ${moved} 篇笔记退回上一层，子目录也提上来了`
      : '已删除 · 里面没有笔记，子目录提到了上一层'
  } catch (e) {
    folderNotice.value = errText(e)
  }
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

// 处理器必须是**建组件时就有了的同一引用**：以前它们在 `await notes.load()` 之后才赋值，
// 于是「载入没回来就切走」时卸载钩子拆的是空函数，随后 onMounted 的续体照样把监听挂上 ——
// 每次快切漏一对，而且失焦时会去 flush 一个已经卸载的模型。
const blurFlush = () => void notes.flush()
const visFlush = () => {
  if (document.visibilityState === 'hidden') void notes.flush()
}

onMounted(async () => {
  window.addEventListener('blur', blurFlush)
  document.addEventListener('visibilitychange', visFlush)
  await notes.load()
})

onBeforeUnmount(() => {
  window.removeEventListener('blur', blurFlush)
  document.removeEventListener('visibilitychange', visFlush)
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
      <template v-for="row in treeRows" :key="row.f.id">
        <div class="trow" :class="{ open: folderMenu === row.f.id }">
          <button
            type="button"
            class="ti"
            :class="{
              kid: row.depth === 1,
              kid2: row.depth >= 2,
              on: notes.scope.value.kind === 'folder' && notes.scope.value.id === row.f.id,
            }"
            @click="notes.go({ kind: 'folder', id: row.f.id })"
          >
            {{ row.f.name }}
            <span class="c">{{ row.f.noteCount }}</span>
          </button>
          <!-- 那颗 … 不能嵌在 .ti 里：按钮里不许再放交互元素（速达分区踩过同一条） -->
          <button
            type="button"
            class="mg"
            :class="{ lit: folderMenu === row.f.id }"
            :aria-label="`管理文件夹 ${row.f.name}`"
            :aria-expanded="folderMenu === row.f.id"
            @click="toggleFolderMenu(row.f.id)"
          >
            …
          </button>
        </div>
        <div v-if="folderMenu === row.f.id" class="fmenu" @keydown.esc.prevent="folderMenu = null">
          <input
            v-if="renamingId === row.f.id"
            v-model="renameDraft"
            class="txt rin"
            placeholder="新名字，回车保存"
            @keydown.enter.prevent="commitRename(row.f.id)"
            @keydown.esc.prevent="renamingId = null"
            @blur="renamingId === row.f.id ? commitRename(row.f.id) : null"
          />
          <template v-else-if="movingId === row.f.id">
            <span class="ml">挪到哪儿</span>
            <button type="button" class="y-chip tgt" @click="moveFolder(row.f.id, null)">顶层</button>
            <button
              v-for="t in legalTargets(row.f.id)"
              :key="t.id"
              type="button"
              class="y-chip tgt"
              @click="moveFolder(row.f.id, t.id)"
            >
              {{ t.name }}
            </button>
            <button type="button" class="y-chip cancel" @click="movingId = null">取消</button>
          </template>
          <template v-else-if="confirmDel === row.f.id">
            <span class="ml">删除这个文件夹？里面的笔记退回上一层</span>
            <button type="button" class="y-chip r" @click="askDeleteFolder(row.f.id)">确认删除</button>
            <button type="button" class="y-chip cancel" @click="confirmDel = null">取消</button>
          </template>
          <template v-else>
            <button type="button" class="fa" @click="startRename(row.f)">改名</button>
            <button type="button" class="fa" @click="movingId = row.f.id">移到…</button>
            <button type="button" class="fa del" @click="confirmDel = row.f.id">删除</button>
          </template>
        </div>
      </template>
      <p v-if="folderNotice" class="fnote">{{ folderNotice }}</p>
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

      <p v-if="notes.saveError.value" class="warn">{{ notes.saveError.value }}</p>

      <p v-if="linkError" class="warn">{{ linkError }}</p>

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
/* 第三层及更深：再缩一级。缩进只画到这儿，更深的树靠父级关系找，不靠缩进猜 */
.ti.kid2 {
  padding-left: 34px;
  font-size: 11.5px;
}
.trow {
  display: flex;
  align-items: center;
  gap: 2px;
}
.trow .ti {
  flex: 1;
  min-width: 0;
}
.mg {
  flex: none;
  width: 18px;
  height: 22px;
  border-radius: 6px;
  color: var(--t3);
  font-size: 13px;
  line-height: 1;
  opacity: 0;
}
.trow:hover .mg,
.mg.lit,
.mg:focus-visible {
  opacity: 1;
}
.mg:hover {
  background: var(--card-2);
  color: var(--t1);
}
.mg.lit {
  color: var(--accent);
  background: var(--accent-soft);
}
.fmenu {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  padding: 3px 2px 6px 8px;
}
.fmenu .ml {
  font-size: 10.5px;
  color: var(--t3);
  flex: 1 1 100%;
}
.fa {
  font-size: 11px;
  color: var(--t2);
  padding: 3px 7px;
  border-radius: 6px;
  border: 1px solid var(--line);
  background: var(--card-2);
}
.fa:hover {
  color: var(--t1);
  border-color: var(--line-2);
}
.fa.del:hover {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 40%, transparent);
}
.fmenu .tgt {
  cursor: pointer;
}
.fmenu .tgt:hover {
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 30%, transparent);
}
.fmenu .cancel {
  cursor: pointer;
  opacity: 0.75;
}
.rin {
  flex: 1 1 100%;
  height: 24px;
}
.fnote {
  font-size: 10.5px;
  color: var(--t3);
  padding: 4px 6px 0;
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
