-- Anvil · 0002 把「当前完成状态」与「最后一次完成时间」拆成两件事
--
-- 0001 只用 `done_at` 一个列同时表达两件事，于是它们互相抵消：
--   PLAN §5.1 要求「取消勾选时不清空 done_at」（否则历史统计少一格），
--   而界面用 `done_at IS NULL` 判断「这条还没做」——
--   结果 set_done(false) 只把 rev 推了一格，勾选框**只能完成、永远取消不了**，
--   而且这条在「待办」和「已完成」两个列表里同时消失/留下（list_undone 过滤 done_at IS NULL）。
--
-- 现在：`completed` 是当前状态（勾选/取消都改它），`done_at` / `done_day`
-- 保持 §5.1 的语义不变 —— 最后一次完成于何时，永不清空。
-- 统计口径统一只看 `completed = 1`，所以「今天完成 N 件」永远等于列表里看到的 N 条。

ALTER TABLE todos ADD COLUMN completed INTEGER NOT NULL DEFAULT 0;
UPDATE todos SET completed = 1 WHERE done_at IS NOT NULL;

-- 活动行的部分索引跟着换谓词，否则「未完成」查询会退回全表扫
DROP INDEX idx_todos_due;
CREATE INDEX idx_todos_due ON todos(due_at) WHERE deleted_at IS NULL AND completed = 0;
