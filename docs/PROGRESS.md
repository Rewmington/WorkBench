# Anvil 实施进度（截至 2026-10-29）

计划文件：`C:\Users\hp\.qoder-cn\plans\quiet-storm-eel.md`（M0→M7，已批准）
需求契约：`docs/PLAN.md`（冻结；本文档只记进度，不改契约）

## 里程碑状态

| | 状态 | 已拿到的证据 |
|---|---|---|
| M0 骨架 | ✅ 已提交 | 无边框窗/托盘/单实例/真 Win32 右键菜单/§8.6 六条各有截图；release 无 devtools；数据目录实测在 `%LOCALAPPDATA%` |
| M1 数据层 | ✅ 已提交 | `cargo test` 覆盖 rev/平票/墓碑/**取消勾选保 done_at**/40 次折半后 rebalance/父删子随/迁移幂等 |
| M2 待办 + 面板 + 速达 | ✅ 已提交 | 真机拖 exe 进窗口出图标并被 Ctrl+K 启动；palette 全键盘可达；lnk 解析有单测 |
| M3 外部联动 | ✅ 已提交 | 博客 6 类 ok + `notes` 诚实报 missing；药箱只读副本读到 5 种药；断网/改名三种破坏各一张截图，零弹窗 |
| M4 备份与恢复 | ✅ 已提交 | 后端 77 Rust 测试 + `tools/m4-check.mjs` 桩端到端；前端 3 断点 × 2 主题 + 内联确认/未配凭据/未恢复失败三态截图；**原生窗口实测命令面板跑通 `backup.now`** |
| M5 候选一工作台 | ⬜ **下一步** | — |
| M6 笔记 + 快捷键 | ⬜ 未开始 | — |
| M7 统计 | ⬜ 未开始 | — |

测试总数（本轮实测）：Rust **77 passed + 4 ignored** + vitest **20 passed**；`cargo clippy --all-targets -- -D warnings` 干净；`npm run typecheck` 干净；`token-check` / `lint-style`（43 文件 8 规则）干净；`node tools/m4-check.mjs` 全绿。

**仍未做、也不该由我做的**：真实 PUT 到坚果云。要你在备份页填一次邮箱 + 应用密码后自己点「立即备份」——我不索要也不读取任何第三方凭据。

## M4 后端（上一轮完成，本轮未改）

`src-tauri/src/backup/`：`crypto.rs` 信封 `ANVILBKP|ver|kdf|m|t|p|salt[16]|nonce[12]`（AAD = nonce 之前的头部字节，Argon2id 64 MiB/3/1，密钥 `Argon2id(email:应用密码)`）；`propfind.rs` 按**本地名**取元素；`webdav.rs` MKCOL/PROPFIND/PUT/GET/DELETE + Basic，目录必须 `/workbench-backup/` 且不含 `medicine-sync`，`device-*` 在 `url()` 与 `list()` 各挡一次；`mod.rs` `export()` = `VACUUM INTO` → 删外部缓存与流水与三个含本机路径的 settings 键 → `VACUUM` → gzip9；`schedule.rs` 60 s 线程 + 指数退避封顶 30 min + 失败不弹窗只写流水。

## M4 前端本轮验收（本轮做的）

**新工具 `tools/shots.mjs`**：CDP 驱动本机 headless Edge，按**真视口**宽度截图。为什么不用现成截图或改容器宽度——§8.4 的断点与 `Backup.vue` 的 `@media` 都读视口宽度，改容器是假验收。M5/M6/M7 每次断点核对都复用它。

- `node tools/shots.mjs 备份 --as=backup` → 1400/1000/760 × 暗/亮六张
- `--then='<js>'` 演「点一下才出现」的状态（内联确认、未配凭据、未恢复的失败）
- `--attach=9222` 直连**原生 anvil.exe** 的 WebView2，只拍真实窗口、不改视口不覆盖主题

**截图清单**（`shots/`）：`m4-backup-{1400,1000,760}-{dark,light}.png`、`m4-backup-confirm-{760,1400}-dark.png`、`m4-backup-unconfigured-1400-{dark,light}.png`、`m4-backup-failing-1400-dark.png`、`m4-backup-palette-native-real.png`（CDP 内容）、`m4-backup-window-native.png`（`capture.ps1` 真窗口像素）。

**原生实测**：命令面板搜「立即备份」→ 只命中 `立即备份到坚果云` → 回车 → 备份页出现「还没配好坚果云凭据，备份先不跑」，**无弹窗**。

## 视觉验收抓出来的 8 个缺陷（都已修）

1. **mock 的 `backupStatus` 在升序数组上 `find(l => l.ok)`** → 取到的是**最旧**一次成功，于是页面说「上次成功 2 天前」而 4 分钟前刚备份过。Rust 侧是对的（`max(at)` + `ORDER BY at DESC`），改 mock 对齐真读模型。
2. **侧栏「坚果云备份」卡读 `settings['backup.lastSuccessAt']`，那个键从来没被写过** → 永远显示「未配置」，与同屏备份页自相矛盾。改接 `useBackup().status`（唯一来源 `backup_log`），并补出三态：未配置 / 凭据已配但没成功过 / 上次成功 X 前。
3. **`f.name.slice(-21, -8)` 当指纹用** → 实际截出 `129Z-a1b2c3d4`（把时间戳尾巴一起显示了）。换成 `tagOf()` 正则取 `.abackup` 前的 8 hex。
4. **已被成功覆盖的旧失败常驻顶部红色警报**（违反 §2 原则 5）→ 只在 `lastErrorAt > lastSuccessAt` 时显示；正向分支用临时把失败改到最新截了一张 `m4-backup-failing-1400-dark.png` 为证，截完已还原种子。
5. **未配凭据时显示「远端快照 0 份」** → 那是假状态（我们根本不知道云端有几份）。改「未连接 · 数量未知」。
6. **`paths.deviceId` 是文件路径，标签却写「设备 id」** → 而 mock 里同一字段给的是 id 值，两边语义不一致。统一成路径 + 标签改「设备 id 文件」（id 值本来就在详情面板的 `最后写入设备` 里能看到，不重复造字段）。
7. **空流水文案用 `--accent` 绿** → 空态不是成功，改中性 `t3`（新增 `.none`）。
8. **命令面板执行 `backup.now` 时人站在别的视图，结果无处渲染** → 看起来像「点了没反应」。改成先 `shell.go('backup')` 再跑，沿用 `todo.new` 已有的惯例。

另外去掉 `DetailPanel` 空态里「空的时候不占地方」这句——§8.4 明写 wide 档**三栏常驻**，那句话是对用户许愿不存在的实现。

## 本轮定下的判断（别再重开）

- **§8.8.6「同屏不出现两处 `--accent` 实心填充」的适用范围**：流水行那个 6px 成功点**保留**。截图上三行两点一红点，读起来是状态不是噪声；这条禁令的原意是「选中卡 / `.hot` 卡 / 主按钮」三者互斥，M5 工作台按那个逐屏检查。左上角 logo 是品牌标记，不算状态高亮。
- **§8.4 断点按窗口宽度测，不按内容列宽**：`useLayout` 测 `.y-win`。按内容列宽测会自我指涉（列宽由 `cols` 决定，`cols` 由列宽决定）→ 抖动。注释里那句「必须测主区」是历史措辞，行为是对的。

## 复现验证的命令

```bash
cd /d/ADevelop/Repositories/Mine/workbench
npm run typecheck && npm run test
node tools/token-check.mjs && node tools/lint-style.mjs
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
cd .. && node tools/m4-check.mjs          # M4 的门（起桩 + 跑 #[ignore] 的 e2e）
node tools/shots.mjs 备份 --as=backup     # 断点 × 主题截图（自己起 vite，端口 1420）
```

**原生 + CDP 驱动真窗口**（M5 的失焦/最小化/降低动效三态验收要用）：

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222 --remote-allow-origins=*" npm run tauri:dev
node tools/shots.mjs 备份 --as=x --attach=9222 --then='<async js>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture.ps1 -Out shots/x.png
```

## 本轮新踩到 / 新定下的事实（别重复调查）

- **`--remote-allow-origins=*` 不能省**：只给 `--remote-debugging-port` 时，非浏览器客户端的 WS 握手会**静默挂住**（不报错、不返回），要 6 s 超时才看得出来。已写进 `shots.mjs` 的连接超时。
- **`capture.ps1` 报 `window rect came back empty` = 窗口被最小化了**，不是渲染坏了。位置会是 `-32000,-32000`、尺寸 215×26。用 IPC 还原：`__TAURI_INTERNALS__.invoke('plugin:window|unminimize'|'show'|'set_focus'|'center', {label:'main'})`。后台启动的 dev 实例容易被系统最小化。
- 上一轮的事实仍然有效（ureq 要 `allow_non_standard_methods`、roxmltree 要 `descendants()`、`Nonce::from`、KDF 参数要夹上限、桩鉴权用 base64 不是 latin1），详见 git log `ea058e6`。
- 集成测试夹具在 `src-tauri/tests/common/mod.rs`（`MS = 1_793_170_800_000` = 2026-10-28T15:00+08:00，与前端 reltime 夹具同一时刻）。

## 下一轮：M5 的第一批事

1. `views/Dashboard.vue`（今日驾驶舱）：KPI 行（TODAY/INBOX/DONE/LATE + **实时时钟**，34px/800/`-.045em`/`tabular-nums`、`--glow-*` 预烘焙渐变、选中卡翻亮 `#f6faf7` 且文字压到 `#4b5450`）。
2. **3D 卡片环**按 §8.8.1：`perspective:1100px` + rAF **只写 transform**（keyframe 占着 transform 就没法拖动改角度）；`live = 可见 ∧ 聚焦 ∧ 未开降低动效 ∧ faces≥3`，`live` 落假必须 `cancelAnimationFrame`；降级 <3 面退 `RingStrip`、0 面退 `CollapsedRow`。
3. 完成时间轴（<60 天不画趋势线只画点）、「下一步建议」`suggest.rs` 五条规则取前三（单独成表、可单测）、详情面板按 §8.8.3 字段。
4. `Progress.vue` 收纳倒计时/便签×2/速达钉选/系统资源四个可折叠部件（600 ms 防抖 + `visibilitychange→hidden`/退托盘前 flush）。
5. 三轴主题 `data-mode × data-preset × data-accent` 全链路 + 自绘 6px 滚动条 + 骨架屏不用 spinner。
6. **验收**：空/满数据两态无死白；失焦/最小化/降低动效三态巡航真停（`shots.mjs --attach` + 任务管理器 GPU 采样，取不到就以「rAF 已 cancel」的 DOM 断言为证据并如实标注）；改系统时间后倒计时与「逾期 X 小时」跟着变（`reltime.ts`/`FixedNow` 单测为主证据）。
7. 顺带：`marked` 与 `dompurify` 目前零 import（M6 笔记的依赖），M6 收口时要么用上要么删。
