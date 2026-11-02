import { computed, ref } from 'vue'

import { api } from '../api'
import type { BlogPayload, LinkedState, MedicineTodayPayload, SnapshotView } from '../api/types'

/**
 * 外部联动的读模型。
 *
 * 首屏只读 SQLite 缓存（`load`），刷新与线上比对都在后台跑并回推事件 ——
 * 站点首包 8 秒是实测过的，把它放在渲染路径上就等于让界面转圈。
 */
const state = ref<LinkedState | null>(null)
const busy = ref<{ blog: boolean; medicine: boolean }>({ blog: false, medicine: false })

function parse<T>(snap: SnapshotView | undefined): T | null {
  if (!snap || snap.status === 'missing') return null
  try {
    return JSON.parse(snap.payloadJson) as T
  } catch {
    // 缓存里的 JSON 坏了不是世界末日：折叠成一行说明就够了
    return null
  }
}

export function useLinked() {
  async function load() {
    state.value = await api.linkedState()
  }

  async function refreshBlog() {
    busy.value = { ...busy.value, blog: true }
    try {
      const blog = await api.linkedRefreshBlog()
      if (state.value) state.value = { ...state.value, blog }
    } finally {
      busy.value = { ...busy.value, blog: false }
    }
  }

  async function refreshMedicine() {
    busy.value = { ...busy.value, medicine: true }
    try {
      const medicine = await api.linkedRefreshMedicine()
      state.value = { ...(state.value ?? { paths: (await api.linkedState()).paths, blog: [], medicine: null }), medicine }
    } finally {
      busy.value = { ...busy.value, medicine: false }
    }
  }

  const snapOf = (kind: string) =>
    computed(() => state.value?.blog.find((s) => s.kind === kind) ?? null)

  const payloadOf = <T,>(kind: string) => computed(() => parse<T>(snapOf(kind).value ?? undefined))

  const medicine = computed(() => parse<MedicineTodayPayload>(state.value?.medicine ?? undefined))
  const medicineSnap = computed(() => state.value?.medicine ?? null)
  const blogSnapshots = computed(() => state.value?.blog ?? [])

  /** 有内容可显示的类别：空清单（music: []）与缺失都不算，部件该折叠 */
  const liveBlog = computed(() =>
    blogSnapshots.value.filter((s) => s.status === 'ok' && (parse<BlogPayload<unknown>>(s)?.count ?? 0) > 0),
  )

  return {
    state,
    busy,
    load,
    refreshBlog,
    refreshMedicine,
    payloadOf,
    medicine,
    medicineSnap,
    blogSnapshots,
    liveBlog,
  }
}
