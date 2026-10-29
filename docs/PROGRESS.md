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
| M7 统计 | ⬜ **下一步** | — |

测试总数（M6 收口时实测）：Rust **126 passed + 4 ignored** + vitest **47 passed**；`cargo clippy --all-targets -- -D warnings` 干净；`npm run typecheck` 与 `npm run build` 干净；`lint-style`（61 文件 8 规则）/ `token-check` 干净；`node tools/m4-check.mjs` 仍全绿。

**仍未做、也不该由我做的**：真实 PUT 到坚果云（要你在备份页填一次应用密码后自己点）；改系统时间的原生对照（要提权，以 `reltime.ts` / `FixedNow` 单测为主证据）。

## M5 交付了什么

**M5a 数据层**：`repos/dashboard.rs`（四个 KPI + 焦点队列 + 分组队列 + 本月时间轴 + 历史跨度，一次查询出齐，避免"KPI 说 3 条、列表 4 条"）、`suggest.rs`（§8.8.4 五条规则按序取前三，纯 SQL + 比较，`RULES` 表驱动）、`dash_summary` / `dash_suggestions`。

**M5b 部件数据层**：`repos/stickies.rs`（槽位 upsert，内容没变不推 rev）、`repos/countdowns.rs`（四模式折算成物化 `ends_at`，`deadline_for` 是纯函数所以 daily/interval 的滚动可单测）、`win32/sysinfo.rs`（GetSystemTimes 两次采样作差 + GlobalMemoryStatusEx）。

**M5c–M5e 前端**：`views/Dashboard.vue`（KPI 行含实时时钟 / 队列三分组 / 环 / 时间轴 / 建议卡）、`components/dash/FocusRing.vue` + `RingStrip.vue` + `CompletionTimeline.vue`、`views/Progress.vue`（四个可折叠部件 + 顺序与显隐存 settings）、`views/Settings.vue`（三轴主题 + 降低动效）、`DetailPanel` 补时间轴点开的日明细。

**新工具 `tools/shots.mjs`**：CDP 驱动本机 headless Edge 按**真视口**截图（§8.4 的断点读视口，改容器宽度是假验收）。`--attach=<port>` 直连原生 anvil.exe 的 WebView2 驱动真窗口；`--url` + `--raw` 用来截 `design/` 基准图做并排比对。M6/M7 的断点核对继续复用它。

## 本轮抓出来修掉的缺陷

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

## 本轮抓出来修掉的缺陷

8. **全局快捷键按了没反应**：`on_event` 里拿 `shortcut.to_string()` 与设置里存的串比。前者是 `shift+alt+KeyA`，后者是 `Alt+Shift+A`，**永远不相等** —— 注册成功、`registered: true`、界面全绿，只有真按一下才暴露。改成比解析后的 `HotKey`（它 `Eq` 且 id 由 mods+key 算出），顺手删掉整个 `same_combo`。这条是 M6d 原生验证里唯一一次「浏览器演不出来的 bug」。
9. **`todos_create` 返回的 stamp 是平铺的**：`made.stamp.id` 直接抛错，而 Tauri 的 rejection 值 `JSON.stringify` 出来是 `{}` —— 排查时看不到原因。改成读 `made.id`，并把错误先用 `String(e)` 打出来。
10. **`DueEditor` 弹层被窗口右边缘切掉**：按钮长在行最右端，弹层却往右开。改 `right: 0` 靠右展开，并在截图脚本里加视口内断言（`r.left>=0 && r.right<=innerWidth`）——只查子元素溢出查不出这种问题。
11. **待办行把同一个时间说两遍**：`DueEditor` 的按钮标签是 `dueLabel(...)`，旁边 `.due-txt` 又是 `dueLabel(...)`。删掉后者（逾期时仍显示 `.over-txt`，那是另一份信息），顺带删掉随之失效的 computed、import 和 CSS。
12. **`shots.mjs` 导航后等 `h2`**：`待办` 视图的 `.bar` 里没有 `h2`，于是每次截图都超时失败。改成等 `.y-view`（每个视图的根）。

## 验证边界（不含糊）

- **环的三态真停**：`降低动效` 与 `页面隐藏` 两态在浏览器 mock 里以「900ms 内 `.y-ring` 的 transform 不再变化 + `data-live=false`」为证据 —— 走的是与原生完全相同的 `live` computed 与 `cancelAnimationFrame` 路径。
- **原生侧只验到降级形态**：你机器上的真实库里焦点卡是 0–4 张，环不画，所以"原生巡航/失焦真停"这一条**没有原生截图为证**。要补的话：`npm run tauri:dev -- -- --seed 90` 造 90 天数据（`--seed` 只在 debug 构建生效），再用 `shots.mjs --attach=9222` 跑同一个探针。我没有擅自往你的真实库里种数据。
- 原生实测到的：驾驶舱在真窗口里渲染正常（`shots/m5-dash-window-native-degraded.png`，含 §8.8.6 的"有选中时主按钮降为 card-2 底 + accent 文字"生效）、详情面板字段按 §8.8.3 出全（rev / 最后写入设备 / 创建来源）。
- **M6d 快捷键是原生实打实的**：`Alt+Shift+A` 在真机 `registered: true`；拿 `Super+L` 试改绑 → 返回「Super+L」已被其他程序占用 → 再查仍是 `Alt+Shift+A` 且 `registered: true`（回滚生效）；把窗口最小化后用 `keybd_event` 发一次真按键（`tools/sendcombo.ps1`，走系统输入队列，只有这条路能进 `RegisterHotKey`），窗口取消最小化并在第一次采样就成了前台窗口。**开机自启只读了状态（false），没替你打开** —— 它写的是 `HKCU\...\Run`，指向哪条 exe 由当前构建决定。
- **M6e 只验到"会响一次"，没验到"看得见"**：scratch 库（`LOCALAPPDATA` 指到 `src-tauri/target/scratch`，不碰你的真库）里 4 条测试待办的 `reminded_at` 全部各落一次、无重复。但**截图时你正全屏打游戏**，Windows 在全屏下把 toast 直接收进通知中心，所以"toast 真的弹出来了"没有图像证据；那张误拍到你游戏画面的截图已当场删掉（`shots/` 本来也不入库）。**点击回跳 `app://reveal` 需要人点一下通知**，这条留给你。
- 「未挂上」那一格（存了绑定但注册失败）在浏览器里是靠 `?mock=1&hotkey=Ctrl%2BAlt%2BC` 造出来的 —— 界面上没有任何操作能进入这个状态（改绑失败会回滚），它的真实来源是换机或恢复快照后撞上别人，所以从种子数据进来。这是**给夹具**，不是注入 DOM 状态：应用仍走完整的 `hotkeysList → registered:false → 界面` 链路。

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

## 本轮新踩到 / 新定下的事实（别重复调查）

- **`--remote-allow-origins=*` 不能省**：只给 `--remote-debugging-port` 时非浏览器客户端的 WS 握手会**静默挂住**，不报错不返回。已给 `shots.mjs` 的连接加 6s 超时。
- **`capture.ps1` 报 `window rect came back empty` = 窗口被最小化了**（位置 `-32000,-32000`、尺寸 215×26），不是渲染坏了。后台启动的 dev 实例容易被系统最小化；用 IPC 还原：`__TAURI_INTERNALS__.invoke('plugin:window|unminimize'|'show'|'set_focus'|'maximize', {label:'main'})`。
- **jsdom 没有 `matchMedia`**，而 `useTheme` 在模块加载期就订阅 `prefers-color-scheme` → 任何 import 到它的组件测试都在 import 阶段炸。已加 `src/test/setup.ts` 桩 + `vite.config.ts` 的 `setupFiles`。
- **`todos::create` 的 zone 兜底是 `inbox`**：定了截止但没选分区的条目，会同时被算进 TODAY 和 INBOX。两个 KPI 各答各的问题，不互斥（`kpi_counts_use_local_day_boundaries` 把这条钉住了）。
- 上一轮的事实仍有效：ureq 要 `allow_non_standard_methods`、roxmltree 要 `descendants()`、`Nonce::from`、KDF 参数要夹上限、WebDAV 桩鉴权用 base64 不是 latin1。
- 集成测试夹具在 `src-tauri/tests/common/mod.rs`（`MS = 1_793_170_800_000` = 2026-10-28T15:00+08:00）；`tests/dashboard.rs` 与 `tests/widgets.rs` 用同一个 MS 但各自建库，互不依赖。
- **组合键的文法**（`global_hotkey::parse_hotkey`）：按 `+` 切、修饰键必须在前、**只能有一个主键且在最后**，所以 `+` 永远当不了主键（想绑加号只能绑物理 `=` 键，即 `Ctrl+Shift+Equal`）。token 大小写不敏感，`KeyA`/`A`、`Digit3`/`3`、`Escape`/`Esc` 都认 —— `src/lib/combo.ts` 因此发裸字母/数字 + W3C `code` 名，`code` 与 `Code` 枚举同名不是巧合。
- **通知插件在 Windows 上的三件事**：① 没有监听者时压根不追踪点击，所以 setup 里必须先 `notification().on_action(...)` 再发；② `target\debug` / `target\release` 下**故意不设 AUMID**（未安装的应用不该声明 AppUserModelID），③ 点击只在 toast 还在屏幕上时上报，进了通知中心就没有；全屏应用在前台时 toast 被系统直接收走。
- **要一个干净的实例做有写入的验证**：`state.rs` 直接读 `LOCALAPPDATA`，所以 `LOCALAPPDATA=<repo>\src-tauri\target\scratch npm run tauri:dev` 就能起一个用独立库的 anvil，不碰真库。单实例锁按 identifier 建，**必须先停掉原来那个实例**。
- **迁移条数别写死在测试里**：`migrate_is_idempotent_and_counts_tables` 现在断言 `user_version == MIGRATIONS.len()`；真正在验的是第二次 `migrate()` 不 panic（重复跑 `ALTER TABLE` 必然炸）。
- 上一轮的事实仍有效：ureq 要 `allow_non_standard_methods`、roxmltree 要 `descendants()`、`Nonce::from`、KDF 参数要夹上限、WebDAV 桩鉴权用 base64 不是 latin1。

## M6 收尾时对上一轮清单的处理

- 第 1–4 条都已交付（笔记 / 数据层 / Markdown 净化 / 快捷键与自启），另补了 §8.6 第 12 条的**系统通知**（上一轮没列，但 PLAN 把它定在 M6）。
- 第 3 条的依赖债清了：`marked` 与 `dompurify` 现在真在用（`src/lib/markdown.ts`），`reka-ui` 确认零 import，已从 `package.json` 卸掉。
- **第 5 条没做，往后推**：`resources.pinned` 这一列还是没加。它不属于 M6 的任何验收判据，而"常用速达"目前没被抱怨过 —— 留给真正要钉选的那天，别为对称感提前建列。

## 下一轮：M7 统计视图

1. `tools/fixtures/seed-90d.json` + 一个 debug-only 的灌入路径（`--seed 90` 已经在了，要确认它能造出**跨 90 天、含逾期与取消勾选**的形状，不然图都是平的）。
2. 5 张手写 SVG：完成量折线 / 星期×小时热力 / 分区占比 / 逾期分布 / 标签词频（§7）。全部走 `--accent` 一族，不引图表库。
3. `tools/verify.mjs`：每张图的聚合 SQL 用 `dbq.mjs` 独立跑一遍，与前端算出来的点数逐个对，差异 > 0 就报出来 —— 「图与列表不一致」是这类视图最常见的死法。
4. 数据不足时按 §8.8 出空态，不画一根贴地的线假装是趋势。
5. 复现验证的截图口径沿用 M5/M6：`node tools/shots.mjs 统计 --as=stats --tag=m7`（3 断点 × 2 主题）+ `--url="...&empty=1"` 的空态。
