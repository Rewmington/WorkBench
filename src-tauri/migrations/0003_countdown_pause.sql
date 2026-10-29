-- Anvil · 0003 让倒计时的「暂停」真的能暂停
--
-- 0001 给了 `paused` 布尔位，但没有地方存「暂停那一刻还剩多少」。
-- 于是暂停只是不再提醒，ends_at 照旧往前走 —— 25 分钟的番茄钟暂停 10 分钟再
-- 继续，界面会告诉你只剩 15 分钟。那不叫暂停。
--
-- `remaining_ms`：只在 paused = 1 时有值，是冻结住的剩余毫秒；
-- 继续时用它重算 ends_at，然后清回 NULL。跑着的倒计时不该读这一列。

ALTER TABLE countdowns ADD COLUMN remaining_ms INTEGER;
