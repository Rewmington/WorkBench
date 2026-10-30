/**
 * 把 `invoke` 的 reject 转成界面上那一句人话。
 *
 * 真后端 reject 出来就是字符串（`error.rs` 的 `Serialize` 走 `serialize_str`），
 * `String(e)` 正好；而 mock 抛的是 `Error`，`String(e)` 会多出一个 `Error: ` 前缀。
 * 同一个错误在两种运行环境下要显示成同一句话，所以判一下再转。
 */
export function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}
