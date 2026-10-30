-- 0005：课表。
--
-- 一行 = 一个「星期 × 节次 × 周次集合」的上课时段，不是一门课。
-- 同一门课常拆成好几行（教务导出就是这样：一个格子里塞两段不同周次），
-- 界面按 name 归组显示，展开"这周有没有它"时按行判断。
--
-- 课**不进 todos**：上完一节课不是"做完一件事"，混进去会把统计页灌满
-- （2026-10-30 定的口径）。所以它自己一张表，待办只能"挂在课上"。
CREATE TABLE classes (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,

  -- 学年学期原样（如 2026-2027-1）：换学期导入时不覆盖旧的那套
  term       TEXT    NOT NULL,
  name       TEXT    NOT NULL,
  teacher    TEXT    NOT NULL DEFAULT '',
  room       TEXT    NOT NULL DEFAULT '',
  -- 1 = 周一 … 7 = 周日，与 days::weekday 同一约定
  day        INTEGER NOT NULL CHECK (day BETWEEN 1 AND 7),
  -- 本地分钟数 0..1439。存分钟而不是存字符串：判断"现在是不是在上课"要能比大小
  start_min  INTEGER NOT NULL,
  end_min    INTEGER NOT NULL,
  -- 节次标签原样（如「第一大节 [01-02]节」），界面直接显示，不自己编号
  period     TEXT    NOT NULL DEFAULT '',
  -- 周次集合的规范化写法："1-10,12-16" 或 "1,3,5,7,9"。展开判断在 Rust 的 weeks_include
  weeks      TEXT    NOT NULL DEFAULT ''
);

-- 按"某学期某星期"取整列，是课表视图唯一的读路径
CREATE INDEX idx_classes_term_day ON classes(term, day) WHERE deleted_at IS NULL;
