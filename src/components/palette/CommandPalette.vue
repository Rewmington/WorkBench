<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { CornerDownLeft, Search } from 'lucide-vue-next'

import { api } from '../../api'
import type { PaletteHit } from '../../api/types'
import { ranked } from '../../lib/fuzzy'
import { useActions } from '../../composables/useActions'
import { usePalette } from '../../composables/usePalette'

/**
 * 命令面板是这张产品的脸，所以自绘不引库（PLAN §3）。
 * 键盘必须全程可达：↑↓ 换行、Enter 执行、Shift+Enter 执行后不关、Home/End 跳两端。
 */
interface Row {
  key: string
  group: string
  label: string
  sub?: string
  run: () => void | Promise<void>
}

const { open } = usePalette()
const actions = useActions()
const query = ref('')
const cursor = ref(0)
const hits = ref<PaletteHit[]>([])
const input = ref<HTMLInputElement | null>(null)

/** 本地 SQLite 不需要 AbortController：只认最后一次请求的序号，旧结果直接丢。 */
let seq = 0

const rows = computed<Row[]>(() => {
  const q = query.value.trim()
  const actionRows: Row[] = (q ? ranked(q, actions, (a) => `${a.label} ${a.id}`) : actions).map((a) => ({
    key: `a:${a.id}`,
    group: a.group,
    label: a.label,
    sub: a.hint,
    run: a.run,
  }))

  const hitRows: Row[] = hits.value.map((h) => ({
    key: `h:${h.kind}:${h.id}`,
    group: h.kind === 'todo' ? '待办' : '速达',
    label: h.label,
    sub: h.sub ?? undefined,
    run: () => runHit(h),
  }))

  return [...actionRows, ...hitRows]
})

const grouped = computed(() => {
  const out: { group: string; rows: Row[] }[] = []
  for (const row of rows.value) {
    const bucket = out.find((g) => g.group === row.group)
    if (bucket) bucket.rows.push(row)
    else out.push({ group: row.group, rows: [row] })
  }
  return out
})

async function runHit(hit: PaletteHit) {
  const { useShell } = await import('../../composables/useShell')
  const shell = useShell()
  if (hit.kind === 'todo') {
    shell.go('todos')
    shell.select({ kind: 'todo', id: hit.id })
  } else {
    shell.go('launcher')
    shell.select({ kind: 'resource', id: hit.id })
  }
}

function runRow(row: Row, keepOpen: boolean) {
  // 先关面板再跑：以前是 `await row.run()` 再 close，于是「立即备份」按下去后
  // 这层模态要一直压到整份加密上传结束（几十秒），底下的备份页什么都看不见。
  // 动作自己的结果与流水都渲染在各页面上，面板的任务只是把这一脚踢出去。
  if (!keepOpen) close()
  void row.run()
}

function close() {
  open.value = false
  query.value = ''
  hits.value = []
  cursor.value = 0
}

/**
 * 面板声明了 `role="dialog" aria-modal="true"`，那两句承诺的是：
 * 打开时焦点进得来、Tab 出不去、关掉时焦点回到原来那把椅子上。
 * 从前 Esc 只绑在 `<input>` 上 —— 焦点一旦 Tab 到列表按钮或跑到遮罩后面，
 * Esc 就不再生效，而遮罩下的控件照样 Tab 得到，等于承诺没兑现。
 */
const pal = ref<HTMLElement | null>(null)
let restoreFocusTo: HTMLElement | null = null

/** 面板里能接住 Tab 的元素。不用 `offsetParent` 过滤可见性：jsdom 里它恒为 null。 */
function focusables(): HTMLElement[] {
  const root = pal.value
  if (!root) return []
  return Array.from(
    root.querySelectorAll<HTMLElement>('input, button, [tabindex]:not([tabindex="-1"])'),
  ).filter((el) => !el.hasAttribute('disabled') && !el.closest('[hidden]'))
}

function trapTab(event: KeyboardEvent) {
  const items = focusables()
  if (items.length === 0) return
  event.preventDefault()
  const here = items.indexOf(document.activeElement as HTMLElement)
  const next = event.shiftKey
    ? here <= 0
      ? items.length - 1
      : here - 1
    : here === items.length - 1
      ? 0
      : here + 1
  items[next]?.focus()
}

function move(delta: number) {
  const total = rows.value.length
  if (!total) return
  cursor.value = (cursor.value + delta + total) % total
}

watch(open, async (v) => {
  if (!v) {
    restoreFocusTo?.focus?.()
    restoreFocusTo = null
    return
  }
  query.value = ''
  hits.value = []
  cursor.value = 0
  restoreFocusTo =
    document.activeElement instanceof HTMLElement ? document.activeElement : null
  await nextTick()
  input.value?.focus()
})

let debounce = 0
watch(query, (q) => {
  window.clearTimeout(debounce)
  const mine = ++seq
  if (!q.trim()) {
    hits.value = []
    cursor.value = 0
    return
  }
  debounce = window.setTimeout(async () => {
    const found = await api.paletteSearch(q)
    if (mine !== seq) return
    hits.value = found
    cursor.value = 0
  }, 40)
})

function onKeydown(event: KeyboardEvent) {
  switch (event.key) {
    case 'Tab':
      trapTab(event)
      break
    case 'ArrowDown':
      event.preventDefault()
      move(1)
      break
    case 'ArrowUp':
      event.preventDefault()
      move(-1)
      break
    case 'Home':
      event.preventDefault()
      cursor.value = 0
      break
    case 'End':
      event.preventDefault()
      cursor.value = Math.max(0, rows.value.length - 1)
      break
    case 'Enter': {
      // 焦点已经落在某一行按钮上时让浏览器自己按它：handler 绑在容器上，
      // 在这里 preventDefault 会把"按下焦点那一行"换成"按下光标那一行"
      const onButton =
        event.target instanceof HTMLElement && event.target.closest('button') !== null
      if (onButton) break
      event.preventDefault()
      const row = rows.value[cursor.value]
      if (row) void runRow(row, event.shiftKey)
      break
    }
    case 'Escape':
      event.preventDefault()
      close()
      break
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="scrim" @mousedown.self="close">
      <div
        ref="pal"
        class="pal"
        role="dialog"
        aria-modal="true"
        aria-label="命令面板"
        @keydown="onKeydown"
      >
        <div class="field">
          <Search :size="15" />
          <input
            ref="input"
            v-model="query"
            class="palette-query"
            placeholder="搜索待办、速达，或输入动作…"
            role="combobox"
            aria-expanded="true"
            aria-controls="pal-list"
            :aria-activedescendant="rows[cursor] ? `pal-${cursor}` : undefined"
          />
          <kbd>Esc</kbd>
        </div>

        <div v-if="rows.length" id="pal-list" class="list scroller" role="listbox">
          <template v-for="bucket in grouped" :key="bucket.group">
            <div class="gh">{{ bucket.group }}</div>
            <button
              v-for="row in bucket.rows"
              :id="`pal-${rows.indexOf(row)}`"
              :key="row.key"
              type="button"
              class="it"
              role="option"
              :aria-selected="rows[cursor]?.key === row.key"
              :class="{ on: rows[cursor]?.key === row.key }"
              @mouseenter="cursor = rows.indexOf(row)"
              @click="runRow(row, $event.shiftKey)"
            >
              <span class="lb">{{ row.label }}</span>
              <span v-if="row.sub" class="sb">{{ row.sub }}</span>
              <CornerDownLeft v-if="rows[cursor]?.key === row.key" :size="13" class="go" />
            </button>
          </template>
        </div>
        <div v-else class="y-empty empty">没有匹配的东西。动作名可以只打一部分，比如「主题」「备份」。</div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  background: rgba(4, 6, 5, 0.55);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 12vh;
  z-index: 60;
}
.pal {
  width: min(620px, 92vw);
  background: var(--panel);
  border: 1px solid var(--line-2);
  border-radius: var(--r);
  box-shadow: 0 26px 60px rgba(0, 0, 0, 0.5);
  overflow: hidden;
}
.field {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 13px 14px;
  border-bottom: 1px solid var(--line);
  color: var(--t3);
}
.field input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: none;
  color: var(--t1);
  font-size: 14px;
}
.field kbd {
  font-size: 10.5px;
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid var(--line);
  background: var(--card-2);
  color: var(--t2);
}
.list {
  max-height: min(56vh, 460px);
  overflow-y: auto;
  padding: 6px;
}
.gh {
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.08em;
  color: var(--t3);
  padding: 9px 9px 4px;
}
.it {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 8px 9px;
  border-radius: var(--r-sm);
  text-align: left;
  color: var(--t2);
}
.it.on {
  background: var(--accent-soft);
  color: var(--t1);
}
.it .lb {
  font-size: 13px;
  font-weight: 650;
}
.it .sb {
  margin-left: auto;
  font-size: 11px;
  color: var(--t3);
  max-width: 46%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.it .go {
  color: var(--accent);
  flex: none;
}
.empty {
  margin: 10px;
}
</style>
