<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { ExternalLink, RefreshCw } from 'lucide-vue-next'

import { api } from '../api'
import type { BlogPayload } from '../api/types'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import PathField from '../components/ui/PathField.vue'
import { useClock } from '../composables/useClock'
import { useLinked } from '../composables/useLinked'
import { mergeFromPayloads, type Projectish } from '../lib/blog'
import { relTime } from '../lib/reltime'

interface Photo {
  id: string
  caption?: string
  date?: string
  location?: string
}
interface Friend {
  name: string
  url: string
  bio?: string
}
interface Profile {
  authorName?: string
  bio?: string
  techStack?: string[]
}

const KIND_LABELS: Record<string, string> = {
  projects: '项目',
  repos: '仓库',
  photos: '照片',
  friends: '友链',
  profile: '关于我',
  music: '音乐',
  notes: '文章',
}

const linked = useLinked()
const { now } = useClock()

const projects = linked.payloadOf<BlogPayload<Projectish>>('projects')
const repos = linked.payloadOf<BlogPayload<Projectish>>('repos')
const photos = linked.payloadOf<BlogPayload<Photo>>('photos')
const friends = linked.payloadOf<BlogPayload<Friend>>('friends')
const profile = linked.payloadOf<BlogPayload<Profile>>('profile')

/** 线上是主源（§6.8）。这几类没够到线上、退回本机仓库时，必须一行说出来是哪几类、为什么。 */
const fallbacks = computed(() =>
  linked.blogSnapshots.value.filter((s) => s.status === 'ok' && s.detail !== '' && s.detail !== '线上'),
)
const lastFetch = computed(() => {
  const times = linked.blogSnapshots.value.map((s) => s.fetchedAt)
  return times.length ? Math.max(...times) : null
})

/**
 * 一张「项目」卡，口径抄站点 `src/lib/projects.ts`：GitHub 仓库打头，`projects.json`
 * 只是覆盖表（改显示名/简介/标签/状态，不顶掉仓库行），GitHub 没有的排在后面。
 * 原来拆成「项目 1 条 / 仓库 4 条」两张卡，于是"我的项目"看着只有 1 个 —— 那不是数据旧，
 * 是我把一件事摊成了两件。
 */
const allProjects = computed<Projectish[]>(() => mergeFromPayloads(projects.value, repos.value))

/** 覆盖行与仓库行的字段凑在一排胶囊里 */
function chipsOf(p: Projectish): string[] {
  return [
    p.status,
    ...(p.tags ?? []),
    p.language,
    p.stars ? `★ ${p.stars}` : null,
    p.pushedAt ? p.pushedAt.slice(0, 10) : null,
    p.archived ? '已归档' : null,
  ].filter((c): c is string => !!c && c.trim() !== '')
}

/** 站点 /projects/ 只有这一屏，所以不再单列「仓库」；照片/友链/关于仍是各自独立的清单 */
const nothingToShow = computed(
  () =>
    !allProjects.value.length &&
    !photos.value?.count &&
    !friends.value?.count &&
    !profile.value?.count,
)

/**
 * 空页必须说"缺什么"，而不是给一个看着像"没有内容"的 0（PLAN §6.8 末）：
 * detail 里带的就是两边各自的实际情况。
 */
const emptyReason = computed(() => {
  if (!linked.state.value?.paths.blogRepoExists) return '本地仓库目录不可用'
  return linked.blogSnapshots.value.find((b) => b.status !== 'ok')?.detail ?? '这个源没有工作台认的清单'
})

function open(url?: string) {
  // 系统默认浏览器，不在 webview 里开新窗口
  if (url) void api.openUrl(url)
}

/**
 * 进这一页**不再自己起一轮拉取**。拉取是应用的事：`App.vue` 挂载时拉一次、之后每 10 分钟一次，
 * 这一页只渲染那一份缓存（点「重新读取」仍然随时可读）。
 * 原来这里也挂一次，于是启动那一轮还在跑的时候进页面就会并发第二条连接 ——
 * 站点会把同一时刻的多条新连接排队成 `timeout: connect`，正是 §6.8 刚量出来的那个降级。
 */
onMounted(async () => {
  if (!linked.state.value) await linked.load()
})

/**
 * 换机或仓库被挪走时要能改路径。命令 M3 就有，界面一直没接 ——
 * 于是别人拿到这个应用，博客永远显示「本地仓库目录不可用」且无从修正。
 */
async function saveRepoDir(raw: string): Promise<string> {
  const p = await api.linkedSetBlogDir(raw)
  await linked.load()
  return p.blogRepo
}
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>博客联动 <span class="sub">读本站公开的清单 · 只读</span></h2>
      <span v-if="lastFetch" class="ago">读取于 {{ relTime(now, lastFetch) }}</span>
      <button type="button" class="y-ghost btn" :disabled="linked.busy.value.blog" @click="linked.refreshBlog()">
        <RefreshCw :size="13" />
        重新读取
      </button>

    </div>

    <p v-if="fallbacks.length" class="stale">
      {{ fallbacks.map((s) => KIND_LABELS[s.kind] ?? s.kind).join('、') }} 读的是本机仓库 —— {{ fallbacks[0].detail.replace(/^本机仓库 · /, '') }}
    </p>

    <CollapsedRow
      v-if="nothingToShow"
      label="博客联动 · 线上与本机仓库都没有可显示的清单"
      :detail="emptyReason"
      :fetched-at="lastFetch"
      :busy="linked.busy.value.blog"
      @retry="linked.refreshBlog()"
    />

    <template v-else>
      <section v-if="allProjects.length" class="blk">
        <h3>项目 <span class="n">{{ allProjects.length }}</span></h3>
        <div class="cards">
          <button v-for="p in allProjects" :key="p.name" type="button" class="card" @click="open(p.url)">
            <span class="ct">{{ p.name }}<ExternalLink v-if="p.url" :size="12" class="el" /></span>
            <span v-if="p.description" class="cd">{{ p.description }}</span>
            <span class="cm">
              <span v-for="c in chipsOf(p)" :key="c" class="y-chip">{{ c }}</span>
            </span>
          </button>
        </div>
      </section>

      <div class="two">
        <section v-if="photos?.count" class="blk">
          <h3>照片 <span class="n">{{ photos.count }}</span></h3>
          <!-- 不拉远端图片：离线时一个破图框正是 §8.3.3 要消灭的死白 -->
          <div v-for="ph in photos.items" :key="ph.id" class="ph">
            <span class="pt">{{ ph.caption || ph.id }}</span>
            <span class="pm">{{ ph.date }}{{ ph.location ? ` · ${ph.location}` : '' }}</span>
          </div>
        </section>

        <section v-if="friends?.count" class="blk">
          <h3>友链 <span class="n">{{ friends.count }}</span></h3>
          <button v-for="f in friends.items" :key="f.name" type="button" class="rr" @click="open(f.url)">
            <span class="rn">{{ f.name }}</span>
            <span class="rd">{{ f.bio }}</span>
          </button>
        </section>
      </div>

      <section v-if="profile?.count" class="blk">
        <h3>关于</h3>
        <p class="pf">{{ profile.items[0]?.bio }}</p>
        <span class="cm">
          <span v-for="t in profile.items[0]?.techStack ?? []" :key="t" class="y-chip">{{ t }}</span>
        </span>
      </section>
    </template>

    <section v-if="linked.state.value" class="blk">
      <h3>
        本机仓库位置
        <span class="n">改这里只是告诉工作台去哪儿读，不会动到仓库本身</span>
      </h3>
      <PathField
        label="博客仓库目录（读它的 src/content/*.json）"
        :value="linked.state.value.paths.blogRepo"
        :fallback="linked.state.value.paths.blogRepoDefault"
        :save="saveRepoDir"
      />
    </section>
  </section>
</template>

<style scoped>
.y-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
.btn {
  flex: none;
  height: 27px;
  width: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
/* 退回本机仓库不是好消息：用 amber，别拿 accent 绿去报"降级了"（§8.6 同一条纪律） */
.stale {
  font-size: 11.5px;
  color: var(--t2);
  background: color-mix(in srgb, var(--amber) 14%, transparent);
  border-radius: var(--r-sm);
  padding: 8px 11px;
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
  font-variant-numeric: tabular-nums;
}
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 9px;
}
.card,
.rr {
  text-align: left;
  border: 1px solid var(--line);
  background: var(--card);
  border-radius: 12px;
  padding: 11px 12px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.card:hover,
.rr:hover {
  border-color: var(--line-2);
}
.ct {
  font-size: 12.5px;
  font-weight: 750;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.el {
  color: var(--t3);
}
.cd,
.rd {
  font-size: 11.5px;
  color: var(--t2);
  line-height: 1.5;
}
.cm {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
}
.rows {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.rn {
  font-size: 12px;
  font-weight: 700;
}
.two {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 12px;
}
.ph {
  border-left: 2px solid var(--line-2);
  padding: 2px 0 2px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.pt {
  font-size: 12px;
}
.pm {
  font-size: 10.5px;
  color: var(--t3);
}
.pf {
  font-size: 12px;
  color: var(--t2);
}
</style>
