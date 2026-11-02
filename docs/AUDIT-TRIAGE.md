# issue #1 审计分诊与逐条方案

> 对象：[issue #1「deepseek审查结果」](https://github.com/Rewmington/WorkBench/issues/1)，报告基线 `master @ ad7059a`（v0.1.6），与实际 HEAD 一致。
> 复核方式：4 P0 与两处关键翻案由我逐行读码确认；P1/P2 由三组只读检索分头核对（前端 / Rust / 手机与 CI），翻案结论另做二次复读。
> **本轮没有运行任何测试**（与原作者同一局限）。动手前先跑一次 `cargo test` + `npm test` + `flutter test` 确认基线。
>
> **本轮复核的边界（重要）**：这是"对 issue #1 那 47 条逐条判真伪 + 给方案"，**不是独立审计**。检索面就是作者那份清单，下面这些 nobody 看过，别把本文当成"全库只剩这些问题"：
> - 作者自己划走的盲区：`src-tauri/src/ui/`、`linking/` 的四个实现、`tests/sync_relay_e2e.rs` 与 `tests/webdav_e2e.rs`（全 `#[ignore]`）、README/`docs/` 与代码的口径比对；
> - 失败路径类：还原中断、时钟回拨、设备数 >8、并发 `sync_now` —— 仓库里没有这些测试，我也没构造；
> - 整个 `mobile/lib` 的 UI 层、`focus trap`/托盘/单实例/快捷键的交互正确性、`mock.ts` 与真实实现的行为差异；
> - 迁移的可回退性、`win32/launch.rs` 对 UWP/`.lnk` 的处理细节、命令面板的越权项。
> 复核期间顺带撞出来的问题记在第五节（已经比清单多出 3 条，说明盲区里大概率还有）。
>
> 判定图例：
> `真` = 成立，该修 · `真但降级` = 机制成立但后果被夸大 · `半真` = 举的例子/细节错，缺陷本身在别处成立 · `误报` = 读码读反了，不修 · `不修` = 有意设计，不动代码 · `待决` = 需要你拍板
>
> 成因代号（详见文末第六节）：
> `A` 类型上分不出"必须在事务里" · `B` 错误与"没有"长得一样 · `C` 一份契约抄了五遍 · `D` 缺故障注入的缝 · `E` 信任边界没写进仓库 · `F` 有意取舍没就近声明

---

## 一、总账

| 级别 | 条数 | 真 | 真但降级 | 半真 | 误报 | 待决 |
| --- | --- | --- | --- | --- | --- | --- |
| P0 | 4 | 3 | 0 | 0 | 0 | 1 |
| P1 | 19 | 9 | 1 | 5 | 4 | 0 |
| P2 | 23 | 13 | 6 | 3 | 1 | 0 |
| 待定项 | 1 | 0 | 0 | 0 | 1 | 0 |
| 报告内小计 | 47 | **25** | **7** | **8** | **6** | **1** |
| 报告外新发现（批次 7 为止） | 4 | 4 | 0 | 0 | 0 | 0 |
| 批次 8 盲区审计新发现（第十二节） | 16 | 16 | 0 | 0 | 0 | 0 |
| 批次 9 收尾（第十三节：F8 真 / F5 真但降级 / F1 有意设计 / N21 桌面启动无声） | 4 | 2 | 1 | 0 | 1 | 0 |
| 批次 10（第十五节：滚动契约合并时揪出的进度页、七条「有命令没入口」、我自己写坏的降级、迁移与文档口径） | 13 | 11 | 1 | 1 | 0 | 0 |
| 合计 | 84 | **58** | 9 | 9 | 8 | 0 |

**这张表自己的一处旧账（顺手纠正）**：上一版合计 71 = 47+8+8+7 只有 70，差的那 1 条是 P0-4（本地库不加密）从「待决」挪进「误报/不修」时，误报那一格没跟着加。这次补上，误报记 8，列合计 84 = 58+9+9+8 与条数对得上。

**P0-4 那一格「待决」已经在 2026-11-02 拍板**：本地库不加密 = 搁置，理由进档（`docs/THREAT-MODEL.md` 与 `mobile/README.md`），所以合计行的待决记 0。上面「报告内小计」那一行保留 1 不改，是因为它记的是第一版判定当时的事实。

第 4 条新发现（N4）来得最晚也最便宜：**笔记预览里点一个正常链接会把整个应用窗口导航走**（缺 `event.preventDefault()`）。静态读码读不出来 —— jsdom 里不发生导航，而它恰好被 N1 掩盖着：`open_url` 从来没成功过的时候，用户只会觉得"点了没反应"。批次 7 的真窗口那一步撞出来的，见第十一节。

"真"这一列里有一批是债而不是缺陷（types.ts 手抄 663 行、mock 进包 1717 行、release 没开 minify、`*.db` 没 ignore 等），它们和 P0 不该抢同一个顺序 —— 分批见第七节。

**P1-11 是实施批次 1 时才翻案的**：`MACHINE_LOCAL_KEYS` 上方逐字写着"目录地址留着（它机器无关，还原后仍然有效）"，所以"没剥离 medicine 的 davDir"那半句是有意的决定，不是漏项 —— 我从"真"改判成"半真"。这类东西只有动手改到那一行才会露出来，所以第十节把实际改动也记了下来。

一句话结论：报告的骨架站得住 —— **4 条 P0 全部成立**，且 P0-2 的实际爆炸半径比作者写的更大；但 47 条里约 1/3 不可直接执行（6 条误报 + 8 条例子或细节错 + 若干条其实是有意设计），按原文逐条修会浪费掉一轮。

---

## 二、P0（4 条全部成立；其中 P0-4 的处置方式等你拍板）

### P0-1 笔记保存失败会静默丢掉刚写的正文 · 真 · 成因 B/D

- **报告**：`useNotes.ts` 里 `dirty.clear()` 写在 `await` 之前，catch 只 `console.error`。
- **复核**：属实。[useNotes.ts:78-79](../src/composables/useNotes.ts) 先 `[...dirty.entries()]` 再 `dirty.clear()`，:82 才 `await api.notesUpdate`；catch（:85-87）只打日志。失败后 [bodyOf](../src/composables/useNotes.ts) 的 `dirty.get(id)?.bodyMd` 已空，立刻回落到库里的旧正文，界面无任何提示。
- **解决方案**：
  1. 不再整表 clear，改成成功一条撤一条 —— **但必须带身份比较：只把 `dirty.delete(id)` 挪到 `await` 之后是错的，会引入新的丢字**。`await` 期间用户可能又敲了几个字（`scheduleSave` 会往 `dirty` 里塞一个**新对象** `{...prev, ...patch}`），此时若无条件 `dirty.delete(id)`，刚写的那几个字就从待保存集合里消失了，而下一次防抖 `flush` 读到的 `dirty` 已经没有它 —— 于是既不请求也不显示，比原 bug 更隐蔽。正确写法：
     ```ts
     const sent = dirty.get(id)          // 记下发出去的那一份
     const saved = await api.notesUpdate(id, sent!)
     if (dirty.get(id) === sent) dirty.delete(id)   // 期间没被新编辑替换过才撤
     ```
     `catch` 分支什么都不删（数据留在 `dirty`，下次防抖自然重试），只把错误写进可见状态。
  2. 错误要"看得见"：本文件目前没有任何错误出口。复用仓内既有模式（`Todos.vue`/`useBackup.ts` 都是 `errText(e)` 存进可见 ref），加 `saveError = ref<string | null>(null)`，由笔记页那一行状态文案消费。**不新增弹窗机制**。
  3. 补一条测试（Vitest）：`vi.mock('../api')` 让 `notesUpdate` reject → 断言 `dirty` 仍含该条、`bodyOf(id)` 仍返回新正文、`saveError` 非空；再加一条并发用例：`flush` 挂起期间调 `scheduleSave` → 断言成功后该条**仍在** `dirty`（就是步骤 1 那个身份比较）。
  4. **不必**给 `useNotes` 加 api 注入 seam：仓里已有两个 spec 用 `vi.mock`（`components/dash/ring.spec.ts`、`views/linked-degradation.spec.ts`），直接 mock `'../api'` 就能测。

### P0-2 换库还原没有回滚 · 真，且比报告更严重 · 成因 A/D

- **报告**：还原时第二次 `rename` 失败后进程内没有活库。
- **复核**：成立，且窗口比作者写的宽：
  1. [backup/mod.rs:373](../src-tauri/src/backup/mod.rs) `*conn = Connection::open_in_memory()?` 在两次 rename **之前**执行 —— 所以 377、383、甚至 388（写 device_id）任一 `?` 失败都会留下一个内存库连接，不只是"第二次 rename"这一格。
  2. [commands/backup.rs:183](../src-tauri/src/commands/backup.rs) 只在 `outcome.ok` 时 `app.restart()`；而 `restore` 把失败折成 `Ok(Outcome{ok:false})`（mod.rs:316-328）返回 —— 于是**失败路径不重启**，进程继续持有那个没有任何表的内存连接，后续每一个要 DB 的命令都会撞上 "no such table"。
  3. 全仓 grep `pre-restore` 只有这一处写入点 + 一个 e2e 断言它存在，**没有任何启动期捡回逻辑**；而 [db/setup.rs:13](../src-tauri/src/db/setup.rs) 的 `Connection::open` 会在文件缺失时新建空库并跑迁移。所以用户手动重启也只会得到一个全新空库，真数据静静躺在 `app.pre-restore-{ms}.db` 里。
- **解决方案**（按"先不丢、再回滚、最后兜底"三层，都是删机制而不是加机制）：
  1. **换序**：Windows 上带句柄不能 rename，所以必须放开旧连接 —— 但不该先把它换成内存库。改成"两步 rename 全成功后才让 `*conn` 失效"不可行（句柄必须先放），因此做法是：`*conn = open_in_memory()` 之后，把三步文件操作抽成一个函数，失败时**回滚并重新 `setup::open(&paths.db)` 把 `*conn` 指回真库**：
     ```rust
     fn swap_db(paths: &Paths, incoming: &Path, ms: i64) -> Result<()> {
         // 1) 现库 -> pre-restore；2) incoming -> 现库；
         // 3) 第 2 步失败则把 pre-restore 改回来（回滚），返回原始错误；
         // 4) 回滚也失败才报"需要手工从 app.pre-restore-*.db 恢复"。
     }
     ```
  2. **失败也要重启**：只要 `swap_db` 动过真库（成功、或失败后回滚成功），库文件状态就已和进程内连接不一致 —— 与成功路径同样处理，让调用方重启。`commands/backup.rs` 的判据从 `outcome.ok` 改成"库是否被动过"（Outcome 加一个 `db_swapped: bool` 字段，别用字符串匹配错误文案）。
  3. **启动兜底**：`setup::open` 之后加一次检查 —— 若本次是"文件原先不存在、刚刚新建出来的空库"，而 `%APPDATA%\...\app.pre-restore-*.db` 存在，则**不写不删**，只在界面与日志里给出明确一条："检测到还原中断留下的 `app.pre-restore-<日期>.db`，当前库是新建的空库"。自动改名恢复有风险（可能覆盖用户这之后的新写入），所以这一版只提示、不擅自动手。
  4. **测试**（这是关键，之前完全没有失败注入）：`swap_db` 收成纯函数后，测试传入一个不存在的 `incoming` 触发第 2 步失败，断言 `paths.db` 仍在原位、内容未被改动、返回的错误是第 2 步的原始错误。用 `temp dir` + 真 rename，不需要 mock 框架。
  5. `pre-restore-*` 文件顺带纳入清理（见 P2-10）。

### P0-3 课表导入先 DELETE 后 INSERT，且不在事务里 · 真 · 成因 A

- **报告**：表头出现以"周"开头的列（例："周末安排"）会让 `day` 取到 8，撞 `CHECK (day BETWEEN 1 AND 7)`，而 DELETE 已执行 → 整学期清空。
- **复核**：机制全部核对通过，但**要更正两点定性**：
  - 裸执行确认：[commands/timetable.rs:71-72](../src-tauri/src/commands/timetable.rs) 是 `let conn = state.lock(); timetable::import(&conn, ...)`，没有事务；而同仓 [commands/mod.rs:34](../src-tauri/src/commands/mod.rs) 有现成 `in_tx`、[merge.rs:183](../src-tauri/src/sync/merge.rs) 有 `unchecked_transaction` —— 是漏，不是设计。
  - CHECK 确认在 [migrations/0005_timetable.sql:24](../src-tauri/migrations/0005_timetable.sql)；列识别确认在 [timetable.rs:195-197](../src-tauri/src/repos/timetable.rs) 的 `.starts_with("星期") || .starts_with("周")` + 匹配序号推 `day`。
  - **更正 1**：真正的缺陷是"没有事务"，`day=8` 只是触发它的其中一种输入。任何一次 INSERT 失败（磁盘满、格子错位、单行超长）都会先删后败。
  - **更正 2**："周末安排"是作者假想的表头，我没有证据它在真实教务导出里出现过；测试里的 2026-10-30 样例不存在这种列。所以按"能出现的输入"排优先级时，它低于前一条。
- **解决方案**：
  1. **包事务**：`timetable::import` 内部用 `conn.unchecked_transaction()` 把 DELETE + 全部 INSERT + 两条 settings 写入圈在一起（放在 repo 层而不是命令层，因为 `import` 自己就是那个"要么全成要么全不动"的业务单元；命令层的 `&conn` 签名不用变）。
  2. **让越界列变成可诊断的错误而不是 CHECK 失败**：`parse()` 里 `days.len() > 7` 直接返回明说哪一列的错（现在会伪装成"星期8 的格子…不是 5 的倍数"）。同时把列识别收紧为 `星期X` 或 `周X`（X ∈ 一二三四五六日），不再接受任何以"周"开头的字符串 —— 这一改同时消掉 P0-3 的触发面。
  3. **测试**：现有 `importing_replaces_the_same_term_only` 旁边加两条 —— (a) 表头 8 列（含"周末安排"）→ 断言 `is_err()` 且原学期课程行数不变；(b) **故障注入用触发器** —— 别指望"空 name 走 NOT NULL"那种构造：`parse_cell` 先把空行过滤掉了，做不出 `name=''` 的行。可行的做法是在测试库里临时建一个必然失败的触发器，让 DELETE 之后的第一条 INSERT 报错：
     ```sql
     CREATE TRIGGER boom BEFORE INSERT ON classes
     BEGIN SELECT RAISE(ABORT, 'injected'); END;
     ```
     断言导入返回 `Err` 且**原学期的行数一条没少**。这条测试在加了事务之后才有意义，两者一起提交。

### P0-4 手机明文 SQLite 且没关 Auto Backup · 待决（a/b 成立，c 需运行时）

- **报告**：本地库不加密；清单没关 `allowBackup`，Android 自动备份会把 `.db` 带上云；恢复后 Keystore 密钥不在，`credentials.read()` 无 try/catch 会抛。
- **复核**：
  - (a) 成立。`mobile/pubspec.yaml` 只有 `sqflite`，全 `mobile/` 目录 grep 无 sqlcipher；`db.dart` 是裸 `openDatabase`，加密只作用于出门的信封，本地 `anvil.db` 是明文。**注意：任何文档都没写过"本地明文库是有意的"**（PLAN-MOBILE 只记录了密码进 Keystore 这一条）。
  - (b) 成立（静态层面）。`AndroidManifest.xml:6-9` 的 `<application>` 确无 `android:allowBackup`；`fullBackupContent` / `dataExtractionRules` / `res/xml` 备份规则文件全仓零命中。缺 opt-out 是事实；是否真的上传取决于用户设备与 Google 侧设置。
  - (c) 部分成立，需运行时验证。`credentials.dart:35-38` `read()` 确实没有 try/catch，且启动链 `main.dart:55 → store.dart:135 → :152` 全程裸奔（在 `runApp` 之前）；`store.syncNow` 只有 `try { } finally { }`，没 catch。但 flutter_secure_storage 在"Keystore 已失效"的恢复场景下是抛异常还是返回 null，静态判不了，"直接崩溃"这句要先真机验。
- **解决方案（三个选项，成本差一个数量级，请你挑）**：
  - **A（推荐先做，几乎零成本）**：只关自动备份 —— manifest 加 `android:allowBackup="false"`，并为 API 31+ 加 `android:dataExtractionRules` 排除数据库。一行属性，不引依赖、不迁数据、不改现有行为，直接消除"明文离开设备"最主要的通道。
  - **B（顺手，独立于 A）**：`credentials.read()` 包 try/catch → 返回"未配置"并给出"请重填应用密码"的文案，而不是启动即崩。这一条与加密无关，是纯健壮性。
  - **C（大工程，建议明确放弃或长期搁置）**：换 `sqflite_sqlcipher`。代价：新增原生依赖（Android 体积、iOS 需 vendored SQLCipher）、首次启动要迁移已有 `anvil.db`、密钥仍只在 Keystore（恢复后照样解不开，反而把 (c) 变严重）、而且药箱那边也是明文库，口径会不一致。
  - 无论选哪个，**把你决定的理由写一句进 `mobile/README.md`**（成因 F：不写就还会被报）。

---

## 三、P1（19 条：9 真 / 1 真但降级 / 5 半真 / 4 误报）

### P1-1 取消完成是 fire-and-forget · 真 · 成因 B

`src/components/shell/DetailPanel.vue:194` 裸调 `api.todosSetDone(t.id, false)`，无 await 无 reload；同文件 :160 已经写了正确的 `.then(load)` —— 是不一致点，不是缺机制。经 Dashboard 时间轴进入该面板时可复现"仍显示已完成"。
**方案**：照抄 :160 的写法（`await` 后 `load()`）。一行。

### P1-2 切分区的请求没有序号守卫 · 真 · 成因 D

`src/views/Todos.vue:206` `watch(zone, reload)`，`reload()`（:49-62）直接 `rows.value = await api.todosList(zone.value)`，旧响应可能后到覆盖新列表。报告给的参照路径写错了，正确参照是 [src/components/palette/CommandPalette.vue:106,114](../src/components/palette/CommandPalette.vue) 的 `const mine = ++seq` + `if (mine !== seq) return`。影响有限（本地 IPC、4 个固定分区），但连点确实会闪错列表。
**方案**：把 CommandPalette 那 3 行搬到 `Todos.vue` 的 `reload()`。不加通用请求管理器。

### P1-3 课表页加载没有 try/catch · 半真 · 成因 B

`src/views/Timetable.vue:33-39` 确实没 try/catch，`main.ts` 也没有全局兜底。但**结论错**：`st` 保持 null 时页面走的是 `CollapsedRow「课表 · 还没有导入过」`（:139/:179-185），不会整页空白。真问题是**后端报错被渲染成"你还没导入课表"**，把人往错误方向支。文件里 :27 已经有 `error` ref 却没被这条路径用。
**方案**：`load()` 包 try/catch 写进既有的 `error.value`；渲染处区分两种文案 —— 有 error 显示"课表读不出来：<原因>"，无 error 且无数据才显示"还没有导入过"。

### P1-4 拖拽排序的三个 window 监听从不卸载 · 误报，不修

`src/composables/useDragSort.ts:78-80` 的三个 `addEventListener` 在 `start()`（pointerdown 回调）内部，`detach()`（:53-57）在 pointerup/pointercancel 里成对 `removeEventListener`。不存在"每次挂载叠一份"。唯一残留场景是拖拽中途组件被卸载，此时监听仍在，但下一次 pointerup 依然会清掉 —— 影响可忽略，**不动**。
**方案**：无。issue 回复里说明 detach 的存在即可。

### P1-5 删除速达项没有确认 · 真但降级 · 成因 E

`src/views/Launcher.vue:102-105` 无确认、无 undo（对照 `Todos.vue:98` 有 `undo.offer`），触发点是 `src/components/launch/Tile.vue:66`（报告写的 `launcher/` 路径不存在），按钮实测 17×17px 不是 12px。但**"不可逆删除"这句不成立**：[commands/launcher.rs:226](../src-tauri/src/commands/launcher.rs) 走 `resources::soft_delete`，`resources.rs:194` 只写 `deleted_at` 墓碑，数据层可恢复 —— 只是没有恢复 UI。
**方案**：`useUndo`/`UndoBar.vue` 确实存在，但"直接复用现有 undo"这条走不通 ——`Todos.vue:98` 的 `undo.offer({label, run})` 能撤销是因为后端有 `todos_restore(ids)`；而速达这边 **没有任何恢复命令** —— `grep` 全仓只有 `resource_delete`（`commands/launcher.rs:222`）和 `resources::soft_delete`（`repos/resources.rs:194`），没有对应的 restore。撤销要么恢复原 id/排序/密文（做不到，除非新加命令），要么用 `resource_add_from_path` 重新添加（会换新 id、丢顺序、丢已存的密文）。所以真实的选项是：
- (a) **加一次确认**（最小，不动后端，不新增机制）—— 推荐；
- (b) 新增 `resource_restore(id)` 命令 + 撤销（要动 Rust，且得决定顺序与密文怎么处理）。

不要顺手补"回收站 UI"，那是另一个功能。

### P1-6 笔记链接协议白名单一路交到 ShellExecuteW · 误报（但引出第 N1 条真缺陷）

前端半边成立：`src/lib/markdown.ts:29` 的 `SAFE_URI` 确实放行 `/` 开头；Rust 半边也成立：`commands/system.rs:29-31` 无 scheme 校验直接 `open_shell` → `launch_or_raise` → `ShellExecuteExW`。
**但攻击链走不通**：[src/api/real.ts:41](../src/api/real.ts) 发的是 `invoke('open_url', { url })`，命令参数名是 `target` —— Tauri 按名反序列化直接失败，调用点又是 `void` 无 catch，所以**任何 openUrl 都静默失败，SMB 认证永远发不出去**。
**方案**：安全部分不作为 P1 处理；但把"Rust 侧无 scheme 校验"顺手补上（见 N1 步骤 2），因为一旦前端修好，这条链就活了。顺序上必须先修 N1 再补校验，否则只是给一个死代码加防御。

### P1-7 还原失败的报错和按钮不在一个视野 · 真 · 成因 B

确认：还原失败写的是 `useBackup.ts:110` 的 `error.value`，只渲染在 `Backup.vue:207`（凭据那一块）；按钮在 :281（远端快照块），中间还隔着 :225 那处动作块。`ok=false` 但不抛异常的那类结果渲染在 :225-227，仍然不挨着被点的那一行。
**方案**：把这条错误渲染挪到快照块内（同块顶部一行），不新增 toast 机制。与 P1-3 属同一类修法：**错误要出现在触发它的位置**。

### P1-8 LWW 只比墙钟 · 真 · 成因 E/D

[src-tauri/src/sync/lww.rs:50-51](../src-tauri/src/sync/lww.rs) `candidate.0 > current.0 || (相等时比 device_id)` —— 报告"没有 tiebreak"的措辞不准（有 device-id 字典序 tiebreak），但确实只有墙钟。`merge.rs:115-119` 远端胜出时把本地行 `dirty = 0`，于是较新本地改动被较旧远端覆盖后**不再重发**。时钟回拨/前跳是真实场景（虚拟机、双系统、时区手改）。
**方案**（定性：便宜缓解，不是根治；不引向量时钟，那是过度设计）：
1. 缓解本机倒退：`SystemNow` 是 unit struct、`ms(&self)` 无状态（`time/mod.rs:23-29`），所以单调下限要落在一个 `static LAST_MS: AtomicI64` 上 —— 取 `max(now_ms, prev + 1)`，同时若 `now_ms` 比 `prev` 小过一截就在日志与界面说一句"系统时间好像被改回去了"。
   - **不要**把 `last` 持久化进 settings 并在 `ms()` 里写：那会变成"写时间戳 → 写库 → 又要时间戳"的递归。代价是只保证**进程内**不倒退；要跨重启也成立，就在启动时用库里已有的最大 `updated_at` 给 `LAST_MS` 做一次**只读初始化**，绝不在 `ms()` 路径里落盘。
   - 必须改在这一层而不是发布前夹一次：`now.ms()` 散在每个 repo 写入里（`time/mod.rs:8-9` 的注释说明时间只从这一个口子进来，正是为了这个）。
   - 配单测：`FixedNow` 造回拨，断言不倒退。
2. 要说清的残留：这一条**只保证本机的时间戳不倒退，不解决跨设备偏移**。而且夹取会让本机时间戳跑到真实时间之前，另一台设备会更久地认为它"更新"——所以它治的是"自己覆盖自己"，不是"A 机器时钟错盖掉 B 机器"。
3. 真正的损害在 `merge.rs:115-119` 那个 `dirty = 0`：本地改动输给远端后就永久停止重发。这一半要不要动是**产品决策**（可选：本地败者仍保留一轮重发 / 把败者写进冲突记录让人看见），本文不替你定，所以第 1 步做完也不等于 P1-8 关闭。
4. 手机端 `engine.dart` 同一处理，避免两台设备互相倒退 —— 与第 1 步同一版本发出去，否则两端单调性各说各话。

### P1-9 一个坏的对端文件让整轮同步失败，且坏文件删不掉 · 真 · 成因 D

[relay.rs:427-430](../src-tauri/src/sync/relay.rs) 在 `attempt()` 里对单个对端解析失败直接 `?`，:474-489 把它折成 `ok:false` —— 整轮中止，无自愈。`backup/webdav.rs:224 pub fn delete` 的唯一调用点在 `tests/webdav_e2e.rs:196`，**生产代码里没人调用**（报告说"零调用点"不精确，但不影响结论）。
**并且手机同样中招**（报告没看到）：`mobile/lib/sync/engine.dart:91-93` 对单个坏对端 `throw WebDavRemote.wrapPeer(name, e)` —— 同一形状。
**方案**：
1. PC 与手机**同时**把"单个对端解析失败"从"中止整轮"降级为"跳过该对端 + 计入可见错误"。形状我核过（[relay.rs:411-433](../src-tauri/src/sync/relay.rs)）：那四个 `?` 就在 `for entry in entries.iter()...` 循环体里，所以把 :426-430 收进一个内部闭包 `let step = || -> Result<()> { ... }`，失败时记账后继续 —— **不是改一个字符**，但也不需要重构。两个附带点：(i) `peers.push(...)` 在 :426 发生在解析**之前**，现在连"读不懂的对端"也被算进 peers 上报，改的时候一并挪进成功分支；(ii) 手机 `engine.dart:82-93` 已是 try/catch，只是里面 `throw`，改成记录后继续即可。同步成功仍然算成功。
2. 界面把 `跳过 N 个读不懂的对端文件（<id>）` 显示出来，让人知道发生了什么。
3. **暂不自动 `delete` 别人的文件**：那是删除其他设备写入的内容，跨设备不可逆，风险高于收益。留一个显式动作（"清理我读不动的对端文件"按钮）比自动删更合适，且要放到后面的批次再议。

### P1-10 PROPFIND 按字节切 &str 会 panic · 半真 · 成因 E

[src-tauri/src/backup/propfind.rs:54-69](../src-tauri/src/backup/propfind.rs)：`i` 是 `raw.as_bytes()` 的**字节**索引，而合法 `%XX` 的两个后继字符必是 ASCII hex，所以 `&raw[i+1..i+3]` 永远落在字符边界上 —— **报告举的 `%E4%B8%AD` 完全安全，不会 panic**。真实窗口只有一种：畸形 href 里 `%` 紧跟一个多字节字符（`%中`），此时 `[i+1..i+3]` 切进字符内部才 panic；尾部 `String::from_utf8_lossy`（:69）已保护了另一侧。
**方案**：一行 —— 进分支前加 ASCII hex 判断：
```rust
if bytes[i] == b'%' && i + 2 < bytes.len()
    && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() { ... }
```
非 hex 时落到"当普通字节 push"那一支。顺手补一条单测：`percent_decode("%E4%B8%AD%zz%")` 与 `percent_decode("%中")` 不 panic。

### P1-11 三目录隔离靠字符串，且没剥离 medicine 的 davDir · 半真（后半句在实施时才核出是有意的）· 成因 E/F

前半成立：[dav_readonly.rs:65,71](../src-tauri/src/linking/dav_readonly.rs) 判据是 `contains("/workbench-backup/")` / `contains("/workbench-sync/")`，纯子串，不看 scheme/host。
后半**不成立**：`backup/mod.rs:45-51` 的 `MACHINE_LOCAL_KEYS` 上方逐字写着「**目录地址留着（它机器无关，还原后仍然有效）**」—— `linked.medicine.davDir` 不在名单里是记录过的决定，不是漏项。这条我在分诊阶段判成"漏项"是读得不够细，实施时撞见注释才翻案。
**方案**：只做前半。判据从"子串"改成解析后比较：取 URL 的 path 按 `/` 切出的目录段（坚果云是 `/dav/<目录>/…`），与三个目录常量做**段级相等**判断。**不引 `url` crate** —— `Cargo.toml` 里没有它（已核），为一条校验加依赖不划算，手工切段 + 复用本仓已有的 `percent_decode` 就够；改严之后必须拿你真实填过的那几种写法各验一遍（大小写、缺前导斜杠、带查询串），这一段静态读码验不出来。
补测试：`dav_readonly` 面对把 `davDir` 填成备份目录地址的几种写法都要拒绝（现有 `the_backup_directory_is_off_limits_to_the_reader` 只验了一种写法）。

### P1-12 父项软删逐条 UPDATE 无事务；queue.push 是死代码 · 半真 · 成因 A

前半成立：`repos/todos.rs:391-397` 循环内逐条 autocommit UPDATE，`commands/todos.rs:74-75` 传的是 `&conn` 而非 tx —— 中途失败确实会留下"子项已删、父项未删"。概率低（同表 UPDATE），但这是数据一致性。
后半**错**：`:397` 的 `queue.push(hit)` 喂给 `while let Some(current) = queue.pop()` 的 BFS，正是靠它抓孙子节点，不是死代码。
**方案**：`commands/todos.rs` 的删除入口改走现成 `in_tx`（把 `&conn` 换成 `&tx`）；`queue` 不动。issue 回复里替作者澄清这条，避免下次又被报。

### P1-13 命令持 DB 锁跨整段网络 IO · 真 · 成因 A

[commands/sync.rs:76-80](../src-tauri/src/commands/sync.rs) `sync_now` 是 `async fn` + `spawn_blocking`，里面 `let conn = state.lock()` 后一路持有穿过 `relay::run`（list → 每个对端 get → put）直到 `:87 view(&conn, ...)` —— 无 drop 无作用域。60s 超时来自 `relay.rs:87` / `webdav.rs:31`，所以"最坏 60s×(1+N) 秒内所有要 DB 的操作排队"成立。报告没说错的是：UI 线程不冻（在 blocking pool）。
**但 `backup_list_remote` 这句成立且更难受**：[commands/backup.rs:142](../src-tauri/src/commands/backup.rs) `pub fn backup_list_remote`（**非 async**），里面直接做网络 `backup::list(&conn)` —— Tauri 对非 async 命令在主线程执行，**会冻窗口**。
**方案**（分两步，别一次改同步语义）：
1. 立刻：**这类"非 async 却干重活"的命令不止报告说的那一条**（我逐个数过 `commands/` 里所有 `pub fn`）：
   - `backup_list_remote`（:143）—— PROPFIND，网络；
   - `backup_test_connection`（:161）—— `ensure_dir()` + `list()`，两次网络往返，**报告漏了**；
   - `backup_self_test`（:191）—— 不走网络，但在主线程跑两次 `crypto::seal/open`，用的是日备那档 Argon2id（64 MiB × 3 轮），**卡的是秒级**，报告也漏了。
   三条统一改成 `async fn` + `tauri::async_runtime::spawn_blocking`，同文件 `backup_restore`（:172）已经有正确写法，照抄即可（注意它用 `handle.state::<AppState>()` 取状态，因为 `State<'_>` 不是 `'static`）。
2. 记录为已知、后续再评估：`sync_now` 持锁跨网络。要改就得把 `relay::run` 拆成"锁内取待发快照 → 锁外网络 → 锁内合并落库"三段，而第三段的合并语义会变（网络期间本地可能有新写入）。这属于设计变更，放到批次 4 单独讨论，**不要顺手改**。

### P1-14 读设置失败会把默认值写回库 · 真 · 成因 B（本类最重的一条）

[repos/settings.rs:11-18](../src-tauri/src/repos/settings.rs) `get_json` 里 `query_row(...).ok()` 把 SQL 错误折成 `None`，`serde_json::from_str(&s).ok()` 把解析错误折成 `None` —— 无法区分"没有这一项"和"读挂了"。`get_or_default`（:62-66）见 `None` 就 `set_json(default)` **写回库**。于是一次瞬时读失败就把用户设置重置为默认，且静默。`all()`（:44）的 `unwrap_or(Value::Null)` 同毛病。
**方案**（一处改，带走一整类）：
1. `get_json` 不再 `.ok()`：`query_row` 的 `Err` 向上返回（用 `optional()` 区分"无行"与"出错"），只有 JSON 解析失败才折成 `None`，且此时**不写回**。
2. `get_or_default` 改成只在"确实没有这一行"时写默认值；出错就报错，让 `:53-54` 那条注释想要的"界面显示当前生效值"退化成日志 + 提示，而不是覆盖数据。
3. 补测试：预置一行坏 JSON + 一行合法值，断言合法值不被默认覆盖、坏 JSON 返回错误而不是静默默认。

### P1-15 iso_to_day 放过非法日期 · 半真 · 成因 D

`repos/timetable.rs:353` 要求三段且 4/2/2 长度，所以报告的例子 `2-31` **会被拒**（现成测试 `a_junk_school_start_date_is_refused_not_stored` 就覆盖了这类）。真正放过的是**格式对但日历不存在**的日期：`2026-02-31`、`2026-04-31`（:361 只查 `1..=12` 月、`1..=31` 日）。后果推导正确：`20260231` 进 [days.rs:80-87](../src-tauri/src/time/days.rs) 的 `with_ymd_and_hms(...).latest()` → None → `_ => 0` → `elapsed=0` → `week_of` 恒返回 Some(1)。可达性偏低（界面是 `input[type=date]`，浏览器给不出 2-31），但 IPC 直调/手填能进。
**方案**：`iso_to_day` 用已在依赖里的 `chrono::NaiveDate::from_ymd_opt(y, m, d)` 取代手搓的两段范围判断（代码更短，且顺带修掉"1..=31 过宽"）；测试补 `2026-02-31`、`2026-04-31` 两条。

### P1-16 手机只 upsert 快照里有的行，物理删除的行永远留在手机 · 真 · 成因 C/D

确认：`mobile/lib` 全目录 grep `DELETE` 只有 `credentials.dart:44`（vault 那条），`merge.dart:82-128` 只做 UPDATE/INSERT；桌面侧课表确实物理删（`timetable.rs:235`）且每轮新 uuid（:238），注释 :230 写明"导入即整学期替换"；`classes` 在整个 src-tauri 里**没有任何 `UPDATE`**（无软删路径）。所以手机上会积累删不掉的幽灵课，而课表在手机是只读、用户无法自己清。
**方案**：合并时让课表跟着快照走"整学期替换"。先把事实核清（`sync/snapshot.rs:162-180` 的 `dump_table` 是 `SELECT <全部列> FROM <表>`，**全表、含墓碑、不按 dirty 过滤**），所以快照里的 `classes` 就是"那台 PC 当前的全部课"；手机侧确实收课表（`merge.dart:17 const phoneAccepts = ['todos', 'classes']`，已核）。据此替换在语义上成立：
1. 取快照 `classes` 行里出现过的 `term` 集合 `T`，先 `DELETE FROM classes WHERE term IN T`，再插入快照里 `T` 的那批行。**按 term 收窄**，不要整表清空 —— 手机上可能还留着别的学期的课，而那个学期只是这轮没发到。
2. 防误删两档：快照**没有** `classes` 这个键 → 完全不动（对端是没课表能力的旧版）；快照有但**空数组** → 也**不动**并记一条提示。理由：`classes: []` 同时可能表示"PC 真的没课"和"PC 刚从旧备份还原回来"，拿它去清空手机是不可逆的赌注，宁可留幽灵课。要真做到"PC 没课就清空"，得让快照带一个"这次含完整课表"的标志位（协议变更，另开一轮）。
3. 补测试两条：本地有一行旧课、同 term 的快照里没有它 → 合并后被删；快照不含 `classes` 键 或 含空数组 → 本地一行不少。
（本条与"手机课表只能由 PC 发布"这条既有边界一致，是让手机副本更忠实，不改变数据流向。）

### P1-17 手机判断"要不要发布"和桌面不一致 · 误报，不修 · 成因 F

两条被引的语句逐字属实（`engine.dart:100` `if (pendingBefore > 0)`；`relay.rs:442` `pending > 0 || local_rows`），**但推论不成立**：
- 手机发出的表只有 `todos`（`engine.dart:16 const phoneTables = ['todos']`），手机从不发布课表；
- 桌面那个 `|| local_rows` 正是为**没有 dirty 列的 `classes`** 补的，`relay.rs:438-440` 记着 2026-10-31 真机复现（提交 `ad19723`「修 PC 只拉取过仍不上传」）；
- 手机本地新增必然 `dirty = 1`（`repo.dart:160,186`）→ `pending > 0` → 会发。
两端条件不同是数据形状不同。**不动代码**；但值得把理由就近写到 `engine.dart:16` 旁边（现在只在 `docs/PROGRESS.md` 里，读码的人看不到 —— 这正是它被误报的原因）。

### P1-18 缺签名 secrets 时只 echo 一句就发 debug 包 · 真，但属已知权衡 · 成因 F/E

机制准确：`release.yml:100-102` 的 `else echo` 之后，下一步 `flutter build apk --release`（:104-106）**无条件执行**，tag 推送的 publish（:123）照发；`build.gradle.kts:56-59` 的 `if (keystorePropertiesFile.exists()) … else signingConfigs.getByName("debug")` 回退确实存在。这是记录过的取舍：`mobile/README.md:65`、提交 `60206ab`、`docs/PROGRESS.md:703` 自认过 v0.1.0 用 debug 签名的害处（不能跨版本覆盖安装）。
**方案**：回退本身保留（本地自测要用），但**发布不许静默降级** —— `release.yml` 里把"缺 secrets"在 tag 触发时改成 `exit 1`（本地构建走不到那条 workflow，不受影响）。一行，比加"签名标记/产物改名"这种机制更省。

### P1-19 信封内存上限设成 1 GiB · 误报，不修 · 成因 F

`envelope.dart:34 const _maxMKib = 1048576; // 1 GiB` 被当成"载荷上限"，实际是**头部里 Argon2 m_cost 的天花板** —— :32-33 的注释写得很清楚：头部是别人写的，不夹住 m_cost 就能让"打开"去申请十几 GB 内存。它必须等于 Rust 的 `crypto.rs:43 MAX_M_KIB`，是**格式一致性要求**，不是松掉的防护。真实载荷上限在 `webdav.dart:30 maxBytes = 32 MiB`，且在解码之前生效（:128-130）。
**方案**：无。回复时把这两处指给他看即可（顺带说明这条恰好证明信封是被认真防过的）。

---

## 四、P2（23 条：13 真 / 6 成立但无害 / 3 半真 / 1 误报）

多数是债而非缺陷；下表"方案"里 `不动` 表示建议明确不修（并在代码旁留一句理由，避免下次再被报）。

| # | 位置 | 判定 | 问题内容（含更正） | 方案 |
| --- | --- | --- | --- | --- |
| 1 | `src/api/types.ts` | 真（债） | 数字准确：实测正好 663 行，全仓无 ts-rs/specta/codegen 配置 | 与 C 类一起解决：见批次 4 的契约生成；单独立项不划算 |
| 2 | `useTheme.ts:62-77` | 真但降级 | 四个 watch 在函数体内、3 个调用点（`App.vue:34`/`Settings.vue:15`/`dash/FocusRing.vue:26`）确认；但组件作用域 watcher 随卸载回收，**不是泄漏**，代价是一次切主题发 3 次冗余 `settingsSet` | 先确认这四个 watch 依赖的是模块级 ref 还是实例内状态：模块级才挪到文件顶层注册一次（同文件内移动，不加机制）；若依赖实例状态，那 3 份就是设计，**别为消重去引 store** |
| 3 | `src/api/*` 错误处理两种观感 | 误报 | 实测与"一半一半"相反：约 25 处 try/catch 落到可见 error，仅 6 处 `void api.` 无 catch（其中 2 处就是 openUrl） | 不动。唯一值得做的是那 6 处里的 `openUrl`（并入 N1） |
| 4 | 列表不虚拟化 + `mock.ts` 是第二套后端 | 真但降级 | `mock.ts` 实测 **1717** 行（非 1575），被 `index.ts:2` 静态 import 所以确实进 bundle；但选择器是 `!inTauri`，真应用里是死代码，且 `mock.ts:1693` 自己写着"mock 是镜像，不是第二套生产实现" | 不动（有意的开发装置）。若要瘦身用 `import type` + 动态 import，属优化不属修复 |
| 5 | `backup/mod.rs` 明文快照留 3 份 | 真 | `:114` 删当次 `snap-{ms}.db`，但 `prune_local:247-258` 只删超出 `KEEP_LOCAL=3` 的 —— 3 份是**故意留的崩溃兜底** | 不动；在 `KEEP_LOCAL` 旁加一句"明文快照，故意保留 3 份用于崩溃兜底" |
| 6 | gzip 解压无上限（两处） | 真 | `snapshot.rs:77-79` 与 `backup/mod.rs:352-354` 都是 `GzDecoder::read_to_end`；压缩侧上限只有 `relay.rs:55 MAX_BYTES=32MiB` | 小修：解压改为限量读取（`take(MAX_BYTES + 1)` 后判长度即报错），复用现成常量，两处同一写法 |
| 7 | `win32/creds.rs:30-34` 错误折成"还没填" | 真（我补核的） | `take()` 把**任何** keyring 错误（含"凭据管理器不可用"、访问被拒）都折成"还没填 X"，真故障看着像没配置 | 小修：`Cargo.toml:46` 是 `keyring = 4.2.0`（已确认），按其"没有条目"那个变体分派 —— 只有真缺条目才说"还没填"，其余原样上抛。注意 `take` 现在连 `entry()` 的构造失败一起折，分派时要分开这两处 |
| 8 | `creds.rs:99-115` `local_key` 静默换钥 | 真（我补核的） | 已有条目 base64 解不出或长度≠32 时，直接生成新钥覆盖，旧 `resources.secret_cipher` 从此解不开且无提示。缓解：文件头 :8 已声明"解不开就该明说"，解密侧确实会报错 | 分两步，且**别假装一行能修完**：(i) 先做到"不静默"—— 读到内容却解不开时返回错误、不覆盖，这样坏状态可复查；(ii) 恢复路径需要代价：全仓没有 `local-key` 的清除入口（只有 webdav/medicine 两条有），要么错误文案教用户去凭据管理器删 `Anvil / local-key` 那条，要么真的加一个显式动作。（不要在没核过恢复路径的前提下就答应"设置页加重置本机密钥"这种新机制） |
| 9 | `relay.rs:421/435` take(MAX_PEERS+1) 再 truncate | **半真** | 描述属实，但那个 `+1` 看上下文是**有意的富余**：:422-424 会把"自己那份" `continue` 掉不计数，所以取 9 条正好能凑满 8 个对端。只有在自己那份不在列表里（首次同步、库刚被还原过）才会多合并 1 个。 | **不动**。直接改成 `take(MAX_PEERS)` 会把上限从 8 个对端压成 7 个 —— 这是一条"看起来像手滑、其实是留位子"的典型，值得在代码里加一句注释说明 +1 给谁 |
| 10 | `incoming-*` / `pre-restore-*` 明文不清 | 半真 | `pre-restore-*` 确实永不清理；`incoming-*` 成功时被 rename 消费、失败时 `:367` 删除 —— 这半句错 | 小修：`prune_local` 同一规则覆盖 `app.pre-restore-*`（保留最新 N 份）。与 P0-2 一起做 |
| 11 | `todos.rs:333-338` 改提醒两条 UPDATE | 真 | `update_row` + `reminded_at = NULL` 分开执行，命令 `:57` 未包 tx | 走现成 `in_tx`。与 P1-12 同一次改 |
| 12 | `todos.rs:481-486` `rebalance` 无事务且整区标脏 | 真 | 循环 UPDATE 每条 `dirty=1`，无事务；由 `reorder(:457)` 触发，命令 `:96` 未包 tx | 事务部分走 `in_tx`；"整区标脏"是有意的（排序影响邻居），不动 |
| 13 | `countdowns.rs:136-138` 读函数写库且不 touch | 真但降级 | `list → roll → write_row(:159)` 确实写库、确实不置 dirty；但 `snapshot.rs:30 PC_TABLES=["todos","classes"]`，countdowns 从不上云 → **无同步影响** | 不动。若想干净可把 `roll` 的写入挪出 `list`，属重构 |
| 14 | `dashboard.rs:83,134` `unwrap_or` 吞错 | 真 | 两处折成 `None`；`suggest.rs:175` 同模式 | 与 P1-14 同批：改成 `?` 或至少 `log::error!` 留痕。注意 `unwrap_or(None)` 若是 `optional()` 的正常空则不算吞 |
| 15 | `stats.rs:288-291` `created_hour` 无索引全表 GROUP BY | 真 | SQL 确认；0001/0002/0004 的全部索引里没有 `created_hour` | 下条新迁移顺手 `CREATE INDEX idx_todos_created_hour`。本地行数小，属"顺手"不属"紧急" |
| 16 | `resources.rs:109` 锁内 `exists()` | 真但降级 | 属实，但只是一次 `stat()` | 不动 |
| 17 | `note_folders.rs:181-184` 三步裸写不 touch | 半真 | "三步裸写"不准：墓碑那步 `:189-192` **有 touch**；note_folders 不在 `PC_TABLES` 这半句他对了 | 不动（不同步所以无数据损失）。若将来 note_folders 入同步，必须先补 touch —— 这条值得写进代码注释 |
| 18 | `release.yml:38-41` CI 无 cargo test/clippy | 真（高价值） | 仓库只有这一个 workflow，步骤是 typecheck / vitest / `flutter analyze` / `flutter test`，**没有** Rust 测试；`#[test]` 实测正好 223 个 / 40 文件 —— 他数得很准 | 加进 CI（`cargo test --manifest-path src-tauri/Cargo.toml` + `cargo clippy -D warnings`）。这是本轮性价比最高的一条：223 个测试一次都不跑，等于没有 |
| 19 | `package.json:15` `lint:style` 不在 CI | 真 | 确认只出现在 `README.md:99` 与 PROGRESS 的手动记录里 | CI 加一行 |
| 20 | `.gitignore:18` 不遮 `*.db` | 真但降级 | 属实，但库、快照、备份都在 `%APPDATA%`（`paths.root`/`paths.backups`），仓库内实际提交不进来 | 防御性加一行 `*.db`，零成本；不是真风险 |
| 21 | `bench.dart:16` 硬编码凭据 + 无 minify | 真（需更正定性） | 常量确在源码，但只有 `-D ANVIL_BENCH` 才执行（`main.dart:32`），CI 构建不带该 define → release 里不跑；域名是你自己的（`docs/PLAN.md:47`），与 `tests/envelope.rs:220` 同族**测试夹具**，不是活凭据外泄。第二半真：`build.gradle.kts` 完全没有 `isMinifyEnabled`/`shrinkResources`/proguard | 把那组测试账号从源码挪到 `--dart-define`（仓库是公开的，字符串会一直留在 git 历史里，现在就换比以后换便宜）。minify 单独决定：开了要给真机回归验证 |
| 22 | `db.dart:83` 无 onUpgrade，新列被静默丢 | 半真（且是有意的） | `version: 1` 无 onUpgrade 属实；但 `merge.dart:92` 是**跳过未知列**（`k != 'id' && localCols.contains(k)`），不是"把列丢掉"，且 `db.dart:15-16` 注释明说这是刻意的"不会把整行写歪" | 不动。但补一条纪律：**升 version 必须同时写迁移**，把这句写进 db.dart 顶部注释 |
| 23 | `tests/common/mod.rs:81` 硬编码药箱库路径 | 真但降级 | 路径确在，但它只被当作 JSON 字符串用于断言"剥除生效"（`webdav_e2e.rs:140`），没有任何测试真去 open 它 → **不会挂 CI/别人机器** | 可改成 temp 路径更干净，但不修也没有实害 |
| 待定项 | `service.dart:37-38` trim 判空但存原文 | 误报（口径统一） | 桌面 `creds.rs:51-52` 一模一样（`!incoming.trim().is_empty()` 判空、存 `incoming.to_owned()`），Basic auth 与 `邮箱:密码` KDF secret 也都用原文（`relay.rs:178`、`webdav.dart:102`）→ **不是手机独有 quirk，是全项目统一惯例** | 不动。若某天要改成"存 trim 后的值"，必须两端同一版本一起改（KDF secret 变了会导致旧信封解不开） |

---

## 五、报告之外，我复核时翻出的 4 条（建议优先于多数 P1）

### N1 所有"用系统浏览器打开"从来没有生效过 · 真 · 成因 C（本类最典型的证据）

- [src/api/real.ts:41](../src/api/real.ts) `openUrl: (url) => invoke<void>('open_url', { url })`，而命令签名是 [commands/system.rs:29](../src-tauri/src/commands/system.rs) `pub fn open_url(target: String)`。Tauri 按参数名反序列化，缺 `target` 直接 reject。
- 前端两个调用点都是 `void api.openUrl(...)` 无 catch：[Notes.vue:53](../src/views/Notes.vue)（笔记预览里的链接）、[Blog.vue:99](../src/views/Blog.vue)（博客页"在浏览器里打开"）。
- `git log -S` 确认两端来自**同一个提交** `ea058e6`（2026-10-28，M3 外部联动）—— 即 v0.1.0～v0.1.6 里这功能一次都没通过。
- 为什么现有任何检查都抓不到：`tsc` 眼里 `invoke` 的参数是 `Record<string, unknown>`；vitest 跑的是 mock；`cargo test` 不碰前端，而 223 个 Rust 测试在 CI 里也从不执行。
- **方案**：
  1. 前端对齐：`invoke<void>('open_url', { target: url })`。Rust 侧不动（`target` 与 `context_menu` 的命名一致，改动面更小）。
  2. 补上 scheme 白名单（P1-6 修好后这条链就活了）：在 `commands/system.rs::open_url` 里只放行 `http(s)://`、`mailto:`，其余拒绝。校验放在**这一条命令**里，不要放进 `open_shell` —— 后者还被 `open_data_dir` 和速达启动复用，那里需要打开本地路径。
  3. 让契约漂移下次能被抓到：`open_url` 是唯一一个"参数名跨语言靠手抄"的命令吗？不是 —— 几十个命令都是。所以真正的修法见批次 4（一条最小契约检查 + CI 跑 cargo test），不要为这一个 bug 加机制。

### N2 P0-2 的第二、第三层（失败不重启 + 无启动兜底）· 真

报告只写了"第二次 rename 失败没有活库"，实际还包括：失败路径返回 `Ok(ok=false)` 因此**不触发 restart**（进程带着没有表的内存库继续跑），以及重启后 `setup::open` 会**新建一个空库**而没有任何提示。方案已并入 P0-2。

### N3 手机侧同样会被一个坏对端文件拖停整轮 · 真

`mobile/lib/sync/engine.dart:91-93` 对单个对端解析失败是 `throw`，与 P1-9 桌面侧同形状。**方案**：与 P1-9 步骤 1 同时修，只修一边会让两端行为不一致（而这正是 P1-17 被误报的那类"两端差异没写清"的坑）。

### N4 笔记预览点链接会把应用窗口导航走 · 真 · 成因 B（批次 7 的真窗口那一步才发现）

`src/views/Notes.vue` 的 `onPreviewClick` 调了 `api.openUrl(href)` 却没有 `event.preventDefault()`，而预览里的 `<a>` 带着真 href —— WebView2 于是自己跳过去，整个界面被换成那个网站。**方案**：那一句 `preventDefault` 之外，再加一层全局 `on_navigation` 闸门（只放行应用自己的地址），因为这条规则（§8.6「原生软件不在自己窗口里开网页」）不该靠每个 `<a>` 各自记得。落地与证据见第十一节。

它为什么躲过了前一轮所有检查：jsdom 里不发生导航；而 N1 修好之前 `open_url` 一次都没成功过，症状被"点了没反应"完全掩盖。

---

## 六、为什么会长成这样（成因归类，供决定"修一条还是修一类"）

5 天 48 个提交、7 个 tag（M0/M1/M2 全在 10-28 一天）。把这 47 条按病根归，是 6 件事：

| 代号 | 病根 | 为什么会产生这些条目 | 一次消掉一片的动作 |
| --- | --- | --- | --- |
| A | 类型上分不出"必须事务里" | rusqlite 的 `Transaction` 只 Deref 不 DerefMut，这个取舍（`tags.rs:71-72` 有注释）把"要不要包事务"推给每个调用点自觉。于是同一反模式出现 5 次：课表导入、还原换库、todos 级联删/改期/rebalance | 写路径统一走现成 `in_tx`；再加一句纪律：多语句写必须收 `&Transaction` |
| B | 错误与"没有"在类型和界面上是同一个东西 | `Result<Option<T>>` + 双层 `.ok()` / `unwrap_or` / `catch{console.error}`；"失败折成字段不折成异常"在同步侧是有意的（`engine.dart` 头注释），被顺手用到普通读写路径就成了误导 | 修 `settings.get_json` 的形状（P1-14），其余同类跟着走 |
| C | 一份契约抄了五遍，中间没东西钉住 | Rust 结构体 / `types.ts` 663 行 / `real.ts` 参数字符串 / `mock.ts` 1717 行 / 手机那套 Dart + 跨语言常量必须同值（`envelope.dart:34` ↔ `crypto.rs:43`）。N1 就是这个洞的产物，且**不可能被现有任何检查抓到** | CI 加 `cargo test`（223 个测试目前从不跑）+ 一条最小 invoke 契约比对 |
| D | 缺故障注入的缝 | 485 个测试全在 happy path；`fs::rename`、gzip、单对端解析失败都直接写在函数体里，测试无法让它失败。P0 四条正好全落在"不可逆 × 需注入故障"的交集 | 把可失败段抽成纯函数（`swap_db` 是范例），失败注入自然可做 |
| E | 信任边界从没写进仓库 | 这是自用工具，坚果云是你自己的账号，模型是"防手滑不防攻击者"。但这句话只在对话里，`docs/` 没有。于是按通用审计口径必然报出 P1-6/P1-10/P2-6/P2-21 | 写一页 `docs/THREAT-MODEL.md`（半页就够），下次任何审计都会自动降掉一批 |
| F | 有意取舍没有就地声明 | debug 签名回退、mock 进包、留 3 份明文快照、信封 1 GiB 上限、跳过未知列、两端发布条件不同 —— 理由散在 docs 或只在对话里，代码旁边没有一句"这里故意如此" | 就地各加一句注释（约 8 处），成本极低，收益是下一轮审计不再误报 |

顺带一句判定学：**6 条误报 + 7 条"例子错"里，至少一半的根因也是 F**。约束只存在于人脑和注释时，读码的人（人或 AI）无法区分"漏了"和"故意的"，误报就是必然产物。所以 F 那一列的注释不是文档工作，是降低未来成本的工作。

---

## 七、建议落地顺序（等你点头，本文件只到"方案"为止，未改任何代码）

| 批次 | 内容 | 为什么在这个位置 |
| --- | --- | --- |
| 0 基线 | 先跑一次 `cargo test` + `npm test` + `flutter test`，让 223/93/89 这些数字变成"实际绿灯几条" | 三个数字我已各自实测核对（`#[test]` 223、`it(`/`test(` 93、`testWidgets`/`test(` 89）—— **作者的计数是准的**。但"有多少条"不等于"CI 跑几条、过不过"，而本文所有判定都建立在前者之上 |
| 1 不可逆数据 | P0-1、P0-2、P0-3（含 P2-10 顺带）、N1 open_url | 四条都是"数据丢了你拿不回来"，且互相独立，可分小提交 |
| 2 错误形状一类 | P1-14、P2-7、P1-3、P1-7、P2-14、P2-8 | 一处 `settings.get_json` 的形状改动带掉 5 条；这批之后故障不再伪装成"没配置/没数据" |
| 3 同步链 | P1-9 + N3、P1-8、P1-16、P1-11、P1-10 | 需要同时改 PC 与手机，测试面最大，单独一批才好回归验证 |
| 4 机制（让下一轮审计不必靠人肉） | P2-18（CI 跑 cargo test/clippy）、P2-19、N1 步骤 3 的契约检查、F 类的 8 处就地注释、E 类的 `docs/THREAT-MODEL.md` | 不修任何用户可见缺陷，但决定"下次还要不要这样过一遍" |
| 5 小修攒批 | P1-1、P1-2、P1-12、P1-15、P1-18、P2-6、P2-11、P2-12、P2-20；P1-13 步骤 1（**三条命令一起**：`backup_list_remote` / `backup_test_connection` / `backup_self_test`） | 便宜，攒一批扫掉。P2-2 挪出本批：要先确认 watch 依赖的是模块级还是实例状态 |
| 待你选分支 | P1-5（加确认 vs 新加 `resource_restore` 命令）、P2-8（文案教用户删凭据条目 vs 真加一个重置入口）、P1-9 步骤 3（要不要做"清理读不懂的对端文件"显式按钮） | 这三条我给不出"最小且不新增机制"的唯一解，需要你定口径 |
| 明确不动 | P1-4、P1-17、P1-19、P2-3、P2-4、P2-5、P2-9、P2-13、P2-16、P2-22、P2-23、待定项 | 误报、有意设计、或"看着像手滑其实是留位子"（P2-9）；只补注释，不改行为 |
| 等你拍板 | P0-4（A / A+B / A+B+C） | 关 Auto Backup 几乎零成本；换 sqlcipher 是另一回事，且药箱那边也是明文库 |

---

## 八、本文件第一版里我自己写错的方案（留档，免得照着旧版动手）

写方案比判真伪更容易出错 —— 因为"这样改一下"听起来总是合理，但要看改完之后**别的路径还成不成立**。下面 7 条是第二版复核时改掉或收紧的：

| # | 第一版给的方案 | 为什么不对 | 现在的写法 |
| --- | --- | --- | --- |
| P0-1 | "把 `dirty.delete(id)` 挪到 `await` 之后" | `await` 期间新敲的字会进 `dirty`，无条件删就把那几个字连同待保存状态一起抹掉，比原 bug 更隐蔽 | 加身份比较 `if (dirty.get(id) === sent) dirty.delete(id)`，并补一条并发测试 |
| P0-1 | "要测失败路径得先给 `useNotes` 注入 api" | 仓里已有两个 spec 在用 `vi.mock`，直接 mock 模块就够了。这是白加一层机制 | 撤回该步骤 |
| P0-3 | 用"空 name 触发 NOT NULL"做故障注入 | `parse_cell` 先过滤空行，这种 CSV 行根本构造不出来 | 改用 `BEFORE INSERT ... RAISE(ABORT)` 触发器 |
| P1-5 | "复用现有 `undo.offer`，零新增机制" | 后端只有 `resource_delete`，没有任何恢复命令；撤销要能还原 id/顺序/密文才成立 | 改成"加确认"为默认，另一条路是新加命令，标为待你选 |
| P1-8 | "记 `sync.lastIssuedMs`，取 `max(last+1)` 就能防时钟回拨" | 时间戳散在每个 `now.ms()` 写入里，只在发布处夹一次盖不住；且它只防自己覆盖自己，跨设备偏移照旧，夹取还会让本机时间跑到真实之前 | 降级为"便宜缓解 + 写明残留"，落点改到 `SystemNow` |
| P1-13 | 只改 `backup_list_remote` 一条命令 | 同类还有 `backup_test_connection`（网络）与 `backup_self_test`（主线程跑两次 64 MiB Argon2），报告和我都漏了 | 三条一起改，并记下 `State<'_>` 不是 `'static` 要用 `handle.state::<AppState>()` |
| P2-9 | "`take(MAX_PEERS)` 去掉 `truncate`" | 那个 `+1` 是给"自己那份会被 `continue` 跳过"留的位子，照原方案改会把对端上限从 8 压成 7 | 改为不动 + 加一句注释说明 |

另外 P1-9（不是"改一个字符"）、P1-16（`classes: []` 的歧义不能拿去清空手机）、P2-7/P2-8（恢复路径没核过就不该答应新入口）这三条属于"方向对但我说轻了"，第二版也收紧了。

---

## 十、批次 1 实际改动（2026-11-01 落地）

基线先跑了一遍，全绿：**Rust 210 通过 / 13 `#[ignore]`、vitest 93、flutter 89**。改完之后：**Rust 218 通过 / 13 ignored / 0 失败，vitest 97，typecheck 干净**。新增 8 条测试（Rust 5 + 前端 3）。

| 条目 | 改了哪里 | 测试怎么证的 |
| --- | --- | --- |
| P0-1 | `src/composables/useNotes.ts`（逐条 delete + 身份比较 + `saveError`）、`src/views/Notes.vue`（复用既有 `.warn` 行显示） | 新 `useNotes.spec.ts` 两条：失败后正文还在 + flush 期间新改动不被抹掉。**做了变异验证**：把身份比较去掉，第二条如期报 `expected '第一段' to be '第二段'` |
| P0-3 | `src-tauri/src/repos/timetable.rs`：`import` 整段进 `unchecked_transaction`；星期编号改成**取自星期字本身**（`周一/星期一`→1..7），认不出的列（如"周末安排"）直接忽略，同一星期重复列只认第一个 | 两条新测试。**变异验证**：把事务拆掉后 `a_failed_insert_leaves_the_previous_term_intact` 报 `left: 0, right: 5` —— 这就是这条 P0 在一学期的课上的真实后果 |
| P0-2 | `src-tauri/src/backup/mod.rs`：抽出 `swap_db`（第二步失败自动改回原样；回滚也失败时错误里写清该手工改哪个文件）；`*conn` 在任何失败路径上都重新指回真库；设备 id 写失败降级为日志；新增 `prune_pre_restore`（P2-10，留最近 `KEEP_LOCAL` 份）；`notice_interrupted_restore` 只在"库是新建的 + 盘上有 pre-restore"时报一条流水 | 三条新测试：半截换库后真库还在原位且内容未变、成功换库时旧库留在盘上、prune 只留 3 份 |
| 启动兜底 | `src-tauri/src/app.rs`：在 `db::open` **之前**问一次"库是否存在"（open 会顺手建文件，之后就分不出来了），新建且有残留 → 写一条 `backup_log` + `log::error`，**不动任何文件** | 已核渲染路径：`commands/backup.rs:41` 取最近 20 条（含失败行），`Backup.vue:306` 整表渲染 —— 这句提示确实会出现在还原按钮所在的流水里，不是又一个静默分支 |
| N1 | `src/api/real.ts`（`{ url }` → `{ target: url }`）、`src-tauri/src/commands/system.rs`（新增 `browser_address` 白名单：http/https/mailto/ftp，按字节前缀比较避免切在多字节中间）、`src/lib/markdown.ts`（`#锚点` 不再送出去）、`Notes.vue` 的 `openUrl` 由 `void` 改成显示错误 | Rust 3 条测试（含"畸形 `%`/多字节前缀不 panic"、UNC `//host/share` 与 `\\host\share` 被拒）；前端 2 条测试锚定"相对路径要交出去由命令拒绝，不在前端静默吞" |

**与原方案的两处偏离（都是改小了）**：

1. 方案里写的"`Outcome` 加 `db_swapped` 字段、失败也 restart"**没有做**。改成"失败路径把连接重新指回真库"之后，进程状态本来就是可用的，不需要重启，也就不需要新字段 —— 少一处要抄进 `types.ts` 的契约。
2. 方案里"`days.len() > 7` 就报错"也**没做**：编号改自星期字之后，`day` 按构造必然落在 1..7，那个上限成了永不触发的分支（表头检测原本就要求 ≥5 个星期列，`days.len() < 5` 同理是死代码，都删了）。

**批次 1 里刻意没碰的**：`Blog.vue:99` 的 `openUrl` 仍是 `void`（那条页的 URL 来自你自己仓库的清单，且这页没有错误渲染位，加一个就超出本批范围）；P0-4 等你拍板。

### 批次 2（2026-11-01 落地）：把"错误"与"没有"分开

改后：**Rust 221 通过 / 13 ignored / 0 失败，vitest 97，typecheck 干净**；clippy 仍只有 `medicine_math.rs` 那两条既有告警，新代码没添。

| 条目 | 改了哪里 | 验证 |
| --- | --- | --- |
| P1-14 | `repos/settings.rs`：`get_json` 用 `optional()?` 分开"无行"与"读失败"，解析失败也抛（带 key 名）；`get_or_default` 因此只在真缺席时写回默认值；`all()` 保留"一行坏不 Blank 整页"但改成 `log::error` 留痕 | 3 条新测试：`DROP TABLE` 后读故障必须是 `Err` 且**不写回**；坏 JSON 必须报错且库里那条坏内容原样留着（同时 `all()` 给 Null）；真缺席才落默认值 |
| P2-7 | `win32/creds.rs::take`：只有 `keyring::Error::NoEntry` 才说"还没填"，其余上抛原话 | **无法单测** —— 要动真凭据管理器，不在本机替用户建删凭据。核过 v4.2.0 的 `Error` 是 `#[non_exhaustive]`，匹配写了兜底分支 |
| P2-8 | `local_key`：已有内容却解不成 32 字节 → **报错且不覆盖**；只有 `NoEntry` 才生成新钥；只报不修（删条目是用户的显式动作） | 核了波及面：`secrets::decrypt` 只在 `resources.rs:255` 的 `Some(text)` 分支被调，所以坏密钥只影响"看那条加密备注"，不影响启动无密文的速达项 |
| P2-14 | `dashboard.rs:83/134` 与 `suggest.rs:175`：`unwrap_or(None/0)` 换成 `optional()?`，最后一个 `unwrap_or(0)` 保留（那才是"真空库"的口径） | 221 条全跑通；`history_days` 的门槛语义没变 |
| P1-3 | `Timetable.vue`：`load()` 补 try/catch 写进**早就存在却没用上的** `error` ref；空态那一行改成 `v-if="!hasData && !error"` | typecheck + 现有 12 条课表相关测试仍绿；渲染分支是静态判读 |
| P1-7 | `useBackup.ts` 新增 `restoreError`（与 `busy` 一样按动作分，不是新机制），`restore()` 两种失败形状都写这一行，并且**不再写 `result`**；`Backup.vue` 把这一行渲染在快照列表正下方 | 实施时发现比报告说的更糟：`ok=false` 的还原结果原来落在「动作」那块（那块讲的是备份），而抛出来的才在凭据那块 —— 两种失败各去了一处不相干的地方 |

批次 2 里唯一"改了但没测到"的是凭据管理器那两条（P2-7/P2-8），原因如上；这两条要真验只能在你机器上手动改/删 `Anvil` 那几条凭据，属于真机验证清单，不是静态判读能覆盖的。

### 批次 3（2026-11-01 落地）：同步链，PC 与手机同时改

改后总数：**Rust 225 通过 / 14 ignored / 0 失败**，`node tools/m11b-check.mjs` 对真桩 **7/7 通过**（含新增那条），**vitest 97**，**flutter 93**，`flutter analyze` 干净，clippy 仍只有 `medicine_math.rs` 那两条既有告警。合计执行 422 条断言。

| 条目 | 改了哪里 | 验证 |
| --- | --- | --- |
| P1-9 | PC `sync/relay.rs`：每个对端的读-解-合收进一个内部闭包，失败只跳过并记进新字段 `unreadable`；`peers` 仍包含它（文件在就算在线）；`wrap_peer` 失去调用点，删掉。`record()` 与 `SyncStatus`/`types.ts`/`mock.ts`/`sync.spec.ts` 同步加 `unreadablePeers`。手机 `engine.dart` 同一形状，`webdav.dart::wrapPeer` 随之删除，`store.dart` 读出该字段、`peersLine` 一句话点名，替掉了原来那串嵌套三元 | **对真桩跑的 e2e**：`one_unreadable_peer_does_not_stop_the_others_from_merging`（一个合法信封 + 一个 `device-pc-b.enc` 塞垃圾字节）→ 断言 `ok`、`applied≥1`、`unreadable == ["pc-b"]`、`peers` 含两台、那条待办真落库。**做了变异验证**：把"跳过"改回 `return Err(e)`，这条如期红，报的就是旧行为「快照文件太短，不像 Anvil 的备份」带走整轮。手机侧对称测试 1 条 |
| P1-10 | `propfind.rs::percent_decode` 进分支前要求两枚都是 ASCII hex；`percent_decode` 改 `pub(crate)` 供 P1-11 复用 | 2 条新测试。**变异验证实锤**：撤掉兜底后 `percent_decode("%中")` 直接 `end byte index 3 is not a char boundary; it is inside '中'` —— panic 是真的，只是触发输入不是作者举的 `%E4%B8%AD` |
| P1-11 | `dav_readonly.rs::check_dir` 从 `contains` 改成**路径分段相等**（大小写不敏感 + 百分号解码 + 丢查询串），禁区表成 `OFF_LIMITS` 常量。实施时核实：写侧 `webdav.rs::Remote::new` 用的是 `ends_with` 正向白名单，本来就是闭的，所以洞只在读侧 | 新测试列了子串判据真漏掉的 5 种写法（缺尾斜杠、大小写、双斜杠、带查询串、`%77` 编码），并反向断言 `workbench-backup-notes/`、`medicine-sync/` 不被误杀。**后半句翻案**：`davDir` 不剥离是 `MACHINE_LOCAL_KEYS` 上方写明的决定，没改 |
| P1-16 | 手机 `merge.dart::_applyTable`：`classes` 且快照非空时，按快照里出现过的 `term` 先 `DELETE` 再插（在 `applySnapshot` 既有事务内）；快照没带 `classes` 或带空数组一律不动；行缺 `term` 直接拒绝而不是猜 | 2 条新测试：被删的那门不留在手机上；上一学期不被牵连；`classes` 缺失/空数组两种都一行不少 |
| P1-8 | PC `time/mod.rs`：`SystemNow::ms()` 用 `static FLOOR_MS: AtomicI64` 夹单调下限，规则抽成纯函数 `forward_of`（回拨才 +1，同毫秒不 +1），首次检测到回拨 `log::warn` 一次；`app.rs` 启动时用**本机自己**写过的最大 `updated_at`（`updated_by = 本机`，todos+classes）垫高，只读查询。手机 `store.dart`：`Clock.live()` 同一规则 + `Clock.raiseFloor`，`reload()` 用本机 todos 垫高 | 两端各 1 条规则测试（`forward_of` / `Clock.forwardOf`）。**明确没测到的**：`SystemNow` 与 `Clock.live()` 本身依赖真墙钟，测不了；跨重启垫底那条查询在 `app.rs` 里，也没测 |

**这一批我替你做的两个选择（不同意就说，改回来都不难）**：

1. **P2-8 选了"只报不修"**：`local_key` 读到坏内容时报错并让用户去凭据管理器删那条，而不是新增一个"重置本机密钥"的界面入口。已核波及面：`secrets::decrypt` 只在存了密文的行上被调，所以坏密钥只影响"看那条加密备注"。
2. **P1-9 选了"不加自动删除"**：`Remote::delete` 仍然没有生产调用点。自动删别人设备写的文件是不可逆的跨设备动作，要做也该是显式按钮（原文步骤 3，仍待你定）。

**这 7 条本身就是第六节那句结论的证据**：连逐行读过码的复核，也会在"方案"这一层退化成靠直觉 —— 因为真正能证伪它们的只有测试，而仓库现在跑不到那一步（CI 里 223 个 Rust 测试一个都不执行，失败路径也没有注入的缝）。所以批次 0 和批次 4 不是打杂，是把"我说的话"变成"可验的话"。

---

## 九、置信度台账（回答"现在确定没问题了"这个提问）

**没有确定。** 这份文件能给的置信度是分层的，别把整份当成同一等级读：

| 层 | 条目 | 置信度 | 还差什么 |
| --- | --- | --- | --- |
| 判定 | 4 条 P0 全部 + N1 + 8 条翻案（误报/半真） | **高**：我逐行读过码，且翻案那条做了二次复读 | 无（静态层面） |
| 判定 | P1 其余 11 条 | **中高**：agent 按"引用真实代码"的要求交，我抽查了 P1-10/14/15/17/19 全部为实 | 未逐条亲读 |
| 判定 | P2 里 12 条"真" | **中**：二手，行号与引用齐，但我没复读 | 需要时逐条读，约 20 分钟 |
| 判定 | `mock.ts` 1717 行、`#[test]` 223 个、keyring 4.2.0、`phoneAccepts` 含 classes、无 `url` crate | **高**：命令实测 | 无 |
| 方案 | 全部 50 条 | **未经运行验证**，本质是"读码后的设计提案" | 批次 0 + 每条改动配的可失败测试 |
| 全局 | 覆盖范围 | **不是独立审计**：顶部盲区清单里那些没扫过；自查时已多撞出 2 条报告漏的 | 若要覆盖，需另开一轮（工作量与本文件相当） |

已经被这轮自查**收紧过**的方案（说明"我说过但对不够"是会发生的事）：P0-1（身份比较）、P0-3（触发器注入）、P1-5（撤回 undo）、P1-8（改在 `SystemNow` + 不持久化，避免 `ms()` 里写库的递归）、P1-9（不是改一个字符）、P1-11（不引 `url` crate，且改严判据后必须拿真填法验）、P1-13（三条命令）、P1-16（`classes: []` 不用于清空）、P2-8（恢复路径未核）、P2-9（撤回，`+1` 是留位子）。

还有一层更根本的：**这份文档本身也是"没有测试"的产物**。它比 issue #1 强的地方只在于每条判定都带了可点开核对的行号，以及错误被标了出来而不是藏着。

### 批次 4 / 5 / 6（2026-11-01 落地）：剩下的全部扫完

**最终门禁状态**（按 CI 的口径跑，不是按"读起来对"）：
`cargo clippy --all-targets -- -D warnings` **exit 0，零警告**；`cargo test` **226 通过 / 0 失败 / 14 ignored**；`node tools/m11b-check.mjs`（真 WebDAV 桩 e2e）**7/7**；`npm run typecheck` 干净；`npm test` **97**；`npm run lint:style` 通过（84 文件 8 规则）；`npm run contract` **91 个调用点对上 91 条命令**；`flutter analyze` 干净；`flutter test` **93**；`flutter build apk --debug` **exit 0**（清单那条改动因此才算真验过，而不是"看着对"）。

| 条目 | 落地 |
| --- | --- |
| P2-18 / P2-19 | `release.yml` 加 `cargo test` + `cargo clippy --all-targets -- -D warnings` + `npm run lint:style` + 契约检查。**CI 一加就抓到两条真问题**：我在 `backup/mod.rs` 留下的未使用 `Read` 导入，以及一个测试里另写了一份无上限的解压（已改成走生产同一条 `gunzip_bounded`） |
| P1-18 | 缺签名 secrets 时，推 tag 直接 `exit 1`；`workflow_dispatch` 干跑仍允许 debug 签名。回退本身保留，注释指回 `mobile/README.md` |
| N1 / P2-1 那一类 | 新增 `tools/contract-check.mjs` + `npm run contract`：按括号配对解析 Rust 命令签名（跳过 `State`/`AppHandle` 等注入类型、`Option<T>` 视为可省），与 `real.ts` 每个 `invoke` 的顶层键比对。**反向验过**：把 `open_url` 改回 `{ url }`，它精准报出「前端传了 { url }，Rust 签名里没有」+「Rust 要参数 target，前端没传」；改回来 91/91 |
| 成因 E | 新增 `docs/THREAT-MODEL.md`：边界表 + 一节「明说不防的」（gzip 炸弹绝对上限、凭据管理器不可用、多用户、跨设备时钟），P0-4 的未决口径写在最后一节 |
| 成因 F | 就地注释：`relay.rs` 的 `take(MAX_PEERS + 1)` 是给自己那份留位子（不是笔误）；`engine.dart::phoneTables` 解释两端发布条件为何不同；`webdav.rs` 那两条邮箱注释保留 |
| P1-13 | `backup_list_remote`、`backup_test_connection`、`backup_self_test` 三条非 async 的重活命令全部改 `async` + `spawn_blocking`（后两条是报告没写、我数 `pub fn` 时发现的） |
| P1-12 / P2-11 / P2-12 | 写路径统一挂现成 `in_tx`：`todos_update` / `set_done` / `delete` / `restore` / `reorder`。按类收，不挑三条改 |
| P1-15 | `iso_to_day` 改用 `NaiveDate::from_ymd_opt`；测试补 `2026-02-31`/`04-31`/`02-30` 拒绝 + `2028-02-29` 放行（闰年别误杀） |
| P2-6 | `snapshot::gunzip_bounded(bytes, max)` 一处实现：同步 64 MiB / 还原库 512 MiB。测试用 4 MiB 零字节流（压完 <8 KB）证明越界会拒、合法小文件照放行、非 gzip 仍报"解不开" |
| P2-15 | 新迁移 `0007_created_hour_index.sql`：局部索引的谓词与 `hour_bars` 的 WHERE 一字不差（写成全表索引可能不被选中） |
| P1-1 | `DetailPanel.vue` 取消完成补 `.then(load)`，与同文件另一处一致 |
| P1-2 | `Todos.vue::reload` 加序号守卫；`finally` 里只有最新一轮才收掉 loading |
| P1-5 | 速达 × 改成两步内联确认（沿用笔记页「彻底删除」的 5 秒撤回节奏）。实现时撞见一个连带问题：`.tools` 只在 hover 时出现，armed 态必须一直可见，否则"已武装"是隐形的 |
| P2-20 | `.gitignore` 补 `*.db` / `*.db-wal` / `*.db-shm` |
| P2-23 | 测试夹具里两条真机路径与两处真实邮箱换成显然假的值（仓库是公开的），两条邮箱改成不同值，以便测出"剥的是键，不是某一键" |
| P0-4 (a/b) | manifest 加 `allowBackup="false"` + `fullBackupContent="false"` + `dataExtractionRules`（新增 `res/xml/data_extraction_rules.xml`，API ≤30 与 31+ 两头都关）；`credentials.read()` 加 try/catch 按未配置处理，恢复过的手机不再起不来 |
| P2-21 | `bench.dart` 那串账号形状的字面量换成不可能存在的域名；要真凭据走 `--dart-define=ANVIL_BENCH_SECRET`。（旧值仍在 git 历史里，要彻底抹得改写历史，另说） |

### 仍然没做的五条（都要你点头，不是遗漏）

> 这一小节是批次 6 当时写的，**已被第十一节末尾的「更新后的仍未做清单」取代**（第 5 条真窗口验证已做完，并新增了两条要你看一眼的）。留在这里是因为它记着当时的判断，不是因为它还作数。

1. **P0-4 方案 C（`sqflite_sqlcipher`）**：没做。理由见 `docs/THREAT-MODEL.md` 末节 —— 引原生依赖 + 迁移已有明文库 + 密钥仍只在 Keystore（恢复后照样解不开），而药箱那边同样是明文库。
2. **release 不开 minify/R8**（P2-21 后半句）：没开。开了必须真机回归一遍确认没砍掉反射用到的类，那是"需要设备"的验证。
3. **`types.ts` 663 行换 codegen**（P2-1）：没做，只做了能抓参数名漂移的契约检查。全量 codegen 要动构建链（ts-rs / specta），另一个量级。
4. **P1-9 步骤 3**（清理读不懂对端文件的显式按钮）：没做，见上面第 3 条选择。
5. **真窗口验证**：`open_url` 修好后"点了真能弹浏览器"、笔记保存失败那行长什么样、速达两步确认的手感 —— 都要在你机器上起 `tauri dev`，我没擅自起（单实例锁会抢你正在用的实例）。

### 补记：成因 F 那批就地注释（第二版漏了 4 条，后补齐）

`useTheme.ts`（P2-2，查证后确认那四个 ref 是模块级单例 —— `toggleMode()` 在组件外改 `mode.value` 就靠这点 —— 所以四个 watch 从 `useTheme()` 挪到模块作用域，一次切主题不再发三次 `settingsSet`）；`note_folders.rs`（P2-17，写明"不在 `PC_TABLES` 才允许不 touch，入同步前必须先补"）；`backup/mod.rs` 的 `KEEP_LOCAL`（P2-5，写明留 3 份明文快照是崩溃兜底、动它之前先想清 P0-2）；`mobile/lib/db.dart`（P2-22，写明升 `version` 必须同时补 `onUpgrade`，否则 `merge.dart` 的"跳过未知列"会把事故伪装成成功）。

补齐后重跑：typecheck 0 / vitest 97 / contract 91 对 91 / `cargo test --lib` 142 通过 / `clippy --all-targets -D warnings` exit 0 / `flutter analyze` 干净 / `flutter test` 93 全过。

---

## 十一、批次 7（2026-11-01 落地）：先取证，发现上一轮的"改完"里有两步没跑过编译

**接手时工作区的真实状态**（不是我上一轮记录的样子）：

- `cargo test` **编译失败**：`backup::credentials` 已改成返回 `Result<Option<…>>`，但两个调用点没跟上 —— `commands/sync.rs:60` 还在 `.is_some()`、`sync/relay.rs:392` 的 `webdav_credentials` 还声明成 `Option<(String,String)>`。
- `flutter analyze` **6 个错**：`snapshot.dart` 用 `extends` 继承 `abstract interface class ChunkedConversionSink`（库外不许继承，且接口只有 `add`/`close`，没有 `addSlice`）、`const GZipCodec()` 并非常量构造、`repo.dart:180/202` 把 `int?` 传给要 `int` 的 `dayInt`。

也就是说第十节末尾那句"批次 0-6 全改完"对**最后两步**（P2-7 的 PC 凭据半边、手机侧 P2-6/P1-14）是不成立的：代码写了，编译器和 analyze 没跑。**这正是这份文档第九节自己说过的那件事** —— 没有跑过的"改完"只是读起来对。

### 补完与新增

| 条目 | 落地 | 怎么证的 |
| --- | --- | --- |
| P2-7（PC 续） | `relay.rs`：`webdav_credentials` 跟上 `Result`，`run` 的 `Err` 走新抽的 `failed()` —— 记进 `sync.state`。顺带把 `Remote::from_env` 的失败也收进同一个 `failed()`：原来它只把 Outcome 返回给调用方，而 `commands/sync.rs:94` 的注释写着「run 已把结果记进状态」，开机/定时那一次什么都不留，注释是假的 | 新测试 `an_unreadable_credential_is_a_failure_not_a_skip`（`DROP TABLE settings` 造真故障，不碰凭据管理器）。**变异验证**：把 `webdav_credentials` 改成 `.ok().flatten()`，这条如期报「读库故障不许冒充『还没配』」 |
| P2-7（PC 续） | `commands/sync.rs::view`：`configured` 沿用备份页 `read_status` 已有的口径（显示用的布尔可以折成 false，但 `log::warn!` 留痕），不新开字段、不让整块状态读失败 | 上面那条测试覆盖的是停摆级；这一处是提示级，判据写在注释里指向 `relay::run` 的记录路径 |
| P2-7 第三处（自查新增） | `commands/linked.rs::cloud_read`：药箱那条密码读不出来时不再折成「压根没启用云端」，改走既有的 `CloudRead::Failed` —— 卡片上就是「云端这次没读上：…」那句原话。配套加 `creds::medicine_password_present()` | 与 webdav 同一形状，静态判读 + 渲染路径已核（`medicine_copy.rs:98` 那条 format 分支）；真凭据管理器仍不可单测 |
| P2-6（手机续） | `_CappedSink implements ChunkedConversionSink<List<int>>`（只实现 `add`/`close`）；`GZipCodec()` 去掉 `const`；越界在 `gunzipBounded` 内部就翻成 `SnapshotException`，`fromBytes` 加 `on SnapshotException rethrow` | 新文件 `mobile/test/snapshot_budget_test.dart` 两条：4 MiB 零字节流对 1 MiB 上限必须拒、合法小文件照放行、非 gzip 仍报「解不开」；第二条真按生产默认上限（64 MiB+1）走 `fromBytes`。**变异验证**：撤掉 `rethrow` → 报的是「快照解不开 gzip」而不是「上限」，正是那句会被改写的事 |
| P1-14（手机续） | `loadOrCreateDeviceId`：`device.id` 解不开 JSON 时**拒绝写**并抛错。这是 P1-14 在手机上的更危险版本 —— PC 那边写回的是一个默认值，这边换掉的 id 会让本机每一行的 `updated_by` 一夜之间变成别人写的（平票裁决、待上传计数、`Clock.raiseFloor` 全押在它上面） | 新测试断言抛 `StateError` **且坏行原样留在库里**。**变异验证**：撤掉那个分支 → 测试拿到一把新 id |
| P1-14（手机续） | `sync.state` 坏掉或解出来不是对象 → 新方法 `syncStateUnreadable()` → 占**已有的** `syncError` 那一行说「读不出来（不是还没同步过）」；一次真同步会把它重写回来 | 新测试三分支（缺席/坏 JSON/形状不对）+ 恢复后 `syncError` 归 null。**变异验证**：把 `if` 夹成 `false &&` → 报 `Expected: contains '读不出来' / Actual: <null>` |
| P1-14 收窄 | 上一轮留的 `readForDisplay` 收掉：它的 `.state` 没有任何调用方读，学期/开学日于是回到 `readSetting`。判据写在注释里 —— 那两条读不出来顶多是课表空着、重新同步一次就补上，不值得各开一条错误行 | 少一个名字指同一件事 |
| 手机 `done_day` | `saveDone`/`insertTodo` 把 `next.doneAt` 落成局部量（Dart 不跨库收窄字段），取消勾选时那一格连键都不带 | 新测试：完成后 `done_day == dayInt(done_at)`、取消后仍在。**变异验证**：撤掉 `if` 里那半句 → `Expected: <20231115> / Actual: <null>` |

### 真窗口验证（批次 6 列的第五条，这次做了）——顺手抓到一个新缺陷

用一次性数据目录跑 dev（`LOCALAPPDATA` 指到 Temp、`ANVIL_WEBDAV_BASE` 指到死端口），**他的真实库与云端这次没被碰过**。

**新缺陷（报告没有、我上一轮也没有）：笔记预览里点一个正常链接，会把整个应用窗口导航走。** `onPreviewClick` 只调了 `api.openUrl`，没有 `event.preventDefault()`，而预览里的 `<a>` 带着真 href —— WebView2 于是自己跳过去，界面整个被换掉。这条恰好被 N1 的修复**掩盖**过：参数名修好之前 `open_url` 从来没成功过，用户看到的只是"点了没反应"。jsdom 里不会发生导航，所以单测看不见。

两层都补了：

1. `Notes.vue`：`event.preventDefault()`，注释写明这一句是 §8.6 那条规则的兑现点。
2. `app.rs`：全局 `on_navigation` 闸门（`tauri::plugin::Builder` 那条路，因为 `tauri::Builder` 上没有这个钩子），判据抽成纯函数 `is_app_url` —— 只放行 `tauri://`、`localhost`/`127.0.0.1`、`tauri.localhost`、`about:blank`，其余拒绝并 `log::warn`。这一层管的是"以后漏掉的那些"和页面自己写 `location.href=`。2 条新测试（各 5 个地址）。

真窗口量到的四条：

| 要验的 | 结果 |
| --- | --- |
| `open_url` 修好后真能弹浏览器 | `https://example.com` 的 invoke 返回成功且 `msedge.exe` 进程数 20 → 21；`javascript:` / `cmd.exe /c calc` / `//host/share` / `file:///C:/Windows/win.ini` 四类全被拒，报的是同一句白名单话 |
| 闸门真的拦得住 | 从页面里执行 `location.href='https://example.net/'` → 地址纹丝不动，而同一地址交给 `open_url` 仍然 RESOLVED |
| 笔记 `saveError` / `linkError` 两行 | 把正在编辑的那条笔记在服务端彻底删掉，让保存**真失败**一次：`.warn` 出现「1 条改动没保存上，内容还留着（切笔记或关掉这页会重试）」，就在编辑区正上方，且那几个字仍在文本框里（`shots/m13-notes-saveerror.png`）。`linkError` 用"前端放行、命令拒绝"的绝对路径 `/Windows/win.ini` 真触发（`shots/m13-notes-linkerror.png`）；`#锚点` 那条没新增任何一行，符合"不该交出去" |
| 速达两步确认的 armed 态 | 不 hover 的前提下点一次 ×：`.tools` 的计算样式从 `display:none` 变 `flex`、`title` 变「再点一次真的移除」、`.armed` 在（`shots/m13-tile-armed.png`）；5.5 秒后自动解除，那颗格子还在（没被误删） |
| 课表错误行 vs 空态 | **没在真窗口拍**：那一条要经原生文件对话框，我没在你机器上弹它。改成了组件测试 `src/views/timetable-error.spec.ts`（jsdom 挂载，两种口径**不同时出现**：报错时不许出现「还没有导入过」）。**变异验证**：把 `v-if="!hasData && !error"` 的 `&& !error` 去掉 → 那条如期红 |
| 闸门在 **release** 二进制里（不是 dev） | 补跑 `npx tauri build --no-bundle`（2m06s）后用一次性数据目录起 `target/release/anvil.exe`。dev 是 `http://localhost:1420`，release 是 `http://tauri.localhost/` —— **这两个走的是 `is_app_url` 不同的分支**，判据写错的表现是"release 打开就是空窗口"，而那种事故只在 release 里出现。实测：release 首页渲染完整（`.y-win` 在、侧栏十一项与驾驶舱文案都在），再从页面里执行 `location.assign("http://example.com/")` → 连续 4 秒轮询 8 次，地址一直是 `http://tauri.localhost/`，窗口仍然可用。**这条原来只是"读起来对"，现在是真验过** |

### 批次 7 终版门禁（按 CI 口径跑）

`cargo clippy --all-targets -- -D warnings` **exit 0，零告警**；`cargo test` **232 通过 / 0 失败 / 14 ignored**（`--list` 241 条全对上）；`node tools/m11b-check.mjs` **7/7**；`npm run typecheck` 干净；`npm test` **99**（15 个文件）；`npm run contract` **91 对 91**；`npm run lint:style` 通过（85 文件 8 规则）；`flutter analyze` **No issues**；`flutter test` **98**；`flutter build apk --debug` **exit 0**。

本轮变异验证 7 次，每一次都如期变红（PC 凭据 2 次、手机 4 次、课表 1 次），改回后重跑全绿。

### 更新后的"仍未做"清单（都要你点头）

1. **P0-4 方案 C（`sqflite_sqlcipher`）**：没做，理由见 `docs/THREAT-MODEL.md` 末节。
2. **release 不开 minify/R8**：没开，要真机回归。
3. **`types.ts` 663 行换 codegen**：只做了契约检查。
4. **P1-9 步骤 3**（清理读不懂对端文件的显式按钮）：没做。
5. **手机 `device.id` 读不出来时拒绝启动**：这条是我这轮定的口径 —— 宁可起不来也不换身份。代价是那种库要靠手工修那一行才能再启动（你有 adb 和 sqlite，但没有"界面里能点一下"的出口）。要改成"用一次性 id 跑着但不写回"也不难，说一声。
6. **凭据管理器那三条**（P2-7 的 webdav、药箱、P2-8 的 local-key）：仍然只能静态判读 + 真机手动改条目才能验，我没在你机器上建删过任何凭据。
7. **课表错误行**只有 jsdom 证据（见上表最后一行）。

---

## 十二、批次 8（2026-11-02 落地）：把第一节那份「盲区清单」真的扫一遍

前十一个批次都在回答"issue #1 说的对不对"。这一轮换问题：**这份文件自己声明没扫过的地方有什么**（顶部那六行盲区）。做法是四路只读并发审计，每路都要求"每条主张必须带我自己打开读过的 file:line、必须说得出触发和后果"，收上来 26 条候选，再由我逐条复读判真伪。

四路分工：桌面原生层（`ui/`、`hotkeys`、`win32/launch`、`tests/`、单实例）／同步与 linking 的失败路径（并发 sync、`MAX_PEERS`、还原中断、时钟回拨、linking 四实现）／前端（`mock.ts` 与真实现的行为差异、命令面板越权、focus trap、README 与代码口径、错误当缺失）／手机 `lib/ui`。

### 桌面那一路：我复读后确认成立并已修的 8 条（都进了门禁）

| 编号 | 症状（修之前） | 落点 | 怎么证的 |
| --- | --- | --- | --- |
| N5 | 版本号散在四处清单（`package.json`/`tauri.conf.json`/`Cargo.toml`/`pubspec.yaml`），少抬一处或 tag 打错，**CI 照样绿** —— 产物名跟着 tag 走，里面装的是上一个版本 | 新 `tools/version-check.mjs` + `npm run version:check`，挂在 `release.yml` 最前面（tag 触发时比对 tag，干跑只查四处一致）；README 发布那节补了"四处一起抬" | **变异验证**：把 `tauri.conf.json` 改成 0.1.7 → 四处逐行列出并 exit 1 |
| N6 | 托盘与右键菜单把非原生项 `emit('menu://click', id)` 交给前端，而**前端从来没有一处 listen** —— 于是「复制选中内容」点了没反应，托盘「设置」只唤窗不切页。它是 N1 的亲兄弟：参数名对不上会当场报错，事件名对不上只是安静地什么都不发生 | `App.vue` 补 `onMenuClick`（`settings` 切页；`copy-selection` 复制选区，走 `execCommand` —— `http://tauri.localhost` 不是可信来源，`navigator.clipboard` 在部分 WebView2 上是 undefined）；`contract-check.mjs` 加第二层：Rust `emit` ↔ 前端 `listen` 双向比对 | `git log -S "menu://click" -- src/` **一条记录都没有**，而它随 `af63b42`（M0 骨架，v0.1.0 起就在）进了 Rust —— 这条通道从第一个发布版起就没通过。**反向验过**：撤掉那条 listen → 检查精准报「事件 'menu://click' 由 src-tauri/src/ui/menus.rs 发出，但前端没有任何 listen」 |
| N7 | 窗口配置是 `visible: false`、前端就绪后才 `show()`；而 `onMounted` 里 `theme.load()`（`settingsAll`，**无 catch**）任一条抛错就整段中断 —— 表现是"双击图标没反应，进程却在托盘里跑"，正是最难查的那种坏法 | `App.vue`：三条 load 收进 try，`show()` 之后再决定要不要提示那一行 | 静态可证（`grep -c catch useTheme.ts` = 0，`tauri.conf.json:28` `"visible": false`）；抛错路径需真窗口，列进待验 |
| N8 | `Notes.vue` / `Progress.vue` 是 `onMounted(async () => { await load(); addEventListener(...) })`：载入没回来就切走时，卸载钩子拆的是空函数（Notes 那边写的还是 `?? (() => {})`），随后续体照样把监听挂上 —— 每次快切漏一对，失焦时还会去 flush 一个已卸载的模型 | 两个视图都改成"引用先定死、挂载即注册、卸载按同一引用拆" | 新 `src/views/listener-lifecycle.spec.ts`（挂→立刻切走→再放行那个 await，三条断言：挂载时就位 / 拆的是同一引用 / 续体不许补挂）。**变异验证**：把 Notes 改回"await 之后才挂" → 报 `expected 2 to be +0` |
| N9 | 还原成功后传进来的那个连接停在**空内存库**上（只有失败分支把它指回真库），要一直等 `app.restart()` 才死；中间在 mutex 上排队的每条命令都报 "no such table"。另一半：`swap_db` 失败时那份明文 `incoming-{ms}.db` 没人删，`prune_local` 只扫 `snap-*` | `backup/mod.rs::restore_with`：成功分支 `match db::open(...)` 指回真库（打开失败只记日志，不把已成功的还原说成失败）；失败分支先删 incoming | 断言加在 `tests/webdav_e2e.rs`（对真桩跑 `node tools/m4-check.mjs`）。**变异验证**：把 `*conn = reopened` 改成 `Ok(_) => {}` → 那条 e2e 如期 FAILED。同一段旧注释"换库之后旧连接是内存占位，要重开才能查"其实是把一个缺陷写成了规矩，一并改掉 |
| N10 | 全局快捷键回调在 Windows 上跑于**主线程**（`global-hotkey` 的隐藏窗建在主线程、`send` 同步），而它第一件事就是 `state.lock()` 读设置 —— 同步那一整段网络 IO 都持着 `db` 那把锁（P1-13 第 2 步明确没做）。于是开机 60s 后那次同步卡住时按 Alt+Shift+A，冻的是整个 UI 事件循环，而用户恰恰是在窗口叫不上来时才按它 | 新增 `AppState::hotkey`（注册成功时写、换绑时跟着换），`on_event` 只比这份缓存，一次库都不碰；判据抽成纯函数 `matches` | 2 条新测试（`Alt+Shift+A` / `shift+alt+KeyA` / `SHIFT+ALT+A` / 带空格 都算同一个键；换绑后旧键与坏值都不响应）。注释里那句"这个 bug 只有真机能暴露"现在由测试盯着 |
| N11 | `migrations.rs` 里"半个迁移不能伪装成已应用"只有注释，没有任何一条测试真的注入过一次失败的迁移 | 循环抽成 `apply(conn, &[Migration], from)`，加一条注入真故障的测试 | **变异验证**：吞掉 `execute_batch` 的错误 → 报 `called unwrap_err() on an Ok value: 2`，正是那句注释要防的事。写这条测试时它先抓到我自己的错（`skip(from)` 是按整表算的），所以它对"续跑"那条边也是有效的 |
| N12 | 第一次"收进托盘"那句提示永远看不见：`on_close_requested` 先 `hide()` 再 `emit`，而藏起来的页面里 toast 的 6 秒计时器照跑 —— 用户把窗口唤回来时那行字早走完了；`tray_hint_shown` 又已经在第一次就消耗掉 | `AppState::tray_hint` 换成三档状态机（0 没收过 / 1 欠一句 / 2 说过了），`show_and_focus` 时才补发；`owe_hint`/`spend_hint` 用 `compare_exchange` | 1 条纯函数测试钉住"只欠一次、只说一次"；隐藏态计时器那条边留给真窗口 |

### 26 条候选里其余的判定（不是每条都已经由我复读）

**判真但没动手（要点头，或成本超出这一轮）**：

- 命令面板 `CommandPalette.vue:144/155/167`：声明了 `role=dialog aria-modal=true`，但 Escape 只绑在 `<input>` 上、也没有焦点陷阱 —— 焦点一旦 Tab 出去，Esc 不再关面板，遮罩下的控件还能被 Tab 到。**我复读确认结构属实**（真·a11y，修法是 keydown 上移到面板容器 + trap + 归还焦点）。
- `MAX_PEERS`（`relay.rs:454`）：窗口之外的对端**静默不参与合并**，而顺序由 listing 决定（坚果云按名字排，`device-pc-*` 恒在 `device-phone-*` 前）；还原一次换一个新 `pc-<uuid>`，旧 `device-*.enc` 在生产代码里没有任何删除路径（P1-9 步骤 3 仍未做），攒到 9 份以上不稀奇。**这轮只做到"在日志里点名被跳过的那几份"**；界面上点名要加字段（`sync.state`/`types.ts`/`mock`/手机同步改），取"最新 8 个"要 PROPFIND 多解析 `getlastmodified` —— 两个都留给下一轮。
- 托盘「退出」那句 `w.eval("window.__anvilFlush && …")`：前端**从没定义过** `__anvilFlush`，而 eval 是 fire-and-forget，紧跟着的 `exit(0)` 也会抢在它那几个 IPC 之前杀进程 —— 注释承诺的"退之前冲刷防抖"是假的。我没有新建一条跨语言 flush 协议（前端注册各自的 flush → 面板事件 → `quit` 命令 → Rust 兜底超时），而是**删掉那句假代码 + 把真实暴露面（打字那一格 ≤600ms）写在原位**，协议那版等你点头。
- `mock.ts` 与真实现的行为差（药箱目录禁令仍是子串判据、删除仍是物理 splice、空标题不校验、palette 无每类上限、补买文案少一个分支）—— 都成立，但 `vi.mock('../api')` 让 vitest 根本不经过 mock，所以这些差异骗的是**在浏览器里做验证的人**（也就是你）。要么逐条镜像（每条都要配测试），要么把 mock 的"仅用于演示"写进文件头 —— 需要你定方向。
- 手机侧 8 条候选（课表整学期替换不比版本戳、只改邮箱会换掉 KDF 口令、两屏两句话、撤销条按标题找、热恢复不重新同步、本地写失败无声、药箱"坏了"与"没药"同形、`reload` 竞态）**我还没逐条复读**，只做了下面这条抽查。

**判为夸大 / 条件不成立 / 已有依据**：

- "PC 两个 `sync_now` 会交错" —— agent 自己读完后翻案：`commands/sync.rs:97` 与 `schedule.rs:25` 都把 mutex 持过整个 `relay::run`，串行化恰好成立（代价是排队，那正是 P1-13 第 2 步记录过的债）。
- "还原换 id 会让 `updated_by = 本机` 那条时间戳下限失效" —— 机制成立，但要"还原 + 之后真发生时钟回拨"两件事叠在一起才疼；而 P1-8 当初就被判成"便宜缓解 + 写明残留"（第八节），这条属于同一句残留的具体形状，记进 THREAT-MODEL 更合适，不该为它引入 `updated_by` 改写。
- "手机 `done_day`/`isoToDay` 会红屏" —— agent 自己查到 PC 侧 `set_week1` 先过 `iso_to_day` 才落库，非 ISO 到不了手机。

### 批次 8 的真窗口那一腿（release 二进制 + 一次性数据目录）

`npx tauri build --no-bundle` 之后带 `LOCALAPPDATA=Temp/anvil-rel9` 起 `target/release/anvil.exe`，从页面里用 `__TAURI_INTERNALS__.invoke('plugin:event|emit', …)` 模拟"托盘/右键那一次点击"（原生菜单 CDP 点不到）：

| 要证的 | 结果 |
| --- | --- |
| 三处启动路径改动没把开窗口弄坏 | 冷启动首屏 `header h1` = 今日驾驶舱、`h1` 只有一个、没有 bootError 那行提示、备份卡显示「未配置」—— 他的真实库与云端没被碰 |
| **N6 那条通道真接通了** | 发 `menu://click('settings')` → h1 变「设置」、侧栏「设置」高亮、设置页整屏渲染（`shots/m14-tray-settings.png`）。这是「托盘设置只唤窗不切页」这个从 v0.1.0 就在的行为，第一次被看到是好的 |
| `copy-selection` 的分支 | 无选区时报「没有选中内容」；未知 id 不动视图也不新增提示。**没证到**：有选区时 `execCommand('copy')` 真把字送进剪贴板（我造的那个选区是空的），以及真用鼠标点原生菜单 |
| 快捷键新缓存 | 设置页显示 `Alt + Shift + A`，注册路径跑过。**没证到**：同步卡住期间按它会不会冻 —— 那正是 N10 要修的场景，要断网 + 真按键 |

一条没归因干净的读数：第一次探针在启动约四分钟后读到 `h1` = 「进行中」，冷启动复测复现不了，而代码里没有任何启动路径会选 progress（全仓只有 `mock.ts` 与侧栏项提到它）。记在这里，不当结论也不当作已排除。

### 批次 8 门禁（按 CI 口径）

`cargo clippy --all-targets -- -D warnings` exit 0 零告警；`cargo test` **236 通过 / 0 失败 / 14 ignored**；`node tools/m4-check.mjs`（真桩还原 e2e）**3/3**；`node tools/m11b-check.mjs` **7/7**；`npm run typecheck` 干净；`npm test` **101**（16 个文件）；`npm run contract` **91 命令 + 4 事件双向对上**；`npm run lint:style` 通过（86 文件 8 规则）；`npm run version:check` 0.1.6 四处一致；`flutter test` **98**（写这段时还没动手机）。变异验证 5 次（N5/N6/N8/N9/N11 各一次），每次都如期变红，改回后全绿。

**手机那一腿跑完之后**（见下小节）：`flutter analyze` 干净、`flutter test` **111**（新增 `undo_target_test.dart` / `timetable_shape_test.dart` / `boot_failure_test.dart` / `write_failure_test.dart` / `error_surfaces_test.dart` 共 13 条），`flutter build apk --debug` 通过并装到 MuMu 上真点过。

### 批次 8 的手机那一腿（MuMu 模拟器：真点击 + 直接读库当断言）

模拟器开着，所以这一腿从「读代码觉得对」升级成「按过、并且库里查过」。两条新缺陷都在这里现形：

| 编号 | 症状 | 落点 | 真机证据（同一条操作序列，两个包各跑一遍） |
| --- | --- | --- | --- |
| N13 | 撤销条找回「我刚勾的那一条」按的是**标题**（`today_screen.dart:117-121`）：同名两条时它改的是列表里排前面的那条，而我勾掉的那条留在已完成 —— 两次写都带 dirty=1 传出去 | `_armUndo` 记 id、target 按 id 找、条幅仍显示标题；演示档跟着换成 `undoDemoId` | 建两条同名 `water` → 勾 A → 勾 B → 点撤销：旧包库里 `A=0 B=1`（撤错条，复现成功），新包 `A=1 B=0`（对）。中间加跑一次「只勾 B 不点撤销」作对照，确认那一下真落在 B 上（`B=1`），不是坐标打空 |
| N14 | **批次 7 我自己那条修复在真机上看不见**：`sync.state` 坏掉时那句「读不出来」是借 `syncError` 显示的，而设置页那一行前面还挡着一个 `!accountSaved → 「还没填坚果云账号」` 分支；单测只断言到 store 字段，所以它是绿的 | 拆成独立状态 `Store.syncStateUnreadable`，由「上次同步」那一行说（状态归状态、错误归错误），测试跟着改断言 | 用 `run-as … sqlite3` 往库里塞一条 `sync.state = {这不是 JSON` → 旧包设置页仍说「这台手机还没成功同步过一次」（`shots/m14-phone-settings-badstate.png`），新包说「上次同步的状态读不出来（不是还没同步过）…」（`shots/m14-phone-settings-fixed.png`），而「立即同步」那行的「去填」指引没被挤掉 |
| N15 | 课已经同步进来了、只是**开学日没设**时，今天屏说「这台手机还没同步到课表」，而课表屏同一时刻说「电脑上的开学日还没设」—— 两屏互相打脸，前者教的动作（去点同步）点了也不会变。根因是 `todayShape()` 把两种情况都塞进同一个 `noTimetable` | 记录里加 `needsWeek1`，`GapLine` 用它说与课表屏同一句话 | 往库里塞一行 `classes` + `timetable.term`、不设 `timetable.week1Monday` → 旧包今天屏 `shots/m14-phone-f3-today.png`（那句错话），新包 `shots/m14-phone-f3-fixed.png`（「电脑上的开学日还没设」）。另加 `test/timetable_shape_test.dart` 两条：反向那条（真没课才说「还没同步到课表」）也钉住 |
| N16 | **库打不开时屏幕上只有启动图标**：`main()` 里那条 `await openAppDatabase()` 一抛，`runApp` 从没执行 —— 表现是"点开没反应"，原因只在 logcat。这种坏法最容易把人推向卸载重装，而卸载会连数据一起删 | `main()` 的启动段包 try/catch，失败时 `runApp(BootFailureApp(...))`：先说「先别卸载重装」，再给常见原因，最后原样带上那句 `DatabaseException` | 用 `run-as … chmod 444 databases/anvil.db` 真造出来 → 旧包 `shots/m14-phone-readonly-db.png`（只有图标），新包 `shots/m14-phone-bootfailure.png`（那一屏话说全了）；`chmod 644` 之后重开恢复正常（`shots/m14-phone-recovered.png`），确认不是我把设备弄坏了。`test/boot_failure_test.dart` 两条，含横屏窄高不溢出 |
| N17 | **本机写失败全程无声**（agent D 的 F6，我自己读码确认）：`setDone`/`create` 是先改内存 + `notifyListeners()`、再 `await repo.save…` 且**没有 catch**，调用方还 `unawaited`。真机造出来之后看到的画面是：界面显示「已完成 2」、弹着撤销条，库里那条仍是 0，重开就"凭空丢了"，异常只在 logcat | 两处写各包 try：失败**回滚内存**、把原因写进新加的 `Store.localWriteError`（今天屏那行状态位，红色），下一次写成功自动消掉；`_brief` 只留 `DatabaseException(...)` 那句，不把整条 SQL 与行 id 摊在界面上 | 注入手法是 `CREATE TRIGGER ... BEFORE UPDATE ON todos BEGIN SELECT RAISE(ABORT,'注入：写不进去'); END;`（拆掉即恢复）。三张图：旧包 `shots/m14-phone-f6-ui.png`（说谎的界面）→ 新包 `shots/m14-phone-f6-brief.png`（红字一条、格子没勾、撤销条也没弹）→ 拆触发器重试 `shots/m14-phone-f6-healed.png`（写成功、红字自消）。`test/write_failure_test.dart` 两条 + 一次变异（把回滚那行删掉 → 报 `Expected: <false> Actual: <true>`） |
| N18 | 今天屏那颗同步 chip 在**试过但一次没成**时说「还没同步过」—— 库里明明存着 `lastError`（真机填了错凭据之后拍到的就是这一屏）。`SyncLine` 原本「四态四句」少了一态：`(lastSyncAt=null, lastError≠null)` 无处可去，就被并进「没试过」 | 加一态 `NeverSynced() when syncError != null` → 说「上次同步没成 · 原因在设置页」；注释改成五态五句 | `shots/m14-phone-f18-chip.png`（错）→ `shots/m14-phone-f18-fixed.png`（对），用的是**真发出去的请求**被坚果云拒掉的返回；测试两条（正反） |
| N19 | 药箱**读挂了**与**没药/还没缓存**在主屏长得一样：`MedicineCard` 在 `view == null || isEmpty || plan == null` 时整卡不画，那句 `medicineError` 只有设置页一小格看得见（agent D 的 F7，我复核成立） | 无缓存**且有错误**时画一行矮卡说「药箱这次没读到：<原因>」；真没配/没药仍然不画，不制造噪音 | 同一次真机 401 那一屏（`shots/m14-phone-f18-fixed.png` 上那行红字）；测试两条（正反） |
| N20 | **只改邮箱、密码留空 = 当场换掉加密口令**（agent D 的 F2，读码确认）：`saveAccount` 无条件写新邮箱、密码留空则沿用 Keystore 那条，而信封口令是 `邮箱:应用密码` 派生 —— 改一个拼错的邮箱会让新旧文件**双向互相读不懂**，界面上却只是一次普通保存，唯一线索要等到同步时变成一串「那份读不懂」 | 判据抽成纯函数 `changesTheKey`，命中时先弹一次确认（说清「旧文件会互相读不懂，要改邮箱请连密码一起重填」） | 纯函数 5 条断言（只有「有旧邮箱 + 邮箱变了 + 密码留空」这一种该问）。确认框那一跳没在真机上走（要第二次填账号），留给用户点 |


顺带两条回归：冷启动不崩、空态两句话照旧（`shots/m14-phone-coldstart.png`）；勾掉一条 → `am force-stop` → 重开，库里仍是 `2 条待办 / 1 条完成`。

**N14 为什么只有真机能抓**：单元测验的是「store 里那句话存在」，界面用的是「这一行今天归谁显示」，中间隔着两个文件和一个不相干的分支。这段我静态读过两遍没起疑，是第一张截图里看见「还没成功同步过一次」才发现的。

这一腿的操作配方（下次照抄，别再摸索）：

```
ADB=C:/Users/hp/AppData/Local/Android/Sdk/platform-tools/adb.exe   # 不在 PATH 上
"$ADB" connect 127.0.0.1:7555      # MuMu；16384 / 5555 也通，是同一台的多个 transport
"$ADB" -s emulator-5554 install -r mobile/build/app/outputs/flutter-apk/app-debug.apk
"$ADB" -s emulator-5554 shell pm clear dev.rewmington.anvil_mobile   # 要确定性起点
"$ADB" -s emulator-5554 shell am start -n dev.rewmington.anvil_mobile/.MainActivity
# 断言直接读库（Android 15 镜像自带 /system/bin/sqlite3）：
"$ADB" -s emulator-5554 shell "run-as dev.rewmington.anvil_mobile sqlite3 databases/anvil.db 'SELECT ...;'"
```

- MuMu 这台是**横屏 1920×1080**（`settings put system user_rotation 1` 无效），逻辑高度只有约 411dp，列表得先 `input swipe` 往上推才露得全。360×800 那一档的版式证据仍然只走 Windows shot harness，别拿这张横屏图当设计稿验收。
- `uiautomator dump` 对 Flutter 是**空的**（语义树没开），坐标只能从截图量：输入条中心 ≈(860,826)、发送 ≈(1824,826)、行首勾选框 ≈(88,444)、撤销条右侧「撤销」≈(1790,626)。
- `input text` 只吃 ASCII，所以造同名两条用的是 `water` —— 这条缺陷与标题语言无关。
- 撤销条只活 4 秒：截图看完再点必然超时。要么把整串动作连打成一条命令，要么先跑一次拿坐标再重跑。
- 我在模拟器里 `pm clear` 过两次，现在留着两条 `water` 测试待办；注入过的坏 `sync.state` 已删。

### 更新后的"仍未做"清单（批次 8 版，都要你点头）

1–7 还是第十一节那七条（sqflite_sqlcipher、R8/minify、`types.ts` codegen、P1-9 步骤 3、手机 `device.id` 拒绝启动的口径、凭据管理器那三条真机验、课表错误行只有 jsdom 证据）。新增：

8. **命令面板的焦点陷阱 + Esc 上移**（真，未修）。
9. **「退出」前冲刷防抖的那条跨语言协议**（我选择先删假代码，不新增机制）。
10. **`MAX_PEERS` 界面上点名被裁掉的对端**（日志已经会点名）/ **按 mtime 取最新 8 个**（要 PROPFIND 多解析一列）。
11. **`mock.ts` 逐条镜像真实现，还是降级为"仅演示"** —— 方向由你定。
12. **手机侧那 8 条候选：复核了 5 条**，全部确认成立并已修 —— F4 撤销按标题找（N13）、F3 两屏两句话（N15）、F6 本机写失败无声（N17）、F7 药箱错误与空态同形（N19）、F2 只改邮箱换掉加密口令（N20）。真机自己又长出 3 条清单上没有的：批次 7 那句「读不出来」在界面上看不见（N14）、库打不开时屏幕上只有启动图标（N16）、同步失败被 chip 说成「还没同步过」（N18）。**剩下 3 条仍是二手**：F1 课表不比版本戳（怀疑是把有意设计当缺陷）、F5 热恢复不重新同步（真：`lib/` 里没有任何 `WidgetsBindingObserver`，但"回前台就同步"要拿坚果云配额换，得你定频率）、F8 `syncNow` 后 `reload` 不在 try 内。
13. **两条只能你手工点的**：真用鼠标点一次托盘「设置」与右键「复制选中内容」（我是从页面里 emit 模拟的，原生菜单 CDP 点不到）；以及在有选区的情况下复制一次，确认 `execCommand` 真把字送进了剪贴板。快捷键那条同理：同步卡住时按一次 Alt+Shift+A，看窗口是不是还起得来。

---

## 十三、批次 9（2026-11-02 晚落地）：把口径拍完，剩下的全部改完

用户口径：**「所有问题都修改完再发布」**。那五条要他点头的，他的答复是：① sqlcipher **搁置**；② 回前台同步选 **C**（不加自动同步，改成界面上说清口径）；③ `mock.ts` 选 **A**（逐条镜像真实现）；④ 对端文件选 **B + C**（既加显式清理入口，也改成按时间取最新 8 个）；⑤ 三条分支里速达选 **(a) 加确认**、`local_key` **按我说的维持只报不修**、手机 `device.id` **由我定**。

### 五条拍板各自落成了什么

| 编号 | 拍板 | 落点 | 证据 |
| --- | --- | --- | --- |
| P0-4 | 本地库不加密 = **搁置**，理由写进文档 | `mobile/README.md`「已知限制」一条 + `docs/THREAT-MODEL.md` 那一节从「唯一一条还没定的」改成「已经定过的一条口径」（含四条理由与"什么时候才重新考虑"） | 纯文档；A/B 两半（关备份、`credentials.read` 包 try）批次 8 前就已完成并在真机验过 |
| F5 | 选 **C**：不新增自动同步，把口径说出口 | ① `SyncLine` / 空态 / 设置页 / 药箱那两行全部改走 `agoLabel`；② `agoLabel` 补齐成与桌面 `relTime` 同形状（原先一小时后退回 `clockTime`，**昨天 09:12 与今天 09:12 长得一样**）；③ 「立即同步」那一行从「打开 app 时也会自动同步一次」改成「只在打开 app 时同步这一次；平时放着不会自己更新，要新数据点我」 | `test/reltime_test.dart` 4 条 + chip 那条 widget 测试；**MuMu 真机**：注入 `lastSyncAt = 3 天前` → 今天屏「上次同步于 3 天前 · 0 条待上传」、设置页「3 天前」（`shots/m15-phone-3days.png`、`shots/m15-phone-settings-rel.png`）。从前这两处都写着一个不带日期的钟点 |
| F8 | 判**真**并修 | `store.dart` 的 `syncNow`：`run()` 与 `reload()` 各自包 catch，两种坏法各说一句（「这一轮同步没跑完」/「同步的结果没读回本机（界面还是旧的）」），**整条不再往外抛** —— 两个调用点都不 catch，从前它一抛只进 logcat | `test/sync_now_failure_test.dart` 3 条；**变异验证**：撤掉 reload 的 catch → 两条如期红（`run()` 那条也红，因为 Keystore 坏时 reload 一起坏），改回全绿 |
| ④c | 对端窗口按**最后写入时间**取，不再按 listing 顺序 | `propfind.rs` 解析 `getlastmodified`（`Entry.modified: Option<i64>`）+ `relay.rs::peer_window`（先摘掉自己那份，再按 mtime 降序，**不知道时间的排最后**）+ 桩服务器带上这一列 | `propfind.rs` 3 条新测试（含"没给属性 ≠ 0"与"读不懂的日期不当成最旧"）、`relay.rs` 4 条窗口测试；e2e 断言真服务器上 `modified.is_some()`（`m11b-check` 8/8） |
| ④b | 加**显式**清理入口 | `relay::clean_unreadable` + `Remote::delete`（404 算已删）+ 命令 `sync_clean_peers` + `useSync.cleanPeers` + 备份页那颗两步确认按钮。**没有任何自动删除路径**。判据是「这一轮真读不懂 ∩ 远端此刻真有 ∩ **删之前重新验一遍**」：只有**能解开、内容却读不懂**的才算可证明的垃圾；**解不开的不删**（"解不开"最常见的原因是那台设备用的是另一组凭据 = 它自己唯一的云端历史），现在读得懂的不删（上一轮结论过期），读不到的不删 —— 跳过的每一份都带理由回到界面上。自己那份永不删；删成之后同步状态里那几台会被摘掉 | `clean_targets` 单测 5 条 + **变异验证**（把判据放宽到"listing 里所有" → 如期红）；e2e `cleanup_deletes_only_the_provable_junk_and_says_why_for_the_rest`（真桩上放两份不同的坏文件：一份连信封都不是 → 必须**没删**并给出理由，一份能解开但内容不是快照 → 删；消失的只有后者）；**真窗口**：备份页那颗按钮渲染出来了，第一下只亮，第二下真发命令，凭据没配时**报的是那句可执行的话，界面没有谎称"删掉了"** |

### 批次 8 判真但没动手的那几条，这次都改完了

| 条目 | 修法 | 证据 |
| --- | --- | --- |
| 命令面板焦点陷阱 + Esc 上移 | `@keydown` 从 `<input>` 上移到 `.pal` 容器；`trapTab` 首尾环绕；关闭时归还焦点；焦点在列表按钮上时 Enter **不抢**按钮自己的默认行为 | `src/components/palette/palette-focus.spec.ts` 5 条 + **变异验证**（把 handler 挪回 input、删掉 Tab 分支 → 那两条如期红）+ **真窗口**：19 个可聚焦元素、Tab/Shift+Tab 都绕回、焦点在按钮上时 Esc 关得掉、关掉后焦点回到顶栏搜索按钮 |
| `mock.ts` 逐条镜像（选 A） | 空标题建待办/改名都拒；palette **每类各 8 条**且各自有顺序；药箱目录禁令改成**分段相等 + 大小写无关 + 百分号还原**；补买文案补齐「≤0 = 已用完」与一位小数；便签/倒计时删除改成墓碑 | `src/api/mock-divergence.spec.ts` 12 条（**直接 import `mockApi`**，绕开 `vi.mock('../api')` 那条"测试根本不经过 mock"的盲区）+ **变异验证**（`PER_GROUP` 8→99 → 封顶那条红）。唯一故意没镜像的（速达删除仍是物理移除）写在文件头，理由：`Resource` 类型里没有 `deletedAt`，界面上两种删除完全等价 |
| P1-5 速达删除加确认（选 a） | 前端早就实现了两步内联确认（`Tile.vue`），但**一条测试都没有** —— 这次补 5 条，并顺手把那个 5 秒定时器改成卸载时撤掉 | `src/components/launch/tile-confirm.spec.ts`（第一下不删、第二下只删一次、过期后回到第一步、armed 时常驻可见、卸载撤表） |
| P2-8 本机密钥坏掉（选"只报不修"） | 报错文案改成可操作：指名「凭据管理器 → Windows 凭据 → 删掉 Anvil / local-key」，并先说清代价（那几条存成密文的速达备注会读不回来，其余照常） | 纯文案；波及面批次 2 已核过（`secrets::decrypt` 只在有密文的行上被调） |
| 手机 `device.id`（我定：维持拒绝启动） | 口径不变（换了 id 等于本机每行 `updated_by` 变成别人写的），但**那一屏现在说得出这是哪种坏法**：身份读不回来时不再列"磁盘满了"那套无关清单，改成"为什么宁可起不来 + 两条出路" | `boot_failure_test.dart` 新增 2 条（含横屏窄高不溢出） |

### 真机/真窗口这一腿自己长出来的新缺陷

| 编号 | 症状 | 落点 | 证据 |
| --- | --- | --- | --- |
| **N21** | **桌面启动失败时屏幕上什么都没有**：release 是 `windows_subsystem = "windows"`，`startup_failure` 只有 `eprintln!`（stderr 无处可去）+ 日志文件 + 非零退出码。双击启动的表现就是"点开没反应" —— 与手机端批次 8 的 N16 同一件事，而这条从前写在注释里当"只能这样" | `logger.rs::startup_failure` 补一个原生消息框（`MessageBoxW`，`windows` crate 与 `Win32_UI_WindowsAndMessaging` 本来就在依赖里，**没引新依赖**）：说原因、说「先别卸载重装」、给出日志路径 | **真窗口**：一次性数据目录里 `DROP TABLE classes` → 修之前进程 <1 秒退出发声；修之后进程**停在对话框上**（12 秒后 `tasklist` 里还在，日志同一句 `STARTUP-FAIL`）。<br>顺带一条读数：这条坏法是**启动即拒**而不是"带着坏库跑着"，所以 `classes` 缺失不会静默显示成"今天没课" —— 这一点是有意的，别去"修"成自动建表（那才是把故障演成缺席） |

### 批次 9 门禁（按 CI 口径，最后一次编辑之后重跑）

`cargo clippy --all-targets -- -D warnings` exit 0 零告警；`cargo test` **245 通过 / 0 失败 / 15 ignored**（批次 8 是 236/0/14）；`node tools/m4-check.mjs` **3/3**；`node tools/m11b-check.mjs` **8/8**（新增那条清理 e2e）；`npm run typecheck` 干净；`npm test` **126**（20 个文件，批次 8 是 101/16）；`npm run contract` **92 命令 + 4 事件双向对上**（新增 `sync_clean_peers`）；`npm run lint:style` 通过（90 文件 8 规则）；`npm run version:check` 0.1.6 四处一致；`npm run build` 绿。
手机：`flutter analyze` No issues；`flutter test` **122**（批次 8 是 111）；`flutter build apk --debug` 通过并装在 MuMu 上真点过。
release 二进制 `target/release/anvil.exe`（20:03）含本批全部桌面改动。

**变异验证一共 6 次**（F8 的 reload catch、`clean_targets` 判据、palette 的 `PER_GROUP`、命令面板 handler 位置 + Tab 分支、以及批次 8 遗留那次），每次都如期变红、改回后全绿。

### MuMu 这一腿的账（要说破）

- 拍到的：`agoLabel` 那条（3 天前 / 设置页那一行）是真机读数，不是 jsdom。
- **没拍到的**：N20「只改邮箱会换掉加密口令」那个确认框**仍然没在真机上点过** —— 要走那一跳必须第二次填账号，而填完会自动发一次真请求到坚果云。我改用另一条路（先把模拟器断网再填），结果 **MuMu 的网络被 `svc wifi disable` 连带把 adb 那条 TCP 通道也断了**，之后 `adb kill-server` / `reconnect` 都回不来，这一腿就停在这里。**模拟器需要你在 MuMu 里重启一次**（它上面只有我造的测试数据，`pm clear` 过多次，没有你的东西）。
- 因此那两条仍然只有单测证据：N20 的确认框（纯函数 5 条断言在，那一跳没点）、设置页「只在打开 app 时同步…」那句（widget 测试在，360 宽不溢出也测了，但没在模拟器上看见）。

### 更新后的"仍未做"清单（批次 9 版）

原来 13 条里，**8/10/11/12 与 P0-4 全部做完或拍板**。剩下这些，都不是"我发现不了"而是"要你的鼠标 / 要你决定成本"：

1. **P0-4 的 C 方案（sqlcipher）**：已拍板搁置，理由进档。要重开时先答"迁移中途掉电怎么办"和"恢复后读不了怎么办"。
2. ~~**凭据管理器那三条**~~ **已由用户在真窗口确认**（2026-11-02 晚）：托盘「设置」切页、有选区时复制、同步在跑时按 Alt+Shift+A 三条他点过说都没问题了。凭据管理器那三条仍只有静态判读 + 单测（我没在他机器上建删过任何凭据条目）。
3. ~~**托盘与右键的真鼠标**~~ 同上，用户真窗口确认过。`menu://click` 的前端分支此前我用页面内 emit 验过，现在是原生那一下也验过了。
4. ~~**同步卡住时按 Alt+Shift+A**~~ 用户确认窗口起得来。
5. **「退出」前冲刷防抖的跨语言协议**：假代码删了，真实暴露面（打字那一格 ≤600ms）写在原位。要做协议版你说一声。
6. **`MAX_PEERS` 的界面点名**：日志与状态行现在会说"这一轮只读到最近写的 8 份"，但被裁掉的具体是哪几份仍只在日志里（要加字段就得动 `sync.state` / `types.ts` / mock / 手机同步改，这一轮只做完了"按时间取"那一半）。
7. **`types.ts` 手抄 663 行要不要 codegen**、**R8/minify 要不要开**：两条债，没动。
8. **N20 确认框 + 设置页那句新文案**：见上一小节，模拟器要重启。
9. **真机（实体手机）与真实坚果云账号的成功往返**：`PLAN-MOBILE §7` 那条验收项，模拟器代替不了。

**批次 10 那七条手工检查已由用户在真窗口逐条确认「都没问题」（2026-11-02 深夜）**：进度页小窗能滚到底；文件夹 `…` 的改名（名字没改按回车什么都不发生）、移到…（候选里没有它自己）、两步删除与那句"几篇退回上一层"；待办标签回车成两颗且切页回来仍在（真落库）；速达换分区 + 写-查看加密备注原样念回；便签清空的第一下待确认与 5 秒自动退回；Ctrl+K「立即备份」面板秒关；以及备份页那句新口径的措辞他认了。他这次是**直接双击 `target/release/anvil.exe` 验的**，用的就是那份真库 —— 所以这些操作确实写进了他的数据，不是演示现场。

**批次 10 之后（2026-11-02 深夜）**：第 5 条改判 —— 见第十五节「退出时同步没跑完就关窗」那小节，机制不加，补的是备份页那句口径（含一条反向断言拦住我写错）。其余各条状态不变：6（被裁掉的对端只在日志里）、7（两条债）、8（MuMu 仍 offline，N20 与手机设置页那句仍未在设备上看过）、9（真机真账号）都还等他或等一次设备重启。

---

## 十四、用户自己用出来的两条（2026-11-02 晚，同一批里最便宜的两条真缺陷）

我把 release 二进制交给他之后，他**正常用了一遍**就报回来两条 —— 都不在任何审计清单上，也不是我四轮盲区扫描能扫出来的形状：

| 编号 | 症状（他的原话） | 根因 | 修法与证据 |
| --- | --- | --- | --- |
| **N22** | 「数据与诊断下面的被截掉了，只能最大化才能看到下面，**小窗滑动没反应**」 | `Settings.vue` 与 `Stats.vue` 的根容器**完全没有滚动容器**：外层 `.y-ctr` 是不滚的 flex 列（`min-height:0` 但没有 `overflow`），内容一高就裁掉。别的屏各自写了 `overflow-y:auto`，这两屏从没写过 —— 所以只有这两屏会这样 | 两屏各加 `flex:1; min-height:0; overflow-y:auto`（不加 `display:flex`/`gap`，那两页的内部间距是靠各块自己的 margin 撑的，改了会连带动版式）。**真窗口量过**：把窗口钉成 1100×640 → 设置页 `根容器可滚:true`、滚到底后最后一块（「数据与诊断…」）底边 624 ≤ 视口 640 ✓ |
| **N23** | 「速达**创建分区了不能删除**」 | `zone_delete` 命令、`api.zoneDelete`、mock 三处全在，**唯独界面没有入口**。契约检查（命令 ↔ 调用点）看不见这一类，因为它比的是 `real.ts` 那一层 —— **有命令没入口**和有能力没命令是同一类账，而且更难发现 | 分区胶囊右侧挂一个 ×（只对真分区，「全部」「未分区」没有），两步内联确认与速达磁贴同一个 5 秒节奏，Esc 可撤回，删完若正看着这个分区就回「全部」，卸载撤定时器。删分区**不动里面的速达**（后端把 `zone_id` 置空 → 退回「未分区」），所以文案敢直接写「里面的速达不会丢」。`src/views/launcher-zones.spec.ts` 5 条钉住；**真窗口整条走过**：建「探针分区」→ × 出现（平时 `opacity:0`）→ 第一下 armed + 提示语对 → 第二下删掉，胶囊从 3 回到 2 |

**顺手做的同类普查**（N22 的形状不可能只有一处）：逐屏看根容器有没有滚动能力 —— 速达/待办/日历/驾驶舱/笔记/课表**都有内层滚动区**（那是设计，不是漏），只有设置与统计两屏内外皆无。真窗口在 1100×640 下量了 7 屏，其余 6 屏当时内容都装得下（**注意**：那台一次性库是空的，所以"装得下"对日历/进行中只是弱证据 —— 数据多的时候会不会顶出去，要他真用几天才说得出）。

**这一节的方法论账**：四轮并发只读审计 + 我自己复读 + 真机真窗口，加起来扫了 71 条，仍然漏掉这两条 —— 它们的共同点是**只在"人真的在用"时才出现**（一个要小窗口，一个要先建分区再想删）。以后每批改完，把我自己不会做、但用户会做的动作列出来交给他，比再开一路 agent 便宜得多。

---

## 十五、批次 10（2026-11-02 深夜落地）：把"有命令没入口"这一类扫到第二层，顺手抓到自己写的一条新缺陷

起因是他那句「所有问题都修改完再发布」。上一轮我只扫了 `real.ts` 那一层（Api 方法有没有被调用），报了 8 条；这一轮把扫描面往下推一层 —— **composable 的导出有没有被模板用到** —— 立刻发现我上一轮的数字是错的。

### 先记一条自我翻案

上一版说「`paths()` / `deviceId()` 是**两个重复入口**」。**错**：这两个 Api 方法在界面上**一个调用点都没有**，设置页那份路径与设备号是从 `settingsAll()` 一次读回来的（`Settings.vue:55`）。所以它们是**死方法**，不是重复。真正重复的是 Rust 侧：`paths` / `device_id` 这两条命令与 `settings_all` 的返回体完全重叠。按「冗余优先合并」的口径，四条命令连同 Api 封装一起删掉，而不是留着当"诊断入口"。

| 编号 | 判定 | 是什么 | 为什么他从来没发现 | 处理 |
| --- | --- | --- | --- | --- |
| **N24** | 真 | 进度页（「进行中」）根容器**没有滚动契约** | 与 N22 同族，但成因不是"忘了写"而是**写了 9 遍**：`.y-view` 这个类**在 CSS 里根本没有全局定义**，每个视图各自在自己的 scoped 样式里抄一遍 `flex:1; min-height:0; overflow-y:auto` —— 11 个视图里进度页一条都没抄 | 把契约收进 `src/style.css` 的**一条**全局 `.y-view`，删掉 9 份局部副本里重复的那三行；课表页保留自己的 `overflow:hidden`（它滚的是表格区，根上再开会出两条滚动条）。`tools/lint-style.mjs` 新增两条机械拦：视图根没挂 `.y-view` 要报，把三行抄回局部也要报（两个方向都做变异验证，见下） |
| **N25** | 真 | 待办打标签 `todos_set_tags` 没有入口 | 标签在**笔记**页能加能删，待办只能显示几个芯片；他自然以为"待办没标签这回事" | 详情面板的「清单」一行改成可编辑（与 `Notes.vue` 那颗 × 与输入框同一形状、同一语义：整组替换、逗号/空格分隔、去重） |
| **N26** | 真 | 速达条目换分区 `resource_move` 没有入口 | 分区可以在速达页建、可以删，唯独**条目进不了分区**（只能靠拖进来时那一刀） | 详情面板加「分区」一行候选芯片，点谁是谁；点自己那一颗**不写库**（免得白涨一条 rev） |
| **N27** | 真 | 写/清除速达加密备注 `resource_set_secret` 没有入口 | 界面上只有「查看加密备注」。他以为备注是别的软件写进来的只读东西 | 详情面板加编辑器（留空保存 = 清除，与后端 `trim().is_empty()` 同口径）；`web` 类条目不给口子，与「查看」同一条件 |
| **N28** | 真（一次三条命令） | 笔记**文件夹**改名 / 移到 / 删除三条命令全没有入口 | 侧栏那棵树只给「新建」留了输入框。他建错一个名字就只能重新建一个、旧的留着 | 每个文件夹行加一颗 `…`（平时不占视线，悬停/聚焦才现），展开成 改名 / 移到… / 删除；删除走两步内联确认并回报「N 篇退回上一层、子目录提到上一层」（`note_folders::delete` 本来就是搬东西不删内容） |
| **N29** | 真 | 便签槽位清空 `sticky_clear` 没有入口 | 只能把字手动删光 —— 那只是存了个空串，记录还在 | 「进行中」页每个槽位加一颗清空，两步确认、5 秒自动解除、同时只可能有一颗在待确认 |
| **N30** | 真（**我自己这一轮写出来的**） | 详情面板读速达时用 `Promise.all(resourcesList, zonesList)`，于是 `zones_list` 一坏整块详情**空白** | 只有真跑才露：我给一次性库塞分区行时把 `sort`（REAL 列）写成了字符串，真后端当场报 `Invalid column type` —— 详情面板于是什么都不显示，而报错只在日志里 | 两条读**分开等**：条目本身照旧显示，分区清单失败就在「分区」那一行写「分区清单读不出来：…」，并留着「未分区」那颗（挪出分区不依赖清单）。`detail-actions.spec.ts` 钉住这条降级 |
| **N31** | 真但降级 | 速达页 `zonesList` 失败会**静默**变成"只有全部/未分区两颗"（既有代码，不是我引入） | 同 N30 的成因，只是它不崩、只是少几颗胶囊 | `Launcher.vue` 的 `reload` 接住错误写进那一屏已有的 `.err` 行，`shell.setCount` 照常走；`launcher-zones.spec.ts` 补一条 |
| **N32** | 真 | 「立即备份」「测试连接」在 composable 里**没有重入保护**：备份页那棵按钮的 `:disabled="busy.run"` 只拦得住按钮 | 命令面板（Ctrl+K → 立即备份）调的是同一个函数，不经过按钮 —— 连按两下就排起两份并发上传，同一天的键写两遍，白烧一倍配额 | `useBackup.runNow/testConnection` 进来先判 `busy`；`useBackup.spec.ts` 用手可控 promise 钉「第二次不打命令」+「跑完还能再跑」 |
| **N33** | 真 | 命令面板执行动作时 `await row.run()` 再关面板：备份那一条要压着这层模态几十秒 | 底下的备份页正是唯一会写出结果与流水的那一页，被面板盖住了 | 改成先关再踢（`void row.run()`）；`palette-dismiss.spec.ts` 两条：普通点一下立刻关、Shift 那一路仍留面板 |
| **N34** | 真 | **库比程序新时迁移静默继续跑**：`apply` 用 `skip(from)`，`from > len` 就是空循环，程序带着看不懂的库启动 | 只有"装了旧版本"或"从旧备份整个换回 app.db"才触发，而他机器上不会自己发生 | `migrate` 先判 `current > MIGRATIONS.len()` → 拒绝启动（那句话正好由批次 9 加的启动弹窗 `logger::startup_failure` 念出来，还带日志路径）。另补一条**数据**测试：只跑到 0001、插两行真数据、再升到最新，断言行没丢、`completed`/`dirty` 回填对，并把升上来的库与"一次建好"的库**逐张对象对比形状** |
| **N35** | 半真 | `useActions.ts` 头部写「**任何能力**都必须在这里有一个 id，命令面板天然覆盖全部功能」，`system.rs` 写「命令面板要能直达每一项能力」 | 实际：18 条动作里全是跳转/新建/外观/系统，四条破坏性确认（删磁贴、删分区、彻底删笔记、清对端信封）**一条都不在**，托盘那几项（退出/重载/设置/复制）也不在 | 判定半真：断言的本意（别漏掉能一键做的）是对的，写成"任何/每一项"就成了假话，而且会被当覆盖率分母用。**破坏性动作不进面板是有意设计**（键盘列表里"确认"会退化成"再按一次回车"）→ 只把边界就近写清，不加动作 |
| **N36** | 真（12 处） | 文档口径过期：README 说驾驶舱有「课表空档/倒计时/便签/速达/系统资源」（那些在待办/进行中/侧栏）、说 `workbench-sync` 是「待办/课表**对等**同步」（课表单向，`PC_ACCEPTS = ["todos"]`）、`git tag v0.1.4` 停在旧版本号、状态段写 M0–M10；PLAN-MOBILE 说手机「不新增密码条目 / 只用一个键」（实际 `syncKey` + `medicineKey` 两条）、说凭据条目叫 `Anvil/webdav`（桌面是 keyring 的服务 `Anvil` + 账号 `webdav`，两边拼法不同）；PLAN 说「备份页那张会上传/不会上传表」（在设置页，`Settings.vue:130`）；mobile/README 写 75 条测试（实测 122 条 / 20 个文件）；`useWidgets.ts` 注释写「系统资源每 1 秒」（侧栏 `SAMPLE = 5_000`） | 文档不参与编译，CI 一层都不响 | 逐条对着代码改。两处**故意不改**：`PROGRESS.md:633` 与 `PLAN-MOBILE.md:198` 里的「48 条」是**那一天**的流水，改它等于伪造历史 |

### 台账里那条「退出时同步没跑完就关窗」——改判为不修（不是遗漏）

第十三节留到最后的那条真缺陷，读到底之后**不是数据问题**：`relay::run_with` 的顺序是 PUT → `clear_dirty`（`relay.rs:656-661`），而 `record()` 在整轮成功之后才写（`relay.rs:698`）；PC 那一侧 `should_publish = pending > 0 || local_rows`，本地有数据就每轮发全量。于是进程中途被杀的三种表现都自愈：没 PUT 成 → dirty 还在，下轮照发；PUT 成了没清 dirty → 下轮把同一份再发一遍；`lastSyncAt` 不会被写成一次没发生的成功。SQLite 那边 WAL 自己兜崩溃。

**真缺的是那句话没人说**，所以补的是文案不是机制：备份页「手机同步」那块现在写着「开机后 60 秒会自动同步这一次，之后没有定时器：新改动要等下一次点「立即同步」，或者明天开机。中途退出不会丢：这一轮没发完的改动，下一轮照常带上。」`backup-sync-wording.spec.ts` 3 条钉住，其中一条是**反向断言**：不许写成「这台电脑不会自己定时同步」—— 桌面确实有 `sync/schedule.rs` 那一次开机同步，我第一稿就写错过，被这条断言拦下来。

顺手补的两处口径：`schedule.rs` 顶上那句「失败只在设置页留一行状态」→ 实际渲染在备份页那一块；`README.md` 状态段「桌面端 M0–M10」→ M0–M11b，并写明同步时机。

### 这一轮的取证

**真窗口（一次性数据目录 + dev 实例，不碰他的库）**

- 12 个屏逐个在 1100×360 量过：修前把进度页的 `.y-view` 摘掉 → `overflowY: visible`、`scrollTop` 推不动、最后一个元素底边 474 > 视口 360（**裁掉 114px 且没有滚动条**，与 N22 一模一样）；挂回去 → `auto`、能滚 129、底边 344 ≤ 360。其余 11 屏 `overflowY` 都对，课表是 `hidden`（有意），外层 `document.scrollingElement` 溢出 0（没多出第二条滚动条）。
- 文件夹整条生命周期：建 A/B/C → `…` → 改名成「A 改名」 → 移到（候选 = 顶层/B/C，**没有它自己**）→ 挪进 B 变第二层 → 建 D 挪进 A **出现第三层**（老模板只渲染两层，第三层会凭空消失，这条必须靠真数据才看得见）→ 删 B → 「已删除 · 里面没有笔记，子目录提到了上一层」，A 回顶层、D 跟着变第二层。
- 待办标签：输入「重要, 私人」回车 → 两颗芯片都在；切到统计再切回来仍是两颗（**从库里读回来的**，不是内存态）；点 × 去掉「私人」后剩一颗。
- 便签：打字落盘 → 第一下变「确认清空」且字还在 → 第二下清空并落库，切走再回来仍空、没有残留 armed。
- 速达：命令面板选中探针条目 → 「分区」一行是「未分区←当前 / 探针区」→ 点「探针区」当前标记跟着走 → 写备注（无报错）→ 「查看加密备注」念回原文，库里 `secret_cipher` 不再是 NULL → 挪回未分区，`zone_id` 回 NULL、`rev` 从 2 走到 4。**顺带说明**：取证脚本一开始把左键当选中用了，而磁贴左键 = 启动 —— 那条探针条目因此真启动过一次 `calc.exe`（用后即查：机器上没有残留的计算器窗口）。这本身也是条口径：**磁贴的左键是"跑它"，不是"看它"**。

**变异验证（4 + 2 组，全部先红后绿）**

`tools/contract-check.mjs` 新增第三层「每条命令都要能从界面走到」，实现是跨文件的调用图：视图/组件的顶层代码与模板标识符作种子，同文件按名字连边，跨文件**只经 composable 的导出清单**连边。第一版只连两层就误报了 `sticky_save`（藏在第三层），第二版按名字全局合流又漏了 `todos_set_tags`（笔记页也有个 `commitTags`）—— 两版的失败原因都写进注释。定稿后基线 **88/88 通过、豁免 0 条**，四组变异全拦：整块拆掉文件夹菜单 → 报 `folder_rename/move/delete`；拆掉标签输入与 × → 报 `todos_set_tags`；拆掉分区选择 → 报 `resource_move`；**把 `Notes.vue` 退回 master 那一版**（三个按钮都还没做的时候）→ 三条全报，也就是说这道闸装在当时就能拦住那片盲区。`tools/lint-style.mjs` 那两条也各自验过。

**这一轮的教训（写下来，下次照着做）**

1. **扫"有没有入口"必须扫到模板那一层**。`api.X(` 被 composable 调了一次不算入口 —— 上一轮那 8 条就是这么漏掉了整整一片文件夹管理。
2. **"抄了 N 遍"的契约一定会漏一份**。修 N22 时我只补了漏的两屏；这次把 `.y-view` 收成一处才发现**第三屏（进度页）也是漏的**，而它漏的方式是"用了这个类却一条规则都没写"。合并重复本身就是取证手段。
3. **实施新入口的过程会造新缺陷**（N30/N31 都是我这轮写出来的），所以每一步都要真跑一遍 —— 静态读码时 `Promise.all` 那行看起来完全正常。

### 批次 10 门禁（最后一次编辑之后重跑）

`cargo test` **247 通过 / 0 失败 / 15 ignored**；`cargo clippy --all-targets --all-features -- -D warnings` 零告警；`node tools/m4-check.mjs` 对真桩 **3/3**；`node tools/m11b-check.mjs` 对真桩 **8/8**（这两条跑的是被 `#[ignore]` 的那批 e2e）；`npm test` **174**（27 个文件）；`npm run typecheck` 干净；`npm run contract` 三层全绿 —— **invoke 88 个调用点对上 88 条命令**（删掉四条重复/死命令后从 92 降到 88）、事件名 4↔4、**命令入口 88/88、豁免 0 条**；`npm run lint:style` 97 个文件（8 条规则 + 视图滚动契约两条拦）、令牌冻结表核对通过；`npm run version:check` 四处 0.1.6 一致；`flutter analyze` 零 issue、`flutter test` **122**。

**与上一版台账的差值核对**：Rust 245 → 247（迁移那两条新测试），vitest 132 → 174（+42：详情面板 13、文件夹 10、便签 6、备份口径 3、命令面板关闭 2、mock 镜像 4、`useBackup` 重入 3，其余是各套原有新增），命令 92 → 88（删 `paths` / `device_id` / `notes_get` / `todos_list_undone`）。
