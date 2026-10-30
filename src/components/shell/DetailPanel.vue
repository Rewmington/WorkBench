<script setup lang="ts">
import { Sparkles } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'

import { api } from '../../api'
import type { Resource, Todo } from '../../api/types'
import { useClock } from '../../composables/useClock'
import { useShell } from '../../composables/useShell'
import { useTodos } from '../../composables/useTodos'
import { dueLabel, relTime } from '../../lib/reltime'
import { hhmm } from '../../lib/timetable'

/**
 * 常驻详情面板（§8.1.1 / §8.8.3）。字段按本机语义重写过：
 * 参考图的「负责人」删掉（单用户本机应用放这个是废的），「所属目录」→ 清单/标签，
 * 「首次发现」→ 创建 · 来源设备，并额外显示 rev 与最后写入设备 —— 同步字段本来就在库里。
 */
const shell = useShell()
const { now } = useClock()

const todo = ref<Todo | null>(null)
const resource = ref<Resource | null>(null)
const children = ref<Todo[]>([])
const secret = ref<string | null | 'unavailable'>(null)
const childDraft = ref('')
const todosModel = useTodos()

/** 时间轴点某天时列在当天的完成项（§8.8.5：复用常驻面板，不另做弹窗）。 */
const dayItems = computed(() => {
  const sel = shell.selection.value
  if (sel?.kind !== 'day') return []
  return todosModel.all.value.filter((t) => t.completed && t.doneDay === sel.day)
})
const dayLabel = computed(() => {
  const sel = shell.selection.value
  if (sel?.kind !== 'day') return ''
  const s = String(sel.day)
  return `${s.slice(0, 4)} 年 ${Number(s.slice(4, 6))} 月 ${Number(s.slice(6, 8))} 日`
})

const PRIORITY = ['紧急', '高', '普通', '低']
const DAY_NAMES = ['周一', '周二', '周三', '周四', '周五', '周六', '周日']

/** 课表的一格。整行由选择带进来（外部导入的只读数据，没有 id 可回查）。 */
const cls = computed(() => (shell.selection.value?.kind === 'class' ? shell.selection.value.cls : null))
const selWeek = computed(() => (shell.selection.value?.kind === 'class' ? shell.selection.value.week : null))

async function load() {
  const sel = shell.selection.value
  todo.value = null
  resource.value = null
  children.value = []
  secret.value = null
  if (!sel) return

  if (sel.kind === 'todo') {
    todo.value = await api.todosGet(sel.id)
    children.value = await api.todosChildren(sel.id)
  } else if (sel.kind === 'resource') {
    const list = await api.resourcesList()
    resource.value = list.find((r) => r.id === sel.id) ?? null
  }
}

watch(() => shell.selection.value, load, { immediate: true })

const shortDevice = (id: string) => (id.length > 10 ? `${id.slice(0, 6)}…${id.slice(-3)}` : id)

async function toggleDone() {
  if (!todo.value) return
  await api.todosSetDone(todo.value.id, !todo.value.completed)
  await load()
}

async function addChild() {
  const title = childDraft.value.trim()
  if (!title || !todo.value) return
  childDraft.value = ''
  await api.todosCreate({ title, parentId: todo.value.id, zone: todo.value.zone })
  await load()
}

async function removeChild(child: Todo) {
  await api.todosDelete(child.id)
  await load()
}

async function revealSecret() {
  if (!resource.value) return
  try {
    secret.value = (await api.resourceRevealSecret(resource.value.id)) ?? 'unavailable'
  } catch (e) {
    // 解不开就把原因原样写出来，不显示空字符串冒充「没有备注」
    secret.value = String(e)
  }
}

const launched = computed(() =>
  resource.value?.lastLaunchedAt ? relTime(now.value, resource.value.lastLaunchedAt) : '从没启动过',
)

async function launchIt() {
  if (!resource.value) return
  await api.resourceLaunch(resource.value.id)
  await load()
}
</script>

<template>
  <section class="y-detail">
    <header class="y-dh">
      <Sparkles :size="14" />
      <span class="t">详情</span>
      <span v-if="todo" class="id">TODO-{{ todo.id.slice(-4).toUpperCase() }} · rev {{ todo.rev }}</span>
    </header>

    <div class="y-db scroller">
      <template v-if="todo">
        <h2 class="dt">
          {{ todo.title }}
          <span v-if="todo.priority <= 1" class="y-chip r">{{ PRIORITY[todo.priority] }}优先级</span>
        </h2>
        <p v-if="todo.note" class="dd">{{ todo.note }}</p>

        <dl class="y-dl">
          <dt>状态</dt>
          <dd>
            <!-- 三态而不是两态：取消勾选后 doneAt 仍然留着，直说「上次完成于」比假装没发生过诚实 -->
            <span class="y-chip" :class="todo.completed ? 'a' : ''">
              <template v-if="todo.completed">完成于 {{ relTime(now, todo.doneAt!) }}</template>
              <template v-else-if="todo.doneAt">未完成 · 上次完成于 {{ relTime(now, todo.doneAt) }}</template>
              <template v-else>未完成</template>
            </span>
          </dd>

          <dt>截止</dt>
          <dd>{{ todo.dueAt ? dueLabel(now, todo.dueAt) : '无截止' }}</dd>

          <dt>清单</dt>
          <dd>
            <span class="y-chip">{{ todo.zone }}</span>
            <span v-for="tag in todo.tags" :key="tag" class="y-chip">{{ tag }}</span>
          </dd>

          <dt>创建</dt>
          <dd>{{ relTime(now, todo.createdAt) }} · {{ shortDevice(todo.createdBy) }}</dd>

          <dt>最后写入</dt>
          <dd>{{ shortDevice(todo.updatedBy) }} · {{ relTime(now, todo.updatedAt) }}</dd>
        </dl>

        <div v-if="children.length" class="kids">
          <div class="kh">子项 {{ children.length }}</div>
          <div v-for="child in children" :key="child.id" class="kr">
            <button
              type="button"
              class="kcb"
              :class="{ on: child.completed }"
              :aria-label="child.completed ? '取消完成' : '标为完成'"
              @click="api.todosSetDone(child.id, !child.completed).then(load)"
            />
            <span class="kt" :class="{ done: child.completed }">{{ child.title }}</span>
            <button type="button" class="kx" aria-label="删除子项" @click="removeChild(child)">×</button>
          </div>
        </div>
      </template>

      <template v-else-if="resource">
        <h2 class="dt">{{ resource.name }}</h2>
        <dl class="y-dl">
          <dt>类型</dt>
          <dd><span class="y-chip">{{ resource.kind }}</span></dd>
          <dt>目标</dt>
          <dd class="path" :title="resource.target">{{ resource.target }}</dd>
          <dt>用过</dt>
          <dd>{{ resource.useCount }} 次 · {{ launched }}</dd>
          <dt>rev</dt>
          <dd>{{ resource.rev }}</dd>
        </dl>
        <button v-if="resource.kind !== 'web'" type="button" class="y-ghost" @click="revealSecret">
          查看加密备注
        </button>
        <p v-if="secret !== null" class="dd">{{ secret === 'unavailable' ? '没有备注' : secret }}</p>
      </template>

      <template v-else-if="dayItems.length">
        <h2 class="dt">{{ dayLabel }}</h2>
        <p class="dd">这一天完成 {{ dayItems.length }} 项</p>
        <div class="kids">
          <div class="kh">完成记录</div>
          <div v-for="t in dayItems" :key="t.id" class="kr">
            <span class="kcb on" />
            <span class="kt" :class="{ done: t.completed }">{{ t.title }}</span>
            <button type="button" class="kx" aria-label="取消完成" @click="api.todosSetDone(t.id, false)">×</button>
          </div>
        </div>
      </template>

      <template v-else-if="cls">
        <h2 class="dt">{{ cls.name }}</h2>
        <dl class="y-dl">
          <dt>教师</dt><dd>{{ cls.teacher || '未写' }}</dd>
          <dt>教室</dt><dd>{{ cls.room || '未写' }}</dd>
          <dt>时间</dt><dd>{{ DAY_NAMES[cls.day - 1] }} {{ hhmm(cls.startMin) }}–{{ hhmm(cls.endMin) }}</dd>
          <dt>节次</dt><dd>{{ cls.period }}</dd>
          <dt>周次</dt><dd>第 {{ selWeek }} 周在 · 排 {{ cls.weeks }} 周</dd>
        </dl>
        <p class="dd">只读副本 · 改课要重新导入</p>
      </template>

      <div v-else class="y-empty">选中任何待办、速达条目，这里显示它的全貌与操作。</div>
    </div>

    <footer v-if="todo || resource" class="y-dfoot">
      <input
        v-if="todo"
        v-model="childDraft"
        class="palette-query kidin"
        placeholder="加一个子项，回车"
        @keydown.enter.prevent="addChild"
      />
      <button v-if="todo" type="button" class="y-solid" @click="toggleDone">
        {{ todo.completed ? '重新打开' : '标为完成' }}
      </button>
      <button v-if="resource" type="button" class="y-solid" @click="launchIt">启动</button>
    </footer>
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
.id {
  margin-left: auto;
  font-size: 10px;
  color: var(--t3);
  font-family: ui-monospace, monospace;
}
.y-db {
  flex: 1;
  min-height: 0;
  padding: 0 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.dt {
  font-size: 17px;
  font-weight: 850;
  letter-spacing: -0.025em;
  line-height: 1.3;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.dd {
  font-size: 12.5px;
  color: var(--t2);
  line-height: 1.55;
}
.y-dl {
  display: grid;
  grid-template-columns: 66px minmax(0, 1fr);
  gap: 7px 10px;
  margin-top: 2px;
}
.y-dl dt {
  font-size: 11px;
  color: var(--t3);
}
.y-dl dd {
  font-size: 11.5px;
  color: var(--t1);
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
}
.path {
  word-break: break-all;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  color: var(--t2);
}
.kids {
  border-top: 1px solid var(--line);
  padding-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.kh {
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.06em;
  color: var(--t3);
}
.kr {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
}
.kcb {
  width: 13px;
  height: 13px;
  border-radius: 4px;
  border: 1.5px solid var(--line-2);
  flex: none;
}
.kcb.on {
  background: var(--accent);
  border-color: transparent;
}
.kt {
  flex: 1;
  min-width: 0;
}
.kt.done {
  color: var(--t3);
  text-decoration: line-through;
}
.kx {
  color: var(--t3);
  font-size: 14px;
  line-height: 1;
}
.y-dfoot {
  flex: none;
  border-top: 1px solid var(--line);
  background: var(--card-2);
  padding: 9px 12px;
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.kidin {
  flex: 1 1 100%;
  padding: 6px 9px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--t1);
  font-size: 12px;
  outline: none;
}
</style>
