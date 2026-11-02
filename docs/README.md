# 文档索引

这些文件是**开发过程中的决策记录**，不是产品说明书。先看根目录 [README](../README.md) 了解项目；下面按阅读顺序排。

## 计划

| 文件 | 管什么 |
|---|---|
| [PLAN.md](PLAN.md) | 桌面端：范围、数据模型、界面定稿、里程碑、已拍板的取舍 |
| [PLAN-MOBILE.md](PLAN-MOBILE.md) | 手机端：路线选择、同步模型、凭据、UI 定稿、里程碑 |

两份计划有交叉但不重复：数据模型与冻结令牌在 `PLAN.md`，手机端同步语义与只读药箱在 `PLAN-MOBILE.md`。

## 过程记录

| 文件 | 管什么 |
|---|---|
| [PROGRESS.md](PROGRESS.md) | 按时间记录做了什么、怎么验的、哪里没做、下一步 |
| [THREAT-MODEL.md](THREAT-MODEL.md) | 这套软件防的是什么、不防什么：每条威胁对应哪道真实防线 |
| [AUDIT-TRIAGE.md](AUDIT-TRIAGE.md) | issue #1 那份外部审计的逐条判定与施工台账（**当前批次进度以这里为准**，不在 PROGRESS 里重复一遍） |

## 截图

| 位置 | 说明 |
|---|---|
| [images/](images/) | README 用的精选截图（演示数据，不含真实个人信息） |
| `../shots/` | 本机取证截图，**被 `.gitignore` 排除**；需要时用 `tools/shots.mjs` 或手机 `ANVIL_SHOT` 重拍 |

## 设计稿

[../design/README.md](../design/README.md) — 各版 HTML 对比稿分别在回答哪个问题，以及哪些决策已经落进代码。

## 当前状态（2026-11-02）

- 桌面：M0–M11b 已落地；最新发布仍是 **v0.1.6**（`master` 上的 `ad7059a`），issue #1 那份外部审计的逐条处理已按主题拆成 11 个本地 commit（批次 0–10），四份清单已抬到 **0.1.7**，**等你点头才 push 与打 tag**。要看"改了什么、还没验什么"就读 [AUDIT-TRIAGE.md](AUDIT-TRIAGE.md)，别读这里。
- 手机：M11a–M11e 已落地（数据层、快照/合并、同步客户端、两屏 UI、药箱只读视图）。
- 未验：实体 Android 设备、真实坚果云账号的成功往返、一周配额实测。
- 两份计划（PLAN / PLAN-MOBILE）与 PROGRESS 是**定稿与流水**，不跟着每批修补改；口径冲突时以代码 + AUDIT-TRIAGE 的判定为准。
