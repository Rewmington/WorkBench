<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { CloudDownload, HardDrive, RefreshCw } from 'lucide-vue-next'

import { api } from '../api'
import AdherenceRings from '../components/medicine/AdherenceRings.vue'
import StockBars from '../components/medicine/StockBars.vue'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import PathField from '../components/ui/PathField.vue'
import UndoBar from '../components/ui/UndoBar.vue'
import { useClock } from '../composables/useClock'
import { useLinked } from '../composables/useLinked'
import { useShell } from '../composables/useShell'
import { useUndo } from '../composables/useUndo'
import { relTime } from '../lib/reltime'
import { errText } from '../lib/err'

/**
 * 药箱是**只读源**：复制它的库再打开，绝不写回去。
 * 它的同步是「每设备全量快照 + 按 updated_at 行级 LWW」且只在启动/手动时跑，
 * 我们写进去的东西会被下一次合并覆盖掉 —— 所以「补买」是往我们自己的待办里写。
 *
 * 坚果云那份是**备用源**：只有配了第二条应用密码才启用，和本机副本取更新的那一份，
 * 用了哪份必须写在界面上（云端那份可能是几小时前发布的）。
 */
const linked = useLinked()
const shell = useShell()
const undo = useUndo()
const { now } = useClock()

const snap = computed(() => linked.medicineSnap.value)
const data = computed(() => linked.medicine.value)
const items = computed(() => data.value?.items ?? [])
const meta = computed(() => data.value?.meta ?? {})
const paths = computed(() => linked.state.value?.paths ?? null)

const lowOnes = computed(() => items.value.filter((m) => m.isLow || m.isOut))

/** 空 detail = 只走了本机那条路（Rust 在那一分支上不写任何话），不是「读不到」 */
const sourceLine = computed(() => {
  const s = snap.value
  if (!s) return '还没读过药箱'
  const read = `读取于 ${relTime(now.value, s.fetchedAt)}`
  return s.detail ? `${s.detail} · ${read}` : `本机副本 · ${read}`
})
const fromCloud = computed(() => /^(仅?云端)/.test(snap.value?.detail ?? ''))

function stockTone(daysLeft: number | null, lowDays: number) {
  if (daysLeft == null) return 'unknown'
  if (daysLeft <= 0) return 'out'
  if (daysLeft <= lowDays) return 'low'
  return 'ok'
}

async function restock(name: string, daysLeft: number | null) {
  const todo = await api.linkedRestockTodo(name, daysLeft)
  await shell.go('todos')
  undo.offer({
    label: `已建「${todo.title}」`,
    run: async () => {
      const affected = await api.todosDelete(todo.id)
      await api.todosRestore(affected)
    },
  })
}

// ---- 云端备用源的配置表单 ----

const EMAIL_KEY = 'linked.medicine.email'

const email = ref('')
const password = ref('')
const saving = ref(false)
const notice = ref<string | null>(null)
const error = ref<string | null>(null)
const cloudReady = computed(() => paths.value?.medicineCloudReady ?? false)

/**
 * 存完立刻刷一次：能不能读上是发一个请求才知道的事，
 * 放在这里做，用户当场就能看到 detail 变成「云端这次没读上：…」，而不是等下次进这一页。
 */
async function saveCred() {
  saving.value = true
  notice.value = null
  error.value = null
  const typed = password.value.trim() !== ''
  try {
    await api.linkedSetMedicinePassword(email.value, password.value)
    password.value = ''
    notice.value = typed ? '已存进凭据管理器，并读了一次。' : '邮箱已存，密码沿用那条。'
    await linked.refreshMedicine()
    await linked.load()
  } catch (e) {
    error.value = errText(e)
  } finally {
    saving.value = false
  }
}

async function forgetCred() {
  error.value = null
  try {
    await api.linkedClearMedicinePassword()
    email.value = ''
    password.value = ''
    notice.value = '已清除，进度回到只看本机副本。'
    await linked.refreshMedicine()
    await linked.load()
  } catch (e) {
    error.value = errText(e)
  }
}

/** 两个路径都交给 PathField：同一套「保存 / 恢复默认 / 后端原话」措辞，不再各写一遍 */
async function saveDavDir(raw: string): Promise<string> {
  const next = await api.linkedSetMedicineDir(raw)
  await linked.load()
  return next
}

async function saveDbPath(raw: string): Promise<string> {
  const p = await api.linkedSetMedicineDb(raw)
  await linked.load()
  return p.medicineDb
}

onMounted(async () => {
  if (!linked.state.value) await linked.load()
  email.value = String((await api.settingsGet(EMAIL_KEY)) ?? '')
  void linked.refreshMedicine()
})
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>药箱联动 <span class="sub">只读 · 不写回</span></h2>
      <span class="src" :class="{ alt: fromCloud }" :title="sourceLine">{{ sourceLine }}</span>
      <button type="button" class="y-ghost btn" :disabled="linked.busy.value.medicine" @click="linked.refreshMedicine()">
        <RefreshCw :size="13" />
        重新读取
      </button>
    </div>

    <CollapsedRow
      v-if="!items.length"
      :label="`药箱联动 · ${snap?.status === 'missing' ? '没找到药箱的库' : snap?.status === 'error' ? '副本读不了' : '还没有药品'}`"
      :detail="snap?.detail"
      :fetched-at="snap?.fetchedAt"
      :busy="linked.busy.value.medicine"
      @retry="linked.refreshMedicine()"
    />

    <template v-else>
      <div class="gauge">
        <AdherenceRings
          class="rings"
          :today-taken="meta.todayTaken ?? 0"
          :today-owed="meta.todayOwed ?? 0"
          :week-rate="meta.weekRate ?? 0"
          :has-week-plan="(meta.weekOwed ?? 0) > 0 || (meta.weekSkipped ?? 0) > 0"
          :streak-days="meta.streakDays ?? 0"
        />
        <StockBars class="stock" :items="items" />
      </div>

      <div class="sum">
        <span v-if="lowOnes.length" class="s warn">余量偏低 <b>{{ lowOnes.length }}</b> 种</span>
        <span class="s">共 {{ items.length }} 种药</span>
      </div>

      <div v-for="m in items" :key="m.id" class="med">
        <div class="mh">
          <span class="nm">{{ m.name }}</span>
          <span class="y-chip" :class="stockTone(m.daysLeft, m.lowDays) === 'low' ? 'r' : stockTone(m.daysLeft, m.lowDays) === 'out' ? 'r' : 'a'">
            {{ m.daysLeft == null ? '未设用量' : m.isOut ? '已用完' : `剩 ${m.daysLeft} 天` }}
          </span>
          <button
            v-if="m.isLow || m.isOut"
            type="button"
            class="y-ghost buy"
            @click="restock(m.name, m.daysLeft)"
          >
            补买
          </button>
        </div>

        <div class="slots">
          <span v-for="s in m.slots" :key="s.minute" class="slot" :class="s.state">
            <i class="dot"></i>{{ s.label }}
            <em>{{ s.state === 'taken' ? '已记' : s.state === 'pending' ? '待记' : s.state === 'skipped' ? '没吃' : '稍后' }}</em>
          </span>
        </div>

        <div class="foot">
          <span>{{ m.remaining }} {{ m.unit }}<template v-if="m.boxes"> · 约 {{ m.boxes }} 盒</template></span>
          <span v-if="m.dailyUse">每天 {{ m.dailyUse }} {{ m.unit }}</span>
          <span>预警线 {{ m.lowDays }} 天</span>
        </div>
      </div>

      <!-- 只说"读的是哪一份"：路径必须是它真正读的那个，否则就是假话（§2 原则 5）。
           怎么读的（复制→只读→删）搬去了设置页「关于与隐私」。 -->
      <p class="note">
        <template v-if="fromCloud">读的是坚果云上的快照 <code>{{ paths?.medicineDav }}</code></template>
        <template v-else>读的是 <code>{{ paths?.medicineDb }}</code> 的副本</template>
      </p>
    </template>

    <div v-if="paths" class="blk">
      <h3>
        <HardDrive :size="13" /> 本机库位置
        <span class="n">{{ paths.medicineDbExists ? '读得到' : '这个位置没有药箱的库' }}</span>
      </h3>
      <PathField
        label="药箱的 SQLite 库"
        :value="paths.medicineDb"
        :fallback="paths.medicineDbDefault"
        :save="saveDbPath"
      />
    </div>

    <div class="blk">
      <h3>
        <CloudDownload :size="13" /> 坚果云备用源
        <span class="n">{{ cloudReady ? '已配应用密码' : '未配 · 只读本机副本' }}</span>
      </h3>
      <p class="why">这台机器没装药箱，或手机那边记得比本机新时才有用。</p>

      <div class="form">
        <label class="fld">
          <span>坚果云注册邮箱</span>
          <input v-model="email" type="email" autocomplete="off" placeholder="me@example.com" />
        </label>
        <label class="fld">
          <span>应用密码（不是登录密码）</span>
          <input
            v-model="password"
            type="password"
            autocomplete="new-password"
            :placeholder="cloudReady ? '留空 = 沿用已存那条' : '只写进凭据管理器'"
          />
        </label>
      </div>
      <p v-if="cloudReady" class="why">密码不回显，留空就是沿用已存那条。</p>
      <div class="row">
        <button
          type="button"
          class="y-ghost btn"
          :disabled="!email.trim() || (!password.trim() && !cloudReady) || saving"
          @click="saveCred"
        >
          {{ saving ? '正在读取云端…' : '保存并立即读一次' }}
        </button>
        <button v-if="cloudReady" type="button" class="y-ghost btn" @click="forgetCred">清除</button>
      </div>

      <PathField
        label="药箱的同步目录"
        :value="paths?.medicineDav ?? ''"
        :fallback="paths?.medicineDavDefault ?? ''"
        :save="saveDavDir"
      />

      <p v-if="notice" class="ok">{{ notice }}</p>
      <p v-if="error" class="bad">{{ error }}</p>
    </div>

    <UndoBar />
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 11px;
}
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
h2 {
  font-size: 13.5px;
  font-weight: 800;
}
.sub {
  font-size: 11px;
  font-weight: 500;
  color: var(--t3);
}
.src {
  margin-left: auto;
  font-size: 11px;
  color: var(--t3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
/* 用了云端那份 = 可能不是此刻本机的事实，措辞已经写在文字里，颜色只是加一层 */
.src.alt {
  color: var(--amber);
}
.btn {
  flex: none;
  height: 27px;
  width: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.gauge {
  display: flex;
  align-items: center;
  gap: 16px;
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: var(--r-sm);
  padding: 12px 13px;
}
.rings {
  width: 288px;
  flex: none;
}
.stock {
  flex: 1;
  min-width: 0;
  border-left: 1px solid var(--line);
  padding-left: 16px;
}
/* 窄一档横着排会把条挤成没读相的细线，改成上下两段 */
@media (max-width: 1000px) {
  .gauge {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
  }
  .rings {
    width: auto;
  }
  .stock {
    border-left: 0;
    padding-left: 0;
    border-top: 1px solid var(--line);
    padding-top: 12px;
  }
}
.sum {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.s {
  font-size: 11.5px;
  color: var(--t2);
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: 9px;
  padding: 6px 11px;
}
.s b {
  font-size: 13px;
  font-weight: 800;
  color: var(--t1);
}
.s.warn b {
  color: var(--amber);
}
.med {
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: var(--r);
  padding: 12px 13px;
  display: flex;
  flex-direction: column;
  gap: 9px;
}
.mh {
  display: flex;
  align-items: center;
  gap: 8px;
}
.nm {
  font-size: 13px;
  font-weight: 750;
}
.buy {
  margin-left: auto;
  flex: none;
  width: auto;
  height: 24px;
  padding: 0 10px;
  font-size: 11.5px;
}
.slots {
  display: flex;
  gap: 7px;
  flex-wrap: wrap;
}
.slot {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  padding: 4px 9px;
  border-radius: 7px;
  background: var(--card-2);
  border: 1px solid var(--line);
  color: var(--t2);
}
.slot .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--t3);
}
.slot em {
  font-style: normal;
  color: var(--t3);
  font-size: 10px;
}
.slot.taken {
  border-color: color-mix(in srgb, var(--accent) 30%, transparent);
}
.slot.taken .dot {
  background: var(--accent);
}
.slot.taken em {
  color: var(--accent);
}
/* 待记：不只换个颜色，文字与位置都在 —— 状态不只靠颜色（§2 原则 3） */
.slot.pending {
  background: rgba(248, 113, 113, 0.1);
}
.slot.pending .dot {
  background: var(--red);
}
.slot.pending em {
  color: var(--red);
  font-weight: 800;
}
.slot.skipped .dot {
  background: transparent;
  border: 1px solid var(--t3);
}
.slot.upcoming {
  opacity: 0.62;
}
.foot {
  display: flex;
  gap: 12px;
  font-size: 10.5px;
  color: var(--t3);
}
.note {
  font-size: 11px;
  color: var(--t3);
  line-height: 1.6;
}
.blk {
  display: flex;
  flex-direction: column;
  gap: 8px;
  border-top: 1px solid var(--line);
  padding-top: 12px;
}
.blk h3 {
  font-size: 12px;
  font-weight: 750;
  color: var(--t2);
  display: flex;
  align-items: center;
  gap: 6px;
}
.n {
  font-size: 10.5px;
  color: var(--t3);
  font-weight: 500;
  margin-left: auto;
}
.why {
  font-size: 11px;
  color: var(--t3);
  line-height: 1.6;
}
.why b {
  color: var(--t2);
  font-weight: 700;
}
.form {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.fld {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.fld span {
  font-size: 10.5px;
  color: var(--t3);
}
.fld input {
  height: 30px;
  min-width: 0;
  flex: 1;
  padding: 0 10px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line-2);
  background: var(--card);
  color: var(--t1);
  font-size: 12px;
  user-select: text;
}
.fld input:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}
.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.ok,
.bad {
  font-size: 11px;
  line-height: 1.5;
  border-radius: var(--r-sm);
  padding: 7px 10px;
}
.ok {
  color: var(--t2);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
}
.bad {
  color: var(--t1);
  background: color-mix(in srgb, var(--red) 12%, transparent);
}
code {
  font-family: ui-monospace, monospace;
  font-size: 10.5px;
  color: var(--t2);
  word-break: break-all;
}
</style>
