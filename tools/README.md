# 取证脚本

这些脚本是「验收自己跑完」用的，不是产品运行时依赖。多数面向 Windows + 本机 Anvil 窗口/库。

| 脚本 | 用途 |
|---|---|
| `shots.mjs` | 断点 × 主题真窗口截图（通过 WebView2 调试端口） |
| `cdp.mjs` | 极简 CDP 客户端，`shots.mjs` 与 `verify.mjs` 共用 |
| `capture.ps1` | 截取 Anvil 窗口，证明只能靠像素看的那几条 |
| `probe-m9c.mjs` | 真后端体检：只读结构计数，不读标题/课程名/教室 |
| `verify.mjs` | 逐图核对聚合 SQL 与列表一致（故意换写法，避免自证） |
| `webdav-stub.mjs` | 本地 WebDAV 桩：Basic 鉴权、命名空间前缀不稳定、href 百分号编码 |
| `m4-check.mjs` / `m11b-check.mjs` | 起桩 → 跑对应的 `#[ignore]` e2e → 核对远端落成了什么 |
| `lint-style.mjs` / `token-check.mjs` | 反网页感静态检查、冻结令牌值断言 |
| `copy-audit.mjs` | 界面文案审计（长句分类，供人判断，不是门禁） |
| `dbq.mjs` | 只读跑聚合 SQL 自证；**不要用它 SELECT \***（不把用户内容搬进对话） |
| `mkicon.mjs` | 把图标源图的白角处理成透明 |
| `then-*.js` / `sendcombo.ps1` | 一次性真交互取证（按真实点击/按键复现某个复现步骤） |

## 约定

- 截图与临时数据写到仓库根的 `shots/` 或 `tools/.webdav-*`，两者都被 `.gitignore` 排除。
- `m4-check` / `m11b-check` 只连本地桩；真实坚果云上传是设置页点「立即同步」的事。
- 涉及本机库的脚本只跑**聚合**，不读原始内容。
