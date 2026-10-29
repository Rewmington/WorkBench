-- Anvil · 0004 提醒要记得住「已经提醒过」
--
-- 0001 有 `remind_at`，但没人读它 —— 提醒时间到了不会有任何事发生。
-- 要让它真的响一次，就得有一列区分「还没到点」「到点且已弹过」「到点还没弹」，
-- 否则后台每 30 秒扫一遍就会每 30 秒弹一条，通知中心变成刷屏。
--
-- `reminded_at`：弹过通知的时刻。NULL = 还没弹过。
-- 改 `remind_at` 时由 todos.rs 清回 NULL —— 用户重设提醒就是想再被提醒一次，
-- 留着旧标记等于新提醒永远不响。

ALTER TABLE todos ADD COLUMN reminded_at INTEGER;

-- 只索引「等着弹」的那一小撮：活动、未完成、设了提醒、还没弹过。
-- 全表索引会在库大了以后白占空间，而这条查询每 30 秒跑一次。
CREATE INDEX idx_todos_remind ON todos(remind_at)
  WHERE deleted_at IS NULL AND completed = 0 AND remind_at IS NOT NULL AND reminded_at IS NULL;
