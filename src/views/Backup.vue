<script setup lang="ts">
import { CheckCircle2, Download, Loader2, PlugZap, ShieldCheck, UploadCloud, X } from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { usingMock } from '../api'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import { useBackup } from '../composables/useBackup'
import { useClock } from '../composables/useClock'
import { relTime } from '../lib/reltime'

/**
 * 备份页只做三件事：说清楚**要上传什么、不上传什么**，配一次凭据，然后能还原。
 *
 * 应用密码进 Windows 凭据管理器，不进 SQLite、不进备份；邮箱留在 settings，
 * 因为密钥是 `email:应用密码` 一起派生的，换机时要靠它解释凭据条目属于谁。
 */
const backup = useBackup()
const { now } = useClock()

const email = ref('')
const password = ref('')
const saved = ref(false)
/** 这次保存有没有真的动凭据管理器那条 —— 两种情况的回话不一样，说成一样就是假话 */
const keptExisting = ref(false)
const selfTestMsg = ref<string | null>(null)
const confirming = ref<string | null>(null)
const testing = ref(false)
let unlisten: (() => void) | null = null

const st = computed(() => backup.status.value)
const configured = computed(() => !!st.value?.configured)
const passwordStored = computed(() => !!st.value?.passwordStored)
const log = computed(() => st.value?.log ?? [])
const paths = computed(() => st.value?.paths ?? null)
const okCount = computed(() => log.value.filter((l) => l.ok).length)

/**
 * 失败条只在「最近一次确实还没缓过来」时出现：已被成功覆盖的旧失败，流水里那一行就够了，
 * 不该在页面顶上常驻一条红色警报。返回 status 本身，模板里 v-if 就能收窄类型。
 */
const staleFailure = computed(() => {
  const s = st.value
  if (!s?.lastError) return null
  const notRecovered = s.lastSuccessAt == null || (s.lastErrorAt ?? 0) > s.lastSuccessAt
  return notRecovered ? s : null
})

function kb(bytes: number | null): string {
  if (bytes == null) return '—'
  return bytes > 1024 ? `${(bytes / 1024).toFixed(0)} KB` : `${bytes} B`
}

/** 文件名里的 8 hex 是这份快照的指纹：anvil-<UTC 时间戳>-<8 hex>.abackup */
function tagOf(name: string | null): string {
  if (!name) return ''
  const m = /-([0-9a-f]{8})\.abackup$/.exec(name)
  return m ? m[1] : name.slice(-24)
}

/** 文件名里的 UTC 时间是这份快照唯一的出生证明：anvil-YYYYMMDDTHHMMSSZ-<8hex>.abackup */
function stampOf(name: string): number | null {
  const m = /^anvil-(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z-/.exec(name)
  if (!m) return null
  const [, y, mo, d, h, mi, s] = m
  return Date.UTC(+y, +mo - 1, +d, +h, +mi, +s)
}

function localOf(name: string): string {
  const ms = stampOf(name)
  if (ms == null) return '认不出时间'
  return new Date(ms).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

async function save() {
  const typed = password.value.trim() !== ''
  saved.value = await backup.saveCredentials(email.value, password.value)
  selfTestMsg.value = null
  if (saved.value) {
    keptExisting.value = !typed
    password.value = ''
  }
}

async function forget() {
  await backup.clearCredentials()
  saved.value = false
  keptExisting.value = false
  email.value = ''
  password.value = ''
}

/**
 * 自证是**本机算法链路**的往返，不联网、也不读凭据管理器 ——
 * 拿它当"凭据能用"的证据是假的（空密码它也说通过），所以回话里不许出现"凭据可用"。
 */
async function selfTest() {
  testing.value = true
  const ok = await backup.selfTest(email.value || st.value?.email || '', password.value)
  selfTestMsg.value = ok
    ? '本机 KDF + AES-GCM 往返通过：同样的邮箱 + 应用密码能解开同一份文件。这只证明算法链路，不证明凭据可用 —— 要试凭据点「测试连接」。'
    : null
  testing.value = false
}

async function restore(name: string) {
  confirming.value = null
  // 成功即重启（Rust 侧 app.restart()），失败才需要在这儿留一行字
  await backup.restore(name)
}

onMounted(async () => {
  await backup.load()
  email.value = st.value?.email ?? ''
  if (configured.value) void backup.refreshRemote()

  if (!usingMock) {
    const { listen } = await import('@tauri-apps/api/event')
    const stop = await listen('backup://result', () => {
      void backup.load()
      void backup.refreshRemote()
    })
    unlisten = () => stop()
  }
})

onBeforeUnmount(() => unlisten?.())
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>坚果云备份 <span class="sub">加密快照 · 独立目录</span></h2>
      <span v-if="st?.lastSuccessAt" class="ago">上次成功 {{ relTime(now, st.lastSuccessAt) }}</span>
      <span class="ago" :class="{ warn: !configured }">{{ configured ? '已配凭据' : '未配凭据' }}</span>
    </div>

    <div class="scope">
      <div class="col">
        <h3><ShieldCheck :size="13" /> 会上传</h3>
        <ul>
          <li>待办、笔记、速达条目、便签、倒计时</li>
          <li>主题与界面设置（不含任何绝对路径）</li>
          <li>整库 gzip 后 AES-256-GCM 加密，坚果云只见密文</li>
        </ul>
      </div>
      <div class="col">
        <h3><X :size="13" /> 不会上传</h3>
        <ul>
          <li>应用密码 —— 只在本机 Windows 凭据管理器里</li>
          <li>药箱库存缓存与健康数据（外部只读源不进云）</li>
          <li>本机绝对路径、坚果云邮箱、备份流水</li>
          <li>药箱的 <code>device-*.json</code>：目录不同，永不读写</li>
        </ul>
      </div>
    </div>

    <div v-if="staleFailure" class="err">
      最近一次失败 · {{ relTime(now, staleFailure.lastErrorAt ?? now) }}
      <b>{{ staleFailure.lastError }}</b>
      <span class="hint">失败不弹窗，一分钟内自动重试；连续失败按 2 倍退避，最长 30 分钟。</span>
    </div>

    <div class="blk">
      <h3>凭据</h3>
      <div class="form">
        <label class="fld">
          <span>坚果云账号（注册邮箱）</span>
          <input v-model="email" type="email" autocomplete="off" placeholder="me@example.com" />
        </label>
        <label class="fld">
          <span>第三方应用管理里的「应用密码」（不是登录密码）</span>
          <input
            v-model="password"
            type="password"
            autocomplete="new-password"
            :placeholder="passwordStored ? '留空 = 沿用已存那条' : '只写进凭据管理器'"
          />
        </label>
      </div>
      <p v-if="passwordStored" class="why">
        应用密码已在本机 Windows 凭据管理器里（服务 <code>Anvil</code> · 账号 <code>webdav</code>）。
        它不回显，所以换一页再回来框里是空的 —— 那不是丢了；要换密码才需要重新填。
      </p>
      <div class="row">
        <button type="button" class="y-ghost btn" :disabled="!email" @click="save">
          <CheckCircle2 :size="13" /> 保存凭据
        </button>
        <button type="button" class="y-ghost btn" :disabled="!password" @click="selfTest">
          <Loader2 v-if="testing" :size="13" class="spin" />
          <ShieldCheck v-else :size="13" /> 加解密自证
        </button>
        <button type="button" class="y-ghost btn" :disabled="!configured" @click="backup.testConnection()">
          <PlugZap :size="13" /> 测试连接
        </button>
        <button v-if="configured" type="button" class="y-ghost btn" @click="forget">清除</button>
      </div>
      <p v-if="saved" class="ok">
        {{
          keptExisting
            ? '邮箱已存；应用密码沿用凭据管理器里那条，没动它。'
            : '两条都写好了：邮箱进设置，应用密码只进 Windows 凭据管理器 —— 不进数据库，备份文件里也没有它。'
        }}
      </p>
      <p v-if="selfTestMsg" class="ok">{{ selfTestMsg }}</p>
      <p v-if="backup.notice.value" class="ok">{{ backup.notice.value }}</p>
      <p v-if="backup.error.value" class="bad">{{ backup.error.value }}</p>
    </div>

    <div class="blk">
      <h3>动作</h3>
      <div class="row">
        <button type="button" class="y-solid btn" :disabled="!configured || backup.busy.value.run" @click="backup.runNow()">
          <UploadCloud :size="14" />
          {{ backup.busy.value.run ? '正在加密上传…' : '立即备份' }}
        </button>
        <button type="button" class="y-ghost btn" :disabled="backup.busy.value.remote" @click="backup.refreshRemote()">
          {{ backup.busy.value.remote ? '列目录中…' : '刷新远端清单' }}
        </button>
      </div>
      <p v-if="backup.result.value?.ok" class="ok">
        {{ backup.result.value.remotePath }}
        <template v-if="backup.result.value.bytes"> · {{ kb(backup.result.value.bytes) }}</template>
      </p>
      <p v-else-if="backup.result.value && !backup.result.value.skipped" class="bad">
        这次没成：{{ backup.result.value.error }}
      </p>
    </div>

    <div class="blk">
      <h3>
        远端快照
        <span class="n">{{ configured ? `${backup.remote.value.length} 份` : '未连接 · 数量未知' }}</span>
      </h3>
      <CollapsedRow
        v-if="!configured"
        label="备份联动 · 还没配坚果云凭据"
        detail="填完上面两个框，这里才会列出远端文件"
      />
      <CollapsedRow
        v-else-if="!backup.remote.value.length && !backup.busy.value.remote"
        label="远端还没有工作台的快照"
        detail="点「立即备份」生成第一份"
        :busy="backup.busy.value.remote"
        @retry="backup.refreshRemote()"
      />
      <ul v-else class="list">
        <li v-for="f in backup.remote.value" :key="f.name" class="it">
          <span class="nm" :title="f.name">{{ localOf(f.name) }}</span>
          <span class="sz">{{ kb(f.size) }}</span>
          <span class="id">{{ tagOf(f.name) }}</span>
          <template v-if="confirming === f.name">
            <span class="ask">覆盖当前库？旧库会改名留着</span>
            <button type="button" class="y-ghost btn tiny" :disabled="backup.busy.value.restore" @click="restore(f.name)">
              确认还原
            </button>
            <button type="button" class="y-ghost btn tiny" @click="confirming = null">取消</button>
          </template>
          <button v-else type="button" class="y-ghost btn tiny" :disabled="!f.readable" @click="confirming = f.name">
            <Download :size="13" /> 还原
          </button>
        </li>
      </ul>
    </div>

    <div class="blk">
      <h3>流水 <span class="n">{{ okCount }}/{{ log.length }} 成功</span></h3>
      <ul v-if="log.length" class="list">
        <li v-for="(l, i) in log" :key="`${l.at}-${i}`" class="lg">
          <i class="dot" :class="l.ok ? 'y' : 'r'"></i>
          <span class="kind">{{ l.kind === 'manual' ? '手动' : l.kind === 'restore' ? '还原' : '每日' }}</span>
          <span class="when">{{ new Date(l.at).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) }}</span>
          <span class="sz">{{ kb(l.bytes) }}</span>
          <span class="msg">{{ l.ok ? tagOf(l.remotePath) : l.error }}</span>
        </li>
      </ul>
      <p v-else class="none">还没有备份记录。开启后每天第一次运行时会自动补一份。</p>
    </div>

    <div v-if="paths" class="where">
      <span>库文件</span><code>{{ paths.db }}</code>
      <span>设备 id 文件</span><code>{{ paths.deviceId }}</code>
    </div>
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
  flex: 1;
  overflow-y: auto;
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
.ago {
  margin-left: auto;
  font-size: 11px;
  color: var(--t3);
}
.ago.warn {
  color: var(--amber);
}

.scope {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.scope .col {
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 10px 12px;
}
.scope h3 {
  font-size: 11.5px;
  font-weight: 750;
  color: var(--t2);
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}
.scope ul {
  margin: 0;
  padding-left: 15px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.scope li {
  font-size: 11.5px;
  color: var(--t2);
  line-height: 1.5;
}
.scope code {
  font-size: 11px;
  color: var(--t1);
}

.err {
  font-size: 11.5px;
  color: var(--t2);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border-radius: var(--r-sm);
  padding: 9px 11px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.err b {
  color: var(--t1);
  font-weight: 600;
}
.err .hint {
  font-size: 10.5px;
  color: var(--t3);
}

.blk {
  display: flex;
  flex-direction: column;
  gap: 7px;
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
}
.btn {
  flex: none;
  height: 30px;
  width: auto;
  padding: 0 12px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.btn:disabled {
  opacity: 0.45;
}
.btn.tiny {
  height: 25px;
  padding: 0 8px;
  font-size: 11px;
}
.spin {
  animation: rot 1.1s linear infinite;
}
@keyframes rot {
  to {
    transform: rotate(360deg);
  }
}

.ok,
.bad {
  font-size: 11.5px;
  margin: 0;
  word-break: break-all;
}
/* 表单底下那行解释：次要信息，不该比输入框本身还抢眼 */
.why {
  font-size: 11px;
  color: var(--t3);
  line-height: 1.6;
  margin: 0;
}
.ok {
  color: var(--accent);
}
.bad {
  color: var(--red);
}
/* 空态不是成功，别用 accent 绿去报「什么都没有」 */
.none {
  font-size: 11.5px;
  margin: 0;
  color: var(--t3);
}

.list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.it,
.lg {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--card);
  font-size: 11.5px;
  color: var(--t2);
}
.it .nm {
  font-weight: 650;
  color: var(--t1);
  font-variant-numeric: tabular-nums;
}
.sz,
.id,
.when,
.kind {
  font-variant-numeric: tabular-nums;
  color: var(--t3);
  font-size: 11px;
}
.ask {
  margin-left: auto;
  font-size: 11px;
  color: var(--amber);
}
.it .y-ghost:last-child {
  margin-left: 0;
}
.lg .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: none;
}
.dot.y {
  background: var(--accent);
}
.dot.r {
  background: var(--red);
}
.lg .msg {
  margin-left: auto;
  color: var(--t3);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 46%;
}

.where {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 10.5px;
  color: var(--t3);
  flex-wrap: wrap;
}
.where code {
  font-size: 10.5px;
  color: var(--t2);
  user-select: text;
}

@media (max-width: 900px) {
  .scope,
  .form {
    grid-template-columns: 1fr;
  }
}
</style>
