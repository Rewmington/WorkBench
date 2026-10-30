-- 0006：`todos.dirty` —— 这台机器上还没传出去的本地写入。
--
-- PLAN-MOBILE §3.4。桌面 §5.1 的同步列里没有它：单写端时代不需要知道
-- "哪些行还没出门"，而路线 A 是各设备覆盖自己的全量快照文件，于是它承担两件事：
--   1. 界面那句「N 条待上传」的唯一计数来源（count(*) where dirty=1）；
--   2. 「本次没有本地写入就跳过 PUT」的省配额判断（PLAN-MOBILE §3.2）。
-- 清它的规则只有一条，写在 `sync::merge::clear_dirty`：比行自己的 `updated_at`，
-- 不比本机 now —— 否则序列化之后、PUT 响应回来之前新写的那条会被悄悄清掉。
--
-- `classes` 不加：手机对课表只读（PLAN-MOBILE §3.1），PC 每次导入都是本地写入，
-- 整表随快照发布即可，不需要"要不要出门"的判断。
--
-- 列默认 0，但**存量行一律置 1**：这些行从没上过云，把它们的 dirty 初始化成 0
-- 就是谎报"已经传出去了"——而 §3.2 那句"没有本地写入就跳过 PUT"会让它们永远留在家里。

ALTER TABLE todos ADD COLUMN dirty INTEGER NOT NULL DEFAULT 0;
UPDATE todos SET dirty = 1;
