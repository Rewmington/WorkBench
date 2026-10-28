import { convertFileSrc } from '@tauri-apps/api/core'

import { usingMock } from '../api'

/**
 * 把本机绝对路径转成 asset 协议 URL（CSP 里已放行 http://asset.localhost）。
 * 浏览器 mock 模式下没有这个协议，返回 null 让调用方退化成首字母占位。
 */
export function assetUrl(path: string | null | undefined): string | null {
  if (!path || usingMock) return null
  try {
    return convertFileSrc(path)
  } catch {
    return null
  }
}
