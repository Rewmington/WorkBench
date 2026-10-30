# 设计稿

这里的 HTML 是**决策前后的对比稿**，不是最终实现。都能直接用浏览器打开（自包含，不依赖构建）。

| 文件 | 回答的问题 |
|---|---|
| `workbench-ui.html` | 桌面主界面「今日驾驶舱」的版式与令牌来源；冻结的色板/圆角/字号在这份里 |
| `workbench-direction-d.html` | 桌面方向的 A/B/C/D 对比，最终选了哪一版 |
| `workbench-todos.html` | 待办页的版式对比与定稿 |
| `workbench-timetable.html` | 课表页的网格与空档写法 |
| `workbench-stats.html` | 统计视图的版式探索 |
| `workbench-mobile.html` | 手机端四条路线（PWA/Flutter/局域网/坚果云）的决策表、竞品与同步文案纪律 |
| `workbench-mobile-ui.html` | 手机端四版视觉对比，最终选「桌面直系」并抄进 Flutter 的 `ThemeData` |

## 其它

| 路径 | 内容 |
|---|---|
| `refs/` | 应用图标美术源与参考图 |
| `_build/` | 把分页稿并成一份的工具（`merge.mjs`）与并版产物 |

## 约定

- 设计稿里的色值与字号是**验收依据**：桌面令牌冻结在 `src/style.css`，手机令牌冻结在 `mobile/lib/palette.dart`；改一个值要回看所有部件（`tools/token-check.mjs` 会挡）。
- 对比稿里被拒的方案不删：它们记录了「为什么不选」，比只留最终稿有用。
