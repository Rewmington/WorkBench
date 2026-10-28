import type { Api } from './types'
import { mockApi } from './mock'
import { realApi } from './real'

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const forcedMock =
  typeof window !== 'undefined' && new URLSearchParams(window.location.search).has('mock')

export const runningInBrowser = !inTauri
export const usingMock = !inTauri || forcedMock

/** 全项目只在这里碰 invoke()。 */
export const api: Api = usingMock ? mockApi : realApi

export type * from './types'
