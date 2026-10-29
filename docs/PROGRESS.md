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
| M6 笔记 + 快捷键 | ⬜ **下一步** | — |
| M7 统计 | ⬜ 未开始 | — |

测试总数（本轮实测）：Rust **108 passed + 4 ignored** + vitest **25 passed**；`cargo clippy --all-targets -- -D warnings` 干净；`npm run typecheck` 干净；`lint-style`（54 文件 8 规则）/ `token-check` 干净；`node tools/m4-check.mjs` 仍全绿（M5 的两次迁移没破坏它）。

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

## 验证边界（不含糊）

- **环的三态真停**：`降低动效` 与 `页面隐藏` 两态在浏览器 mock 里以「900ms 内 `.y-ring` 的 transform 不再变化 + `data-live=false`」为证据 —— 走的是与原生完全相同的 `live` computed 与 `cancelAnimationFrame` 路径。
- **原生侧只验到降级形态**：你机器上的真实库里焦点卡是 0–4 张，环不画，所以"原生巡航/失焦真停"这一条**没有原生截图为证**。要补的话：`npm run tauri:dev -- -- --seed 90` 造 90 天数据（`--seed` 只在 debug 构建生效），再用 `shots.mjs --attach=9222` 跑同一个探针。我没有擅自往你的真实库里种数据。
- 原生实测到的：驾驶舱在真窗口里渲染正常（`shots/m5-dash-window-native-degraded.png`，含 §8.8.6 的"有选中时主按钮降为 card-2 底 + accent 文字"生效）、详情面板字段按 §8.8.3 出全（rev / 最后写入设备 / 创建来源）。

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

## 本轮新踩到 / 新定下的事实（别重复调查）

- **`--remote-allow-origins=*` 不能省**：只给 `--remote-debugging-port` 时非浏览器客户端的 WS 握手会**静默挂住**，不报错不返回。已给 `shots.mjs` 的连接加 6s 超时。
- **`capture.ps1` 报 `window rect came back empty` = 窗口被最小化了**（位置 `-32000,-32000`、尺寸 215×26），不是渲染坏了。后台启动的 dev 实例容易被系统最小化；用 IPC 还原：`__TAURI_INTERNALS__.invoke('plugin:window|unminimize'|'show'|'set_focus'|'maximize', {label:'main'})`。
- **jsdom 没有 `matchMedia`**，而 `useTheme` 在模块加载期就订阅 `prefers-color-scheme` → 任何 import 到它的组件测试都在 import 阶段炸。已加 `src/test/setup.ts` 桩 + `vite.config.ts` 的 `setupFiles`。
- **`todos::create` 的 zone 兜底是 `inbox`**：定了截止但没选分区的条目，会同时被算进 TODAY 和 INBOX。两个 KPI 各答各的问题，不互斥（`kpi_counts_use_local_day_boundaries` 把这条钉住了）。
- 上一轮的事实仍有效：ureq 要 `allow_non_standard_methods`、roxmltree 要 `descendants()`、`Nonce::from`、KDF 参数要夹上限、WebDAV 桩鉴权用 base64 不是 latin1。
- 集成测试夹具在 `src-tauri/tests/common/mod.rs`（`MS = 1_793_170_800_000` = 2026-10-28T15:00+08:00）；`tests/dashboard.rs` 与 `tests/widgets.rs` 用同一个 MS 但各自建库，互不依赖。

## 下一轮：M6 的第一批事

1. `views/Notes.vue`：Markdown 源码 + 预览双栏（左 textarea 即真相源），文件夹树 + 标签 + 回收站（`trashed_at`，恢复/彻底删除走原生 confirm）。
2. `repos/notes.rs` + `note_folders`，走 M1 的同步列与 `FixedNow` 纪律。
3. **`marked` + `dompurify` 现在还是零 import** —— M6 必须用上（博客正文与笔记都是不可信输入），vitest 要覆盖 `<script>` / `javascript:` / `onerror` 三种注入用例；M6 收口时这两个依赖要么在用要么删掉。同理 `reka-ui`（PLAN §3 已记"实际未采用"）。
4. 快捷键自定义：组合键序列化后 `global-shortcut.register`，**注册失败即冲突** → 内联提示"该快捷键已被其他程序占用"并保留原绑定；开机自启开关进设置页。
5. 顺带把「速达钉选」的真 pinned 列补上（M5 偏离 2 的收尾）。
