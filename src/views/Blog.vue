<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { ExternalLink, RefreshCw } from 'lucide-vue-next'

import { api } from '../api'
import type { BlogPayload } from '../api/types'
import CollapsedRow from '../components/ui/CollapsedRow.vue'
import { useClock } from '../composables/useClock'
import { useLinked } from '../composables/useLinked'
import { relTime } from '../lib/reltime'

interface Project {
  name: string
  description?: string
  tags?: string[]
  url?: string
  status?: string
}
interface Repo {
  name: string
  description?: string
  language?: string
  stars?: number
  pushedAt?: string
  archived?: boolean
  url?: string
}
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

const linked = useLinked()
const { now } = useClock()

const projects = linked.payloadOf<BlogPayload<Project>>('projects')
const repos = linked.payloadOf<BlogPayload<Repo>>('repos')
const photos = linked.payloadOf<BlogPayload<Photo>>('photos')
const friends = linked.payloadOf<BlogPayload<Friend>>('friends')
const profile = linked.payloadOf<BlogPayload<Profile>>('profile')

const stale = computed(() => linked.blogSnapshots.value.find((s) => s.status === 'stale') ?? null)
const lastFetch = computed(() => {
  const times = linked.blogSnapshots.value.map((s) => s.fetchedAt)
  return times.length ? Math.max(...times) : null
})

/** 博客正文（notes）在对方仓库里已被清空，这里只列可显示的类别，不硬凑空页 */
const nothingToShow = computed(
  () =>
    !projects.value?.count &&
    !repos.value?.count &&
    !photos.value?.count &&
    !friends.value?.count &&
    !profile.value?.count,
)

function open(url?: string) {
  // 系统默认浏览器，不在 webview 里开新窗口
  if (url) void api.openUrl(url)
}

onMounted(async () => {
  if (!linked.state.value) await linked.load()
  void linked.refreshBlog()
})
</script>

<template>
  <section class="y-view">
    <div class="bar">
      <h2>博客联动 <span class="sub">读本地仓库，只读</span></h2>
      <span v-if="lastFetch" class="ago">缓存于 {{ relTime(now, lastFetch) }}</span>
      <button type="button" class="y-ghost btn" :disabled="linked.busy.value.blog" @click="linked.refreshBlog()">
        <RefreshCw :size="13" />
        重新读取
      </button>
      <button type="button" class="y-ghost btn" :disabled="linked.busy.value.online" @click="linked.checkOnline()">
        线上比对
      </button>
    </div>

    <p v-if="stale" class="stale">
      {{ stale.kind }}：{{ stale.detail || '网站版本较新，本地缓存还没同步' }} —— 工作台读的是缓存，没写任何东西到站点。
    </p>

    <CollapsedRow
      v-if="nothingToShow"
      label="博客联动 · 本地仓库里没有可显示的清单"
      :detail="linked.state.value?.paths.blogRepoExists ? undefined : '本地仓库目录不可用'"
      :fetched-at="lastFetch"
      :busy="linked.busy.value.blog"
      @retry="linked.refreshBlog()"
    />

    <template v-else>
      <section v-if="projects?.count" class="blk">
        <h3>项目 <span class="n">{{ projects.count }}</span></h3>
        <div class="cards">
          <button v-for="p in projects.items" :key="p.name" type="button" class="card" @click="open(p.url)">
            <span class="ct">{{ p.name }}<ExternalLink v-if="p.url" :size="12" class="el" /></span>
            <span v-if="p.description" class="cd">{{ p.description }}</span>
            <span class="cm">
              <span v-if="p.status" class="y-chip">{{ p.status }}</span>
              <span v-for="t in p.tags ?? []" :key="t" class="y-chip">{{ t }}</span>
            </span>
          </button>
        </div>
      </section>

      <section v-if="repos?.count" class="blk">
        <h3>仓库 <span class="n">{{ repos.count }}</span></h3>
        <div class="rows">
          <button v-for="r in repos.items" :key="r.name" type="button" class="rr" @click="open(r.url)">
            <span class="rn">{{ r.name }}</span>
            <span class="rd">{{ r.description }}</span>
            <span class="rm">
              <span v-if="r.language">{{ r.language }}</span>
              <span v-if="r.stars">★ {{ r.stars }}</span>
              <span v-if="r.pushedAt">{{ r.pushedAt.slice(0, 10) }}</span>
              <span v-if="r.archived" class="arch">已归档</span>
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
.btn {
  flex: none;
  height: 27px;
  width: auto;
  padding: 0 10px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.stale {
  font-size: 11.5px;
  color: var(--t2);
  background: var(--accent-soft);
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
.rm {
  display: flex;
  gap: 9px;
  font-size: 10.5px;
  color: var(--t3);
}
.arch {
  color: var(--amber);
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
