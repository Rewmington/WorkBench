# Anvil 实施进度（截至 2026-10-29）

计划文件：`C:\Users\hp\.qoder-cn\plans\quiet-storm-eel.md`（M0→M7，已批准）
需求契约：`docs/PLAN.md`（冻结；本文档只记进度，不改契约）

## 里程碑状态

| | 状态 | 已拿到的证据 |
|---|---|---|
| M0 骨架 | ✅ | 无边框窗/托盘/单实例/真 Win32 右键菜单/§8.6 六条各有截图；release 无 devtools；数据目录实测在 `%LOCALAPPDATA%` |
| M1 数据层 | ✅ | rev/平票/墓碑/取消勾选保 done_at/40 次折半 rebalance/父删子随/迁移幂等 |
| M2 待办 + 面板 + 速达 | ✅ | 真机拖 exe 出图标并被 Ctrl+K 启动；palette 全键盘可达；lnk 解析单测 |
| M3 外部联动 | ✅ | 博客 6 类 ok + `notes` 诚实报 missing；药箱只读副本；三种破坏各一张截图零弹窗 |
| M4 备份与恢复 | ✅ | 77 Rust 测试 + 桩 e2e；备份页 3 断点 × 2 主题 + 三态截图；原生实测命令面板跑通 `backup.now` |
| M5 候选一工作台 | ✅ | 驾驶舱 + 进行 + 设置三视图；环的降级有组件测试锁住；三态真停在 mock 里以「角度不再变化」为证据 |
| M6 笔记 + 快捷键 + 通知 | ✅ | 回收站三态（活动/回收站/墓碑）有 12 条集成测试；净化有 11 条 vitest；快捷键冲突在**真机**上验过（Win+L 被拒 → 旧绑定回滚）；提醒 ticker 在真机落库 |
| M7 统计视图 | ✅ | 90 天种子数据上 `tools/verify.mjs` **25/25 项逐图核对一致**；五张图 3 断点 × 2 主题 + 空态 + 原生满数据各截过 |
| M8 药箱云端备用源（计划外追加） | ✅ | 只读客户端在**类型层**没有写方法；本机/云端取更新者的五条分支各有单测；四种数据源标注各一张截图；**真机对真坚果云读回来一份云端快照**（顺带挖出三条 https 路共有的闪退，见下） |

测试总数（闪退修完之后实测）：Rust **152 passed + 4 ignored** + vitest **53 passed**；`cargo clippy --all-targets -- -D warnings` 干净；`npm run typecheck` 干净；`lint-style`（68 文件 8 规则）/ `token-check` 干净。

**v1 的 M0→M7 到这就全部走完了。** M8 是走完之后按你的要求追加的一条：药箱进度也从坚果云取一份。

**仍未做、也不该由我做的**：真实 PUT 到坚果云（要你在备份页填一次应用密码后自己点）；改系统时间的原生对照（要提权，以 `reltime.ts` / `FixedNow` 单测为主证据）。

## M5 交付了什么

**M5a 数据层**：`repos/dashboard.rs`（四个 KPI + 焦点队列 + 分组队列 + 本月时间轴 + 历史跨度，一次查询出齐，避免"KPI 说 3 条、列表 4 条"）、`suggest.rs`（§8.8.4 五条规则按序取前三，纯 SQL + 比较，`RULES` 表驱动）、`dash_summary` / `dash_suggestions`。

**M5b 部件数据层**：`repos/stickies.rs`（槽位 upsert，内容没变不推 rev）、`repos/countdowns.rs`（四模式折算成物化 `ends_at`，`deadline_for` 是纯函数所以 daily/interval 的滚动可单测）、`win32/sysinfo.rs`（GetSystemTimes 两次采样作差 + GlobalMemoryStatusEx）。

**M5c–M5e 前端**：`views/Dashboard.vue`（KPI 行含实时时钟 / 队列三分组 / 环 / 时间轴 / 建议卡）、`components/dash/FocusRing.vue` + `RingStrip.vue` + `CompletionTimeline.vue`、`views/Progress.vue`（四个可折叠部件 + 顺序与显隐存 settings）、`views/Settings.vue`（三轴主题 + 降低动效）、`DetailPanel` 补时间轴点开的日明细。

**新工具 `tools/shots.mjs`**：CDP 驱动本机 headless Edge 按**真视口**截图（§8.4 的断点读视口，改容器宽度是假验收）。`--attach=<port>` 直连原生 anvil.exe 的 WebView2 驱动真窗口；`--url` + `--raw` 用来截 `design/` 基准图做并排比对。M6/M7 的断点核对继续复用它。

## M5 抓出来修掉的缺陷

1. **`useTheme` 的 watcher 只 persist 不 apply** —— 在设置里切模式/预设/强调色/降低动效，**要到重启才生效**，界面还不报任何错。这条是我自己差点漏掉的：之前所有亮色截图都是 `--then` 里直接 `documentElement.dataset.mode='light'` 拍出来的，注入属性恰好把"切了不生效"这个洞盖住了。现在 `shots.mjs` 改成点顶栏那个真开关，并等 `data-mode` 真的变了才继续。
2. **勾选框只能完成、取消不了**（M1 遗留，见 M5a 提交）：`done_at` 一个列同时当"当前状态"和"最后完成时间"用，两个语义互相抵消。迁移 `0002` 加 `completed` 拆开；统计口径统一只看 `completed = 1`。PLAN §5.1 就地记了这次修正。
3. **倒计时的"暂停"不冻时间**（迁移 `0003` 加 `remaining_ms`）：25 分钟的钟暂停 10 分钟再继续，界面会说只剩 15 分钟。
4. **`yyyymmdd` 整数相加造出非法日期**：测试里 `TODAY + 5` = `20261033`，`day_start_ms` 拿不到时刻只能回 0（1970 年），于是"5 天后的待办"在界面上变成"逾期最久的一条"。加了 `days::days_between` / `month_first_day` / `month_last_day`，并给 `day_start_ms` 上 `debug_assert` —— 静默的 1970 比 panic 坏得多。
5. **亮色主题两处不可读**：选中卡与白卡分不开（加 1.5px 描边找回重心）；环上非高亮卡融进背景（我误删了静态 box-shadow —— §8.8.1 禁的是投影*过渡*，不是投影本身）。
6. **仪表卡压在降级列表第一张卡上**：它只该浮在环上，改成 FocusRing 的 `#overlay` 插槽，降级分支里不渲染。
7. **空队列的虚线条在 268px 窄列里折成三行**：文案从"工作台 · 队列是空的"缩到"队列是空的"。

## 对 PLAN 的两处偏离（都是我看渲染结果后定的，不是顺手改的）

1. **卡片环的降级门槛从「< 3 面」提到「< 5 面」**（§8.8.1）。理由：`backface-visibility: hidden` 下同时只有 |角度| < 90° 的面可见，3 面时相邻卡差 120° → **永远只有一张卡看得见**，4 面时邻居正好 90° 侧对镜头看不见。我拿 `--url` 截了候选一的基准图对比，参考图自己是 6 面。门槛写在 `FocusRing.MIN_FACES`，`ring.spec.ts` 逐档钉住 0 / 1–4 / 5+ 三种形态。
2. **「速达钉选」实现成「常用速达」**（按 `use_count` 取前 4）。`resources` 表没有 pinned 列，§5.2 也没规划它；为一个小部件单开一次迁移不值，而"最常用的四个"是这个位置真正要回答的问题。要真钉选，M6 顺手加列即可。

## M6 交付了什么

**M6a 笔记数据层**：`model/note.rs` + `repos/notes.rs` + `repos/note_folders.rs` + `commands/notes.rs`（14 条命令）。三态分得很清楚：活动（`deleted_at IS NULL AND trashed_at IS NULL`）/ 回收站（`trashed_at IS NOT NULL`）/ 彻底删除（打墓碑，永不物理 DELETE，正文与标签都留着）。`folder_delete` 不删内容——直属笔记挪到无文件夹层、子文件夹上提一级，返回被挪走的条数。`folder_move` 拒绝挪进自己的后代（会造出环，树一渲染就死循环）。`repos/tags.rs` 泛化成 `Owner::{Todo,Note}`，笔记与待办共用一张 `tags` 表。

**M6b Markdown**：`src/lib/markdown.ts` 用 `marked` + `dompurify`，白名单**显式列出**（不给 `class`、不给 `style`、不给 data-*），`SAFE_URI` 只放 https/mailto/ftp/相对路径，`afterSanitizeAttributes` 再兜一层并强制 `rel="noopener noreferrer"`、给 `<input>` 补 `disabled`。`blocked` 计数从**净化前**的 HTML 数起——DOMPurify 会在钩子跑之前就把 `javascript:` href 抹掉，净化后再数永远是 0。

**M6c 笔记界面**：`views/Notes.vue`（文件夹树 / 列表 / 双栏编辑器）+ `composables/useNotes.ts`，600ms 防抖只写真正改了的标题与正文。

**M6d 全局快捷键 + 开机自启**：`src-tauri/src/hotkeys.rs`（注册、改绑、失败回滚）+ `commands/hotkeys.rs`（`hotkeys_list` / `set` / `reset` / `autostart_get` / `autostart_set`）+ `src/lib/combo.ts`（捕获→序列化→显示，11 条单测）+ `useHotkeys.ts` + 设置页那一格。判据只有一条：**注册失败就是被占用**（Windows 没有查询组合键归属的 API），失败时 Rust 侧先把旧绑定挂回去，前端只负责说那句话。

**M6e 提醒走系统通知中心**（§8.6 第 12 条，顺带把 0001 里一直没人读的 `remind_at` 接上）：迁移 `0004` 加 `reminded_at`；`todos::take_due_reminders` 查和盖章一次做完；`src/notify.rs` 一条 30 秒线程，一次最多 5 条；`DueEditor` 的弹层里加了「不提醒 / 到点 / 提前1小时 / 提前1天」四档，存绝对时刻但按相对截止显示，换日期时按老偏移重算。点击回跳用 `action_type_id` 把 todo id 原样带回来（插件在 Windows 上会把整份通知数据回传给 `on_action`），不拿标题反查——标题会重复，id 不会。

## M6 抓出来修掉的缺陷

8. **全局快捷键按了没反应**：`on_event` 里拿 `shortcut.to_string()` 与设置里存的串比。前者是 `shift+alt+KeyA`，后者是 `Alt+Shift+A`，**永远不相等** —— 注册成功、`registered: true`、界面全绿，只有真按一下才暴露。改成比解析后的 `HotKey`（它 `Eq` 且 id 由 mods+key 算出），顺手删掉整个 `same_combo`。这条是 M6d 原生验证里唯一一次「浏览器演不出来的 bug」。
9. **`todos_create` 返回的 stamp 是平铺的**：`made.stamp.id` 直接抛错，而 Tauri 的 rejection 值 `JSON.stringify` 出来是 `{}` —— 排查时看不到原因。改成读 `made.id`，并把错误先用 `String(e)` 打出来。
10. **`DueEditor` 弹层被窗口右边缘切掉**：按钮长在行最右端，弹层却往右开。改 `right: 0` 靠右展开，并在截图脚本里加视口内断言（`r.left>=0 && r.right<=innerWidth`）——只查子元素溢出查不出这种问题。
11. **待办行把同一个时间说两遍**：`DueEditor` 的按钮标签是 `dueLabel(...)`，旁边 `.due-txt` 又是 `dueLabel(...)`。删掉后者（逾期时仍显示 `.over-txt`，那是另一份信息），顺带删掉随之失效的 computed、import 和 CSS。
12. **`shots.mjs` 导航后等 `h2`**：`待办` 视图的 `.bar` 里没有 `h2`，于是每次截图都超时失败。改成等 `.y-view`（每个视图的根）。

## M7 交付了什么

**M7a 聚合**：`repos/stats.rs` 一次算完五张图要的数 —— 热力图（15 周 × 7 天、周一对齐、**空白天也占一格**）、每日柱（按时 / 补做 / 无截止三档）、今日环、标签横条（近 90 天前 8 + 「无标签」单独一档）、建待办时段直方图（24 根 + 门槛）。今日环直接调 `dashboard::open_today` / `done_on`，跨度用 `dashboard::history_days` —— 同一条 SQL，于是「统计页的环」与「驾驶舱的 TODAY / DONE 两张卡」不可能对不上。新增 `commands/stats.rs::stats_report`。

**M7b 视图**：`views/Stats.vue` + `components/stats/` 五个组件，全部按 §7 指定的机制手写（CSS grid 色块 / flex 高度 / 一个 `stroke-dasharray` 的 SVG 圆环 / div 宽度），没引任何图表库，颜色只用现有令牌（`color-mix` 调档，**没往 `:root` 加新令牌**，`token-check` 因此仍然过）。点热力图或某根柱 → 右栏详情面板切到那天明细（§8.8.5 的复用，不另做弹窗）。

**M7c 核对**：`tools/verify.mjs` —— 把统计页的每个数字用一条**写法不同**的查询独立复算（逐日参数化 count 代替 GROUP BY、JS `Date` 折本地日代替 chrono、`LEFT JOIN … IS NULL` 代替 `NOT EXISTS`、`todos_all` 列表代替聚合命令），25 项在 90 天种子数据上全过。CDP 客户端抽成 `tools/cdp.mjs` 给 `shots.mjs` 与 `verify.mjs` 共用。

## M7 抓出来修掉的缺陷

13. **90 天种子全是「未完成」**（M5 遗留，本轮才暴露）：`seed.rs` 的历史行 INSERT 里写了 `done_at` / `done_day` 却**没写 `completed`**。0002 把"当前状态"和"最后完成时间"拆成两列之后，这条 INSERT 就种出一堆 `done_at` 有值但 `completed = 0` 的行 —— 统计页会画出一张全空的图，而看起来像是统计写错了。修完之后种子给出 153 条完成、75 个活跃日。
14. **时段直方图把墓碑也算进去**：`hour_bars` 漏了 `deleted_at IS NULL`，与本页其余四张图口径不一致（一张算"现在库里的东西"、另一张偷偷算上删掉的）。补上并加了一条测试钉住。
15. **`verify.mjs` 差点写成自证**：第一版在拿完 CDP 结果之后就 `db.close()`，后面所有对照都跑在已关的连接上；同时"独立算法"如果照抄 `stats.rs` 的 GROUP BY，25/25 全过也什么都证明不了。两处都改了。

## 对 PLAN 的第三处偏离

**§9 的验收写「5 张手写 SVG 图」，实际只有一张是 SVG。** §7 逐图指定了机制（热力图 = CSS grid 上色块、柱状图 = flex + 高度、圆环 = `stroke-dasharray`、横条 = div 宽度），M7 照 §7 做 —— 手写、不引库、全部吃设计令牌这三条意图都满足，且 §7 才是细节规格。圆环那张确实是唯一需要 SVG 的。

## 验证边界（不含糊）

- **环的三态真停**：`降低动效` 与 `页面隐藏` 两态在浏览器 mock 里以「900ms 内 `.y-ring` 的 transform 不再变化 + `data-live=false`」为证据 —— 走的是与原生完全相同的 `live` computed 与 `cancelAnimationFrame` 路径。
- **M5 留的那个口子已经在 M7 期间补掉了**（2026-10-29，scratch + `ANVIL_SEED=90` + 3 条补焦点卡 → 焦点卡 7 张，环终于画出来了）：原生实测到「巡航中」（900ms 内 transform 从 -12.2° 走到 -24.7°）、**最小化后真停**（1.2s 内冻在 -331.967°）、**收进托盘后真停**（1.4s 内冻在 -2446.88°）、**设置里开降低动效后真停**，且重新 show + set_focus 之后又恢复巡航。截图 `shots/m5-ring-native-full-native-real.png`。
  **仍未被原生证明的只剩"另一个应用抢走焦点"这一种**：Windows 的前台锁让后台进程 `SetForegroundWindow` 直接失败（实测 `nowForeground=False`），所以 `windowFocused` 这条只能靠 `plugin:window|hide`/`minimize` 间接覆盖，或者你手动点一下别的窗口。
- 原生实测到的：驾驶舱在真窗口里渲染正常（`shots/m5-dash-window-native-degraded.png`，含 §8.8.6 的"有选中时主按钮降为 card-2 底 + accent 文字"生效）、详情面板字段按 §8.8.3 出全（rev / 最后写入设备 / 创建来源）。
- **M6d 快捷键是原生实打实的**：`Alt+Shift+A` 在真机 `registered: true`；拿 `Super+L` 试改绑 → 返回「Super+L」已被其他程序占用 → 再查仍是 `Alt+Shift+A` 且 `registered: true`（回滚生效）；把窗口最小化后用 `keybd_event` 发一次真按键（`tools/sendcombo.ps1`，走系统输入队列，只有这条路能进 `RegisterHotKey`），窗口取消最小化并在第一次采样就成了前台窗口。**开机自启只读了状态（false），没替你打开** —— 它写的是 `HKCU\...\Run`，指向哪条 exe 由当前构建决定。
- **M6e 只验到"会响一次"，没验到"看得见"**：scratch 库（`LOCALAPPDATA` 指到 `src-tauri/target/scratch`，不碰你的真库）里 4 条测试待办的 `reminded_at` 全部各落一次、无重复。但**截图时你正全屏打游戏**，Windows 在全屏下把 toast 直接收进通知中心，所以"toast 真的弹出来了"没有图像证据；那张误拍到你游戏画面的截图已当场删掉（`shots/` 本来也不入库）。**点击回跳 `app://reveal` 需要人点一下通知**，这条留给你。
- 「未挂上」那一格（存了绑定但注册失败）在浏览器里是靠 `?mock=1&hotkey=Ctrl%2BAlt%2BC` 造出来的 —— 界面上没有任何操作能进入这个状态（改绑失败会回滚），它的真实来源是换机或恢复快照后撞上别人，所以从种子数据进来。这是**给夹具**，不是注入 DOM 状态：应用仍走完整的 `hotkeysList → registered:false → 界面` 链路。
- **M7 的数是全对上了，图好不好看仍要你一眼**：`verify.mjs` 证明的是"屏幕上的每个数字都能被另一条独立查询复算出来"，证明不了"这五张图放在一起是你想看的"。种子数据下的原生截图在 `shots/m7-seed90-native-real.png`（侧栏底部那行路径显示的是 `target\scratch`，正好自证 §2 原则 4 与"没碰你真库"）。
- **图表本身没写前端单测**，这是刻意的：五个组件里唯一算得出的纯逻辑是色阶分桶与日期折算，而日期折算的真口径在 Rust（`days` 模块有 12 条单测），跨月/跨年的形状已由 `heatmap_is_a_monday_aligned_grid...` 与 `bars_split...by_calendar_day_not_by_millisecond` 钉住。给 CSS 布局写测试只会锁死实现而验不出问题。

## 复现验证的命令

```bash
cd /d/ADevelop/Repositories/Mine/workbench
npm run typecheck && npm run test
node tools/token-check.mjs && node tools/lint-style.mjs
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
cd .. && node tools/m4-check.mjs
node tools/shots.mjs 今日驾驶舱 --as=dash --tag=m5      # 3 断点 × 2 主题
node tools/shots.mjs 今日驾驶舱 --as=dash-empty --tag=m5 \
  --url="http://localhost:1420/?mock=1&empty=1"          # 空数据态
```

原生 + CDP 驱动真窗口：

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222 --remote-allow-origins=*" npm run tauri:dev
node tools/shots.mjs 今日驾驶舱 --as=x --tag=m5 --attach=9222 --then='<async js>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture.ps1 -Out shots/x.png
```

M6 的快捷键三态 + 原生按键（`sendcombo.ps1` 走 `keybd_event`，是唯一能进 `RegisterHotKey` 的路径）：

```bash
node tools/shots.mjs 设置 --as=hotkey-clash --tag=m6 --widths=1400,760 --modes=dark,light \
  --then='(async()=>{const b=document.querySelector(".combo");b.click();await new Promise(r=>setTimeout(r,150));document.body.dispatchEvent(new KeyboardEvent("keydown",{code:"KeyC",ctrlKey:true,altKey:true,bubbles:true}));await new Promise(r=>setTimeout(r,500));return "chip="+b.textContent.trim();})()'
node tools/shots.mjs 设置 --as=hotkey-idle --tag=m6 --url="http://localhost:1420/?mock=1&hotkey=Ctrl%2BAlt%2BC"
# ↑ 「未挂上」只能从种子数据进（改绑失败会回滚），%2B 是 + 的转义

# 换绑 → 真机冲突 → 回滚，以及按一下快捷键真能唤回窗口
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223 --remote-allow-origins=*" npm run tauri:dev
node tools/shots.mjs 设置 --attach=9223 --tag=m6d --then='<invoke hotkeys_set Super+L 的 async js>'
node tools/shots.mjs 设置 --attach=9223 --tag=m6d --then='(async()=>{const i=window.__TAURI_INTERNALS__.invoke;await i("plugin:window|minimize",{label:"main"});return "min="+await i("plugin:window|is_minimized",{label:"main"});})()'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/sendcombo.ps1 -Keys 'ALT,SHIFT,A'
```

M6e 的提醒**别在真库上验**：起一个 scratch 实例（先停掉原实例，单实例锁按 identifier 建）

```bash
LOCALAPPDATA='D:\ADevelop\Repositories\Mine\workbench\src-tauri\target\scratch' \
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223 --remote-allow-origins=*" npm run tauri:dev
node tools/dbq.mjs "SELECT count(*) c FROM todos WHERE title LIKE 'M6e %' AND reminded_at IS NOT NULL" \
  'D:\ADevelop\Repositories\Mine\workbench\src-tauri\target\scratch\dev.rewmington.anvil\app.db'
```

```bash
LOCALAPPDATA='D:\ADevelop\Repositories\Mine\workbench\src-tauri\target\scratch' \
ANVIL_SEED=90 \
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9224 --remote-allow-origins=*" \
  npm run tauri:dev
# 起来之后（另开一个 shell）：
node tools/verify.mjs --attach=9224 \
  --db='D:\ADevelop\Repositories\Mine\workbench\src-tauri\target\scratch\dev.rewmington.anvil\app.db'
node tools/shots.mjs 统计 --attach=9224 --tag=m7 --as=seed90
rm -rf src-tauri/target/scratch   # 用完就删，别让它冒充真库
```
`--seed` 走 `ANVIL_SEED` 环境变量比走 argv 稳（`tauri dev` 会把它自己的参数插在 `cargo run` 那串前面）。

环的三态在原生上重跑一遍（焦点卡要 ≥ 5 才画环，种子只给 4 张，所以先补三条）：

```bash
node tools/shots.mjs 今日驾驶舱 --attach=9225 --tag=m5 --as=arm --then='(async()=>{const i=window.__TAURI_INTERNALS__.invoke;const t=Date.now();for(let k=0;k<3;k++)await i("todos_create",{draft:{title:"补焦点卡 "+(k+1),dueAt:t+k*60000,zone:"today"}});return "ok";})()'
# 换视图再换回来，逼 Dashboard 重新 mount（attach 模式不重新导航，内存里还是旧 summary）
node tools/shots.mjs 今日驾驶舱 --attach=9225 --tag=m5 --as=cruise --then='(async()=>{const r=document.querySelector(".y-ring");const a=r.style.transform;await new Promise(x=>setTimeout(x,900));return "cruising="+(a!==r.style.transform);})()'
# 三态：minimize / hide / 设置里勾降低动效，各自读两次 transform 是否相等
```

## 本轮新踩到 / 新定下的事实（别重复调查）

- **`--remote-allow-origins=*` 不能省**：只给 `--remote-debugging-port` 时非浏览器客户端的 WS 握手会**静默挂住**，不报错不返回。已给 `shots.mjs` 的连接加 6s 超时。
- **`capture.ps1` 报 `window rect came back empty` = 窗口被最小化了**（位置 `-32000,-32000`、尺寸 215×26），不是渲染坏了。后台启动的 dev 实例容易被系统最小化；用 IPC 还原：`__TAURI_INTERNALS__.invoke('plugin:window|unminimize'|'show'|'set_focus'|'maximize', {label:'main'})`。
- **jsdom 没有 `matchMedia`**，而 `useTheme` 在模块加载期就订阅 `prefers-color-scheme` → 任何 import 到它的组件测试都在 import 阶段炸。已加 `src/test/setup.ts` 桩 + `vite.config.ts` 的 `setupFiles`。
- **`todos::create` 的 zone 兜底是 `inbox`**：定了截止但没选分区的条目，会同时被算进 TODAY 和 INBOX。两个 KPI 各答各的问题，不互斥（`kpi_counts_use_local_day_boundaries` 把这条钉住了）。
- **孤儿 vite 会连着咬住两件事**：杀掉 `npm run tauri:dev` 的后台任务不等于杀掉它的子进程 —— `anvil.exe` 与 `vite` 都可能活着，前者占着单实例锁与 scratch 库文件（`rm` 报 `Device or resource busy`），后者占着 1420 让下一次 `tauri:dev` 的 `beforeDevCommand` 直接失败。收尾顺序：`Stop-Process -Name anvil` → 查 `netstat | grep :1420` → 再删目录。
- **`todos_create` 返回的 `Stamp` 是平铺的**（`#[serde(flatten)]`）：读 `made.id` 不是 `made.stamp.id`；写错时 Tauri 的 rejection 值 `JSON.stringify` 出来是 `{}`，所以排查第一步是 `String(e)` 而不是打印对象。
- **统计口径必须处处同一条 `deleted_at IS NULL`**：五张图里漏一张，就会出现"标签条说 153 项、时段直方图的总数比它大"这种自己跟自己打架。`verify.mjs` 的 25 项里有几项专门在数这件事。
- **验原生环巡航要先让焦点卡 ≥ 5**：`todos::focus` 只取「未完成 · 无父项 · 逾期或在 today 区」前 8 条，`ANVIL_SEED=90` 的种子只给 4 条（`live` 那 6 条里有 2 条没设截止也不在 today 区），所以环不画。补三条 `dueAt=now` 的待办就到 7 张。**`--attach` 模式不重新导航，所以建完数据要切一次视图再切回来**才会重新 mount 拿到新 summary。
- **`data-live` 挂在 `.y-ringbox` 上而不是 `.y-ring` 上**，而且只在 `live` 变化或 mount 时写 —— 首次 mount 前属性可能压根不存在，探针读它会拿到 `undefined`。要判巡航与否，直接读 `.y-ring` 的 `style.transform` 两次是否相等最可靠。
- **后台进程抢不到前台**：`SetForegroundWindow` 从非前台进程调用会被 Windows 的前台锁挡掉（返回成功但焦点没变，实测 `nowForeground=False`），所以"别的窗口抢焦点"这一态没法自动化。`plugin:window|hide`（收进托盘）可以，而且它本来就是 §8.8.1 列的三态之一。
- 集成测试夹具在 `src-tauri/tests/common/mod.rs`（`MS = 1_793_170_800_000` = 2026-10-28T15:00+08:00）；`tests/dashboard.rs` 与 `tests/widgets.rs` 用同一个 MS 但各自建库，互不依赖。
- 上一轮的事实仍有效：ureq 要 `allow_non_standard_methods`、roxmltree 要 `descendants()`、`Nonce::from`、KDF 参数要夹上限、WebDAV 桩鉴权用 base64 不是 latin1。
- **组合键的文法**（`global_hotkey::parse_hotkey`）：按 `+` 切、修饰键必须在前、**只能有一个主键且在最后**，所以 `+` 永远当不了主键（想绑加号只能绑物理 `=` 键，即 `Ctrl+Shift+Equal`）。token 大小写不敏感，`KeyA`/`A`、`Digit3`/`3`、`Escape`/`Esc` 都认 —— `src/lib/combo.ts` 因此发裸字母/数字 + W3C `code` 名，`code` 与 `Code` 枚举同名不是巧合。
- **通知插件在 Windows 上的三件事**：① 没有监听者时压根不追踪点击，所以 setup 里必须先 `notification().on_action(...)` 再发；② `target\debug` / `target\release` 下**故意不设 AUMID**（未安装的应用不该声明 AppUserModelID），③ 点击只在 toast 还在屏幕上时上报，进了通知中心就没有；全屏应用在前台时 toast 被系统直接收走。
- **要一个干净的实例做有写入的验证**：`state.rs` 直接读 `LOCALAPPDATA`，所以 `LOCALAPPDATA=<repo>\src-tauri\target\scratch npm run tauri:dev` 就能起一个用独立库的 anvil，不碰真库。单实例锁按 identifier 建，**必须先停掉原来那个实例**。
- **迁移条数别写死在测试里**：`migrate_is_idempotent_and_counts_tables` 现在断言 `user_version == MIGRATIONS.len()`；真正在验的是第二次 `migrate()` 不 panic（重复跑 `ALTER TABLE` 必然炸）。
- 上一轮的事实仍有效：ureq 要 `allow_non_standard_methods`、roxmltree 要 `descendants()`、`Nonce::from`、KDF 参数要夹上限、WebDAV 桩鉴权用 base64 不是 latin1。

## M6 收尾时对上一轮清单的处理

- 第 1–4 条都已交付（笔记 / 数据层 / Markdown 净化 / 快捷键与自启），另补了 §8.6 第 12 条的**系统通知**（上一轮没列，但 PLAN 把它定在 M6）。
- 第 3 条的依赖债清了：`marked` 与 `dompurify` 现在真在用（`src/lib/markdown.ts`），`reka-ui` 确认零 import，已从 `package.json` 卸掉。
- **第 5 条没做，往后推**：`resources.pinned` 这一列还是没加。它不属于 M6 的任何验收判据，而"常用速达"目前没被抱怨过 —— 留给真正要钉选的那天，别为对称感提前建列。

## M7 收口时对上一轮清单的处理

- 第 1 条改道：没做 `tools/fixtures/seed-90d.json`，直接用已有的 `ANVIL_SEED=90`（它本来就造得出跨 90 天、含补做与周末稀疏的形状）—— 但**它当时是坏的**（缺陷 13），所以这条从"已经有了"变成"先修好它"。
- 第 2 条按 §7 的逐图机制做，不是五张 SVG（偏离写在上面）。图的种类也按 §7 的表来，而不是上一轮我凭印象列的那五张：完成量折线 §7 明确判**不做**（日粒度噪声大），所以画的是热力图 / 每日柱 / 今日环 / 标签横条 / 时段直方图。
- 第 3–5 条都做了：`tools/verify.mjs` 25 项对照、空态与门槛、3 断点 × 2 主题 + `&empty=1` 的截图。

## M7 之后：你反馈的「倒计时和便签一起打开」

我先按"两个入口指向同一页"修了一版（侧栏点亮判定加 `focus` 区分 + 折叠状态提成受控的 `v-model:open`，点便签就只开便签）。你看完说"这不就是同一页展开不同抽屉吗，直接把侧边改成进行中吧" —— 对，那是更好的解，所以那一版**回退了**：

- 侧栏「进行」分组（倒计时 / 便签两条）合成一条 **「进行中」**，挂到第一组末尾；v1 导航从 12 项变 11 项，PLAN §8.7 就地记了这条改动与理由。
- 顺带清掉因此变成死代码的东西：`NavItem.focus` 字段、`useShell` 的 `focus` 状态与 `isCurrent()`、`WidgetCard` 的受控 `open`（回到组件内部 ref）、`Progress.vue` 里那条 `focus → 展开` 的 watch。`go('todos', id)` 的第二个参数也去掉了 —— 它一直在写 `shell.focus`，但没有任何地方读它，回跳实际靠的是 `select({kind:'todo', id})`。
- 命令面板跟着受益：原来有「转到 倒计时」「转到 便签」两条一模一样的动作，现在只剩一条。
- `useShell.spec.ts` 加了一条**结构性**断言：没有任何两个导航条目指向同一个视图。这是这次的教训，值得被测试挡住而不是靠 review 眼神。
- 页内标题原来写「进行」，与顶栏大标题重复（同一个毛病在设置页也犯过），改成「部件 · 正在跑的都能折叠与排序」。

证据：`shots/fix-nav-merged-1400-dark.png` 与 `shots/fix-nav-merged-760-dark.png` —— 侧栏只有一条「进行中」且只它亮，四个部件都展开、各自可折叠。

**顺带记一句你已决定的**：开机自启你不需要，所以那一格保持关；它默认就是关的，代码不用动。

## M8：药箱进度也从坚果云取一份（2026-10-29 追加）

你先提的是「弄两个密钥，一个访问药的坚果云、一个访问这个软件存储的坚果云」。我去查了前提再动手：**坚果云的应用密码是账号级的**，第三方应用管理里建的每一条都能读写整个网盘，所以两把密码**不是权限边界**。把这一点当面说完之后你选了另一条读法 —— 让工作台也从云端读药箱进度，于是有了 M8。

**M8a 只读客户端**（`linking/dav_readonly.rs`）：与 `backup::webdav::Remote` 是两个类型，这个是故意做窄的 —— 只有 PROPFIND 与 GET，**没有 PUT / MKCOL / DELETE 方法**。「绝不写药箱目录」这条边界因此落在编译期，不靠运行时判断也不靠注释。另有反向钉子：地址里含 `/workbench-backup/` 直接拒，读一次最多 8 份文件、单份上限 8 MiB（是健康数据，别对着陌生目录无界地拉）。

**M8b 第二把凭据与取更新者**：凭据管理器多一条 `Anvil/medicine-webdav`（与 `Anvil/webdav` 互不干扰），邮箱进 `settings`（它不是秘密，且要解释那条凭据属于谁），应用密码只进凭据管理器。备份导出时 `linked.medicine.email` 一并删掉，`linked.medicine.davDir` 留着 —— 它机器无关，换机还原后仍然有效，而凭据不在就是"没配"，不会出现半条配置。判定只有一句：`cloud_is_fresher`，**平票算本机**（那是用户眼前的 app 刚写的）。

**M8c 界面**：顶部一行写明这次用的是哪份、对方发布于多久以前（`云端 · phone 发布于 3 分钟前 · 读取于 2 分钟前` / `本机 · 云端那份是 phone 3 天前发布` / `本机 · 云端这次没读上：…`），底部那句跟着换指向的绝对路径。备用源那块表单在**没有数据时也照常可达**，因为"这台机器压根没装药箱"正是它存在的理由之一。

### M8 抓出来修掉的缺陷

16. **云端读失败时那一行什么都不写**：`CloudRead::Failed` 与 `Skipped` 在本机成功这条分支上塌成同一个空 detail，于是"配了云端但 401"看起来和"压根没配"一模一样。现在失败原因写进那一行，`a_failed_cloud_is_not_silent_when_local_works` 钉住它。（另一处不算缺陷、算漏写：底部那句原来无条件说「来自 medicine_tracker.db 的副本」，用云端那份时是假话 —— 现在它跟着 `detail` 换指向的路径。）

### 验证边界（M8 这一条要不含糊）

- 判定逻辑、解析、只读客户端的钉子、临时副本删干净 —— 17 条 Rust 测试，全在合成数据与自建 fixture 上跑，**没碰你机器上那个真的 `medicine_tracker.db`，也没碰凭据**。
- 四种数据源标注（未配 / 云端赢 / 本机赢但读了云端 / 云端读失败）各一张截图，是**在浏览器里真填表单真点按钮**截出来的，不是注入状态。

## M8 之后：你一点「读取云端」就闪退（2026-10-29 夜）

**根因不在 M8 的代码里，在依赖的配置上。** `ureq` 3.5 的默认 TLS provider 是 `Rustls`，而 Cargo.toml 里是按 `default-features = false, features = ["native-tls"]` 引的 —— 两边对不上时 ureq **不返回错误，而是在任何 I/O 发生之前直接 `panic!`**：

```
thread 'tokio-rt-worker' panicked at ureq-3.5.0/src/unversioned/transport/mod.rs:517:
uri scheme is https, provider is Rustls but feature is not enabled: rustls
```

而 release 配置里写着 `panic = "abort"`（Tauri 模板默认），于是一 panic 整个进程就没了 —— 没有对话框、没有日志，就是你看到的闪退。Windows 事件日志里的签名对得上：`0xc0000409` + 子码 `7` = `FAST_FAIL_FATAL_APP_EXIT`，正是 Rust abort 的落点。

**波及面比看到的那一次大**：三条 https 路全都中招 —— 备份（测试连接 / 立即备份 / 列远端 / 还原）、博客的线上比对、药箱的云端读取。事件日志里 20:15 还有一次同样的崩溃，那次是**不含任何云端代码的旧包**，说明你点的是备份或线上比对。也就是说 M4 交付的"备份"当时**一次真实 https 都没跑过**。

**为什么测试没抓到**：`tools/webdav-stub.mjs` 是 `http://127.0.0.1:<port>`，压根不需要 TLS provider，所以 panic 那条 `if` 永远进不去。这是"桩测过的路径与真机路径不同"的教科书案例 —— 桩证明了报文形状与目录边界，证明不了握手那一层。

**三处改动**：

1. 新增 `src/net.rs`：`agent(global, connect, user_agent)` 一处配好 `TlsProvider::NativeTls` + PROPFIND 门票，三条路都换成它。以前是三份各写各的 config，正是这种"三处都要记得加同一行"最容易漏。
2. `[profile.release]` 去掉 `panic = "abort"`。桌面应用里一条命令 panic 不该让窗口消失：unwind 时 `spawn_blocking` 的 panic 会被 `JoinHandle` 接住，命令层折成一句人话显示在界面上。**这条是被 debug 版反证过的** —— 同一个 panic 在 debug（unwind）下应用照常活着，只是那行显示读取失败。
3. `net.rs` 里一条回归测试：对 `https://127.0.0.1:1/` 发一次请求，断言拿到的是 `Err` 而不是 panic。provider 判错发生在任何 I/O 之前，所以"连不上"就足以证明这条链走对了 —— 不碰任何外部服务、不碰任何凭据。

**真机复验过了**：debug 版带着 `--remote-debugging-port` 起来，用 CDP 点掉你那个「重新读取」按钮，日志里再无 `panicked at`，界面上出来的是 `云端 · hne2f4b35m 发布于 4 小时前 · 读取于 刚刚` —— 也就是说这确实是**从你坚果云上读回来的那一份**，而且它比本机那份新（本机赢的时候会写成 `本机 · 云端那份是 …`）。说明白：这次是我用你存进凭据管理器的那条应用密码发的请求，只有 PROPFIND 与 GET 两种动词（客户端压根没有写的方法），没往那个目录写任何东西。

**顺带修掉一个当场暴露的小毛病**：那一度显示成「读取于 即将」。原因是 Rust 写 `fetched_at` 的时钟比前端 1 秒一次的 tick 快几毫秒，`relTime` 遇到负的差值就走进了"未来"分支。`relTime` 的 12 个调用点全是「完成于 / 上次成功于 / 读取于」这种**只可能在过去**的语义，所以把它内部的差值夹住、删掉那条永远不该走到的未来分支；未来的读法本来就归 `dueLabel` / `dayWordLabel`。

## 你截图指出的两处：打开是 Edge 错误页 + 图标四角是白的

### 「localhost 拒绝连接」是我自己造成的，而且是个会反复踩的坑

修完 panic 之后我为了跑那条回归测试执行了一次 `cargo test --release`。它把
`target\release\anvil.exe` **重新编译了一遍**，而这次编译没有 tauri CLI 的环境 ——
tauri 的 `build.rs` 里写着 `let dev = !custom_protocol`（`custom-protocol` 是靠命令行
feature 传的），所以产物变成 `cfg(dev)` 版本，`cfg(dev)` 下窗口 URL 取 `devUrl`
（`tauri-2.12.5/src/manager/mod.rs:346`），也就是 `http://localhost:1420`。
vite 那时候已经被我关掉 → ERR_CONNECTION_REFUSED。

**规则：`npm run tauri:build` 之后不许再跑任何 `cargo build|test --release`**，
否则同一个路径上会留下一个"看起来是正式包、其实连 devUrl"的 exe。要跑 release 测试就跑完再重新 build。

验证方式（不看你的数据）：在 1420 上放一个只记连接的探针，再启动新包 —— 九个采样秒内**没有任何连接**，
说明它走的是内嵌资源；同时窗口句柄非零，而窗口只有在前端 `App.vue` 跑到
`await getCurrentWindow().show()` 之后才会可见，所以"渲染出来了"这两条一起被证明了。

### 图标四角是白的：源图就是白底

`design/refs/anvil-app-icon.png` 是"深色圆角块画在白底上"，四角 A=255 纯白 ——
`tauri icon` 只做缩放与打包，忠实地把白角带到 ico 和所有尺寸里，于是任务栏与托盘上是四个白点。

新增 `tools/mkicon.mjs`（本项目不引图像依赖，PNG 编解码是手写的）：从四条边同时漫填找**与外圈连通**的近白像素
→ 抠成 A=0；再对紧邻背景的那一圈抗锯齿像素按覆盖度**反预乘**，否则深色块外沿会留一圈灰白光晕。
实测抠掉 14283 个背景像素、反预乘 377 个边缘像素，改后所有尺寸四角 A=0（24px 托盘图因面积平均剩 A=5~7，等于看不见）。
`tools/mkicon.mjs --write` 出透明版源图与 `tray.png`，其余尺寸交给 `npx tauri icon design/refs/anvil-app-icon-transparent.png`。

**第一次以为修好了，其实没有**：`.ico` 换了，重新 `npm run tauri:build` 出来的 exe 里嵌的**还是旧图标**。
从 exe 抽出来量一下就露馅了（`[System.Drawing.Icon]::ExtractAssociatedIcon(exe)` → 四角 A=255 纯白）。
原因是 tauri-build 把图标编进 Windows 资源、tauri-macros 把 128x128.png 当默认窗口图标嵌进去，
**两边都没对 `icons/` 声明 `rerun-if-changed`** —— 图标变了而 build script 是新鲜的，cargo 就不重跑它。
修法是在 `src-tauri/build.rs` 里自己声明 `cargo:rerun-if-changed=icons`；改完重编，抽出来量就是 A=0。

**教训：图标这类"编译期嵌进产物"的资源，验的时候必须验产物，不能只验源文件。**
我第一轮只量了 `icons/*.png` 与 `icon.ico` 的四角，那证明的是生成器没问题，证明不了 exe 里是新的。

**装过的话要重装一次新包**；如果重装后任务栏还是旧样子，那是 Windows 的图标缓存，
`ie4uinit.exe -show` 或注销一次就刷过来。

## v1 的 M0→M7 走完之后

**代码侧没有待办了。** 剩下的都是只有你能做或只有真环境能答的：

1. **点一条提醒通知**，看窗口是否跳回那条待办（M6e 留的口子，`app://reveal` 已通，缺的是真人一点）。开发版可能压根不弹 —— 插件在 `target\debug` 下故意不设 AUMID，要装过 `npm run tauri:build` 的正式版才准。
2. ~~开机自启~~ **已定：不需要**（2026-10-29 你说了），那一格保持关，代码不用动。
3. **真实坚果云 PUT** 一次（备份页填应用密码 → 手动备份），确认远端 `/workbench-backup/` 里只有 `anvil-*` 文件、没有 `device-*.json`。我没替你碰凭据。
4. ~~M8 的那一次真 GET~~ **已验（2026-10-29 夜，修完 ureq 的 TLS provider 之后）**：`云端 · hne2f4b35m 发布于 4 小时前` 是从你坚果云真读回来的。剩下的是**你在正式版界面上自己点一次**，确认看到的也是这一行。
5. **用起来一周再回头改**：统计页的窗口（15 周 / 30 天 / 90 天）是我替你定的，你觉得哪个不对就说，改的只是 `repos/stats.rs` 顶部四个常量。
6. 已知没做的、也都记在 PLAN：周期待办、多 PC 同步、手机伴侣（v2）、`resources.pinned`（M5 偏离 2，仍不做）。

## 补上外部源路径的设置入口（2026-10-30）

你问"发给别人他能不能用"，查下来的答案分两半。**安全上没问题**：`linking/` 只有 PROPFIND 与 GET 两种动词，对网站压根没有写能力；能写的那个客户端被钉在 `/workbench-backup/`；应用密码只在 Windows 凭据管理器里，不在 exe 也不在库里。**但别人装上确实配不起来**：博客仓库路径 `linked_set_blog_dir` 命令 M3 就有，界面一直没接，换机的人只能看到「本地仓库目录不可用」且无从修正；药箱库路径连命令都没有。

新增 `components/ui/PathField.vue`，三处共用（博客仓库 / 药箱库 / 药箱的坚果云目录），同一套「保存 / 恢复默认 / 后端原话」措辞。顺带清掉三个真实缺陷：

17. **「恢复默认」按钮以前不出现**：Medicine.vue 里手抄了一份坚果云默认地址（`DEFAULT_DAV`），改 Rust 的常量不会通知它。现在默认值由 `SourcePaths` 一起送来（`blogRepoDefault` / `medicineDbDefault` / `medicineDavDefault`），只有一份。
18. **`normalize_dir` 名不副实**：它说"变成绝对路径"但只是 trim + 去引号，填 `repo\blog` 会存成相对路径，之后 `is_dir()` 拿它对着**进程 CWD** 判断 —— 于是"路径看着没错，就是永远说目录不可用"。现在相对路径按 CWD 展开，配一条单测。
19. **点一次保存发两次请求**：`PathField` 原来把按钮包在 `<label>` 里，label 的激活行为会二次派发点击。测试里表现为"计数等于 2"，是真 bug 不是测试写错。改成 `<div>` 容器。

另外把那条博客降级测试从「不该有 `.blk`」收窄成「不该有清单内容（`.cards` / `.rr`）」—— 仓库不可用时恰恰**需要**路径编辑块出现，原断言把新功能误判成了 bug。

## M9 课表：数据层（2026-10-30）

你给了真文件（`学生个人课表_202412004577.xls` → 又另存成 CSV）与教务网址 `jw.educationgroup.cn/ytkjxy_jsxsd/`。探完的结论：

- **那个 .xls 是真 BIFF8 二进制**（`D0CF11E0` 打头），不是伪装的 HTML —— 想直接读它要么装解析库要么手写解码。
- **CSV 完全够用，所以两样都没装**。矩阵结构在 CSV 里活着：列 = 星期一~星期日，行 = `第N大节⏎HH:MM-HH:MM`，格子里是**五段一组**的 `课程名 / 教师 / 周次 / 教室 / 节次`，而且**一个格子可以装两组**（同一门课的不同周次段）。
- **你那份不是 UTF-8**：Excel 选「CSV(逗号分隔)」时中文 Windows 写的是 GBK，没有 BOM。导入器只接 UTF-8，判出来不是就明说"请在 Excel 里选 CSV UTF-8(逗号分隔)"—— 加一个 GBK 解码依赖不值当，而且这条限制看得见比隐形强。
- 真文件实测（临时转成 UTF-8 后跑 `#[ignore]` 的那条钩子）：**学期 2026-2027-1 · 27 个上课时段 · 12 门课**，星期全在 1–7、开始都早于结束、每段都带教室。临时副本已删，**仓库里不留任何人的课表**（测试夹具是假的课程名与教师名）。

`classes` 一行 = 一个「星期 × 节次 × 周次集合」，不是一门课 —— 因为同一门课在不同周次段可以出现在不同教室。"这周有没有它"由 `weeks_include("1-10,12-16", n)` 判，`11` 这种空出来的周次必须算没有。

两处刻意的设计：
1. **格子字段数不是 5 的倍数就拒绝导入**，并指名是哪个格子（`星期1 的格子里有 N 行…`）。错位着导入会把教师当教室存进去，那种脏数据比导不进去糟得多。
2. **导入报告里带 `needsWeek1`**：CSV 只有"学年学期"，没有"第 1 周是哪天"。没有它就算不出"今天第几周"，硬猜会猜错一整个学期 —— 所以界面要拿这个去提示你先定开学日，而不是默默显示一个错的。

## M9b 课表命令面（2026-10-30）

四条命令：`timetable_state`（学期 / 开学日 / 当前周 / 总周数 / 几门几段）、`timetable_set_week1`、`timetable_week`、`timetable_import`（原生对话框 → 读字节 → 导入）。

三处值得记的判断：

- **对话框开着的时候不能持有数据库锁**。`timetable_import` 先把文件读完、拿到文本，才 `state.lock()` —— 备份 tick 与通知 ticker 都在等这把锁（与速达的 `resource_pick` 同一模式）。
- **`next_free_gap` 只有一个**。课表页与待办「今天」档顶部那行空档必须调同一个函数（§8.8 第 8 条），两处各写一遍迟早给出两个答案。它同时处理三件事：跳过已经上完的课、把重叠的课用 `cursor.max(end)` 合成一段（调课很常见）、不足 30 分钟不算空档（门槛值界面上要写出来）。
- **`week_of` 返回 `Option`**。开学日没设、还没开学、学期已经结束，三种情况都返回 `None`，界面要求填开学日 —— 回 0 或者硬猜都会把一整个学期显示成另一周。

新增的断言：导入课表后 `todos` 仍是 0 行（§6.7 边界 2 被测试钉住）；`classes` 不在备份剥离名单里（envelope 测试里塞了一行课，还原后必须在 —— 哪天有人把它跟 `linked_snapshots` 一起剥掉就会红）。

顺带一个自己踩到的坑：测试里 `FixedNow(1_700_000_000_000)` 其实是 **2023-11-14**，我按"2026-10-30"写预期，连错两条。周次这种算的是本地日的量，要从日整数反推（`days::day_start_ms(20261030)`），别记裸常量。

Rust：165 passed + 5 ignored；clippy 干净。

## M9c 课表页 + 待办「今天」档改形状（2026-10-30）

侧栏第 12 项「课表」（`LayoutGrid` 图标），右栏新增一种选中类型 `class`。版式就是你在对比稿里挑的 ④，只是课不再和待办挤在同一条时间轴上：

- **节次那一列钉住**（`position: sticky`），横着滚也不跑；**今天整列染色**；没排课的格子写「空闲」而不是留白 —— 留白看着像渲染坏了。
- **「此刻」与「选中」在同一格上必须分得清**：此刻是描边，选中是左色条 + `--accent-soft` 底。这两种状态会同时落在同一格上（正在上的那节课被你点了），所以记号不能撞。这里我先踩了一坑：把 `:disabled` 当选中用，于是首屏**每张卡都像被选中**，CDP 数出来 `disabled=14 sel=1` 才发现。
- 空态两条：没导过 → 折叠成一行、入口就在旁边；导过但没设开学日 → 琥珀条要求填，页面不猜周次。

### 待办「今天」档

三条带：**按截止时间**（未做，按 dueAt 升序）/ **随时**（未做，没设截止）/ **已完成**。顶部一行读课表：`今天 1 节课 · 最近空档 12:15–16:00`。其余档仍是**一条一行**，一行代码都没分叉 —— 不分带时它就是"一条没有标题的带"。

三条带的第三条是**拍完第一张截图才加的**：按 dueAt 排之后，90 天历史里那些三个月前做完的旧账（dueAt 更早）全部顶到最前面，今天要做的被挤出屏幕。所以完成的单独一条、且排最后。同一次截图还暴露了两个真缺陷，都改了：

1. **`.over` 这个 class 名被两件事共用** —— TodoRow 用它表示"这条逾期了"，`Todos.vue` 的 `:deep(.over)` 用它画拖动落点。结果每条逾期行下面都多了一道 accent 绿线，看着像故意的分隔。拖动那个改名 `drag-over`，并补了一条断言。
2. **勾掉的待办还挂着"逾期 89 天"** —— `overdue` 没看 `completed`，于是"完成于 08/03"和"逾期 89 天"并排显示，两句互相矛盾的话。

空档那行的文案是**由 `now` 现算的**，`loadGap` 只缓存"今天这几节课"。写死在 load 那一刻的话，待办页停在 12:09、课表页每秒跳一格，两个页面就会给出不同的值（§9 要求同一个值）。`src/views/today-shape.spec.ts` 把时钟冻在周五 10:00，两边对同一句 `最近空档 10:00–16:00`。

那一行最后**从列表卡片里挪到了卡片外面**：真机上一打开，「今天」档是空的（你上午让我把测试数据清干净了），而 `.gap` 长在 `.list` 里，`v-if="visible.length"` 一为假就整块不渲染 —— 空清单时这一屏什么都不说。挪出来之后，一条待办都没有也照样回答"今天几点有空"（补了一条用例钉这个）。

### 真后端实测（发布版）

`npm run tauri:build` 之后带 `--remote-debugging-port` 起了 `target/release/anvil.exe`，`tools/probe-m9c.mjs` 只读**结构计数**（不读任何标题、课程名、教室），结果是：

- `location.href` 是 `http://tauri.localhost/` —— 打包资源，不再指向 devUrl。dev server 是我先杀掉才起的，所以这条是真证据。
- 侧栏 12 项、有「课表」；课表页 8 列 × 6 节次行、`needWeek1: false`、图例「12 门课 · 本周 13 个时段 · 最近空档 12:34–14:00」。
- 待办页那一行给出 **同一句** `今天 4 节课 · 最近空档 12:34–14:00`。库里 `todos` 一行都没有，bands 为空、`.gap` 仍在 —— 挪出去那次改动的效果就在这儿。
- `greenLines: 0`、`overdueOnDone: 0`：那两个 class 撞名与"勾掉还喊逾期"的缺陷在真数据上也确认没了。

顺带发现：`classes` 里已经有 27 行 / 12 门课、`timetable.week1Monday` 也设好了 —— 是**你自己**在 12:26 用刚编出来的这一版导进去的（`created_by` 是本机设备号，27 行同一毫秒）。我没有替你的真库写过一行。

### 验收口径换了一条

原写「点格子右栏出关联待办」，做的时候发现 `classes` 和 `todos` 之间没有任何字段能连起来，"关联"只能靠拿课程名去猜标题 —— 那是用字符串匹配冒充数据关系。所以右栏出的是这节课的事实 + 一句"导入进来的只读副本"，理由记在 PLAN §10。

### 验证

- 前端 71 passed（`today-shape.spec.ts` 7 条：三条带的顺序与内容、空档行与课表页同值、没设开学日不编"全天有空"、空清单时那一行还在、课不进待办、把手与绿线）+ typecheck + `lint:style` 与令牌冻结表都过。
- Rust 164 passed + 5 ignored、clippy `-D warnings` 干净。新增 `importing_a_whole_term_moves_no_completion_number`：整学期导进来，今日环 / 完成日历 / 标签总数 / 历史天数一个数都不许动。
- 截图：`shots/m9-today-{1400,1000,760}-{dark,light}.png`（今天档三条带 + 空档行）、`shots/m9-tt-ok-*`（课表常态）、`shots/m9-tt-panel-*`（点一格出右栏，含图例那句空档）、`shots/m9-tt-*`（未设开学日与窄档）。
## 你报的：回退一份备份之后，坚果云的账号和密码都不见了（2026-10-30）

诊断先于改动，取证只读聚合值（不读邮箱内容）：

- 现在这台机器的 `settings` 里 `webdav.email` **整行没了**，`linked.medicine.email` 是 `null`。
- 还原时留下的 `app.pre-restore-*.db` 里两行都还在 —— 所以是**还原这一步抹掉的**，不是别的。
- Windows 凭据管理器里 `webdav.Anvil` **仍然存在**（`cmdkey /list` 只有目标名，没读秘密）。密码从来没进过库，也就不可能被还原删掉。

根因是 M4 的一条正确规则被用错了地方：备份产物**不许携带**本机凭据与绝对路径（`export` 剥掉 `MACHINE_LOCAL_KEYS` 四项，envelope 测试钉着）。但还原是**换库**，不是**换机器** —— 整文件覆盖顺手把这四项清成了空，于是备份页与药箱页同时变成"未配置"，而那个还躺在凭据管理器里的密码因为邮箱没了而没人能解释它属于谁。备份页的保存按钮要邮箱 + 密码两个都填才让点，所以看起来"密码也没了"。

改法：同一份名单管两头。`restore_with` 在换库**之前**把这四项抄下来，换完库、写完新设备 id 之后写回刚还原好的库；`export` 的剥除名单改成引用同一个 `MACHINE_LOCAL_KEYS` 常量，以后加一项不会只记得改一头。写回失败只记日志不算还原失败 —— 库已经换好了，顶多重填一次邮箱，把成功的还原说成失败更糟。

- 单测 `the_artifact_drops_them_and_the_restore_puts_them_back`：出门那份四项全空、主题留着；写回后四项回来且值是本机还原前那个。
- e2e `put_propfind_get_and_restore_against_the_stub` 加了断言：目标机器登录的是**另一个**账号，还原之后留下的必须是它（`node tools/m4-check.mjs` 真跑过，3 passed）。
- 换机场景不变：新机器本来就没有这四项，`machine_local_rows` 返回空，就不往库里塞空值（第二条单测钉这个）。

**你这边现在要做的**：备份页填一次邮箱即可 —— 那条应用密码还在凭据管理器里，但表单要两个都填，所以顺手重填一次；药箱页的邮箱 + 密码要重填（凭据管理器里没有 `medicine-webdav.Anvil` 这条，说明它早就没存过）。另外 `linked.blog.repoDir` 与 `linked.medicine.dbPath` 你这两台都没自定义过，所以没东西可丢。
## 你报的第二个：换一页回来密码框空了，但「测试连接」还连得上（2026-10-30 傍晚）

两个现象都是真的，而且互相印证：应用密码**从来没进过库**，所以没有任何东西可以回显 —— 表单里那个 `password` 是组件本地状态，切页就没了。而「测试连接」拿的是**已存的那条**（邮箱读 settings、密码读凭据管理器），它连得上恰恰说明密码还在。

问题在界面说的话与事实相反：

- 保存按钮要邮箱 + 密码同时填才能点 —— 等于逼你为重存一个邮箱再打一遍应用密码。
- 「加解密自证」空着密码也回一句「凭据可用」—— 它其实只做本机 KDF + AES-GCM 往返，不联网、也不读凭据管理器，**空密码它也说通过**。那是一盏假绿灯。

改了什么：

- `creds::keep_or_set`：框里空着 = **沿用已存那条**，不是清空；清空是「清除」按钮那种显式动作。备份与药箱两条命令都走它。判的是纯函数，单测不碰凭据管理器（真去写 `webdav` 那条会覆盖你已经配好的密码）。
- `BackupStatus` 多一个 `passwordStored`：邮箱与密码**分开报**。还原之后正是"邮箱空着而密码还在"这种状态，只报一个 `configured` 就说不出这句真话。
- 两页的表单下面各写一行它到底存在哪（服务 `Anvil` · 账号 `webdav` / `medicine-webdav`）与为什么不回显。
- 自证的回话换成不撒谎的那句，并且要真填了密码才能点；「保存凭据」只要邮箱。

顺带一句关于"用记事本打开一堆乱码"：`app.db` 是 SQLite 二进制、`backups/*.abackup` 是 gzip 之后 AES-256-GCM 的密文，两样都不该有可读的密码 —— 密码在 Windows 凭据管理器里，由系统按当前用户加密存放。要看它：控制面板 → 凭据管理器 → Windows 凭据 → `webdav.Anvil` → 显示（需要你本人的登录身份）。
