-- 0007：`todos.created_hour` 的索引。
--
-- 统计页那张「一天里哪些小时在写待办」每次都是 `GROUP BY created_hour` 全表扫
-- （`repos/stats.rs::hour_bars`）。列是 0002 加的整数，一直没配索引 —— 行数上来之后
-- 首屏最慢的就是这一张图（issue #1 的 P2-15）。
--
-- 局部索引按查询的真实条件收窄：`deleted_at IS NULL AND created_hour IS NOT NULL`。
-- 谓词与查询一字不差，SQLite 才会用它；写成全表索引反而可能不被选中。

CREATE INDEX idx_todos_created_hour ON todos(created_hour)
  WHERE deleted_at IS NULL AND created_hour IS NOT NULL;
