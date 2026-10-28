-- Anvil · 0001 初始结构
-- 依据 docs/PLAN.md §5.1（同步友好约定）与 §5.2（表清单）。
-- devices / sync_cursor 两张表 v1 不建（手机伴侣延到 v2，PLAN §10），
-- 但 rev / created_by / updated_at / updated_by / deleted_at 五列照建 —— 现在建是几行 DDL，
-- 以后补要写迁移并面对"历史来源未知"。
--
-- 同步列豁免（有意为之，不是遗漏）：
--   linked_snapshots —— 外部只读缓存。混进同步列就等于把健康数据（药箱副本）
--                      带进可上云的表；它同时被 M4 从备份产物里剔除。
--   backup_log       —— 每台机器自己的审计流水。
--   settings         —— 每台机器自己的 UX（主题/布局/快捷键/路径），同步了是错的。
--
-- 本地日/时整数列（done_day / created_day / created_hour）：PLAN §7 写的
-- `date(done_at/86400000)` 是 UTC 日界，UTC+8 晚间完成会归到次日；改存写入时算好的
-- 本地 yyyymmdd 整数，聚合与 TZ/DST 无关。

CREATE TABLE todos (
  id           TEXT PRIMARY KEY,
  rev          INTEGER NOT NULL DEFAULT 1,
  created_at   INTEGER NOT NULL,
  created_by   TEXT    NOT NULL,
  updated_at   INTEGER NOT NULL,
  updated_by   TEXT    NOT NULL,
  deleted_at   INTEGER,

  title        TEXT    NOT NULL,
  note         TEXT    NOT NULL DEFAULT '',
  done_at      INTEGER,
  done_day     INTEGER,
  due_at       INTEGER,
  remind_at    INTEGER,
  priority     INTEGER NOT NULL DEFAULT 2,
  pinned       INTEGER NOT NULL DEFAULT 0,
  parent_id    TEXT    REFERENCES todos(id),
  sort_key     REAL    NOT NULL DEFAULT 0,
  zone         TEXT    NOT NULL DEFAULT 'inbox',
  created_day  INTEGER,
  created_hour INTEGER
);
CREATE INDEX idx_todos_live   ON todos(zone, pinned DESC, sort_key);
CREATE INDEX idx_todos_done   ON todos(done_day);
CREATE INDEX idx_todos_parent ON todos(parent_id);
CREATE INDEX idx_todos_upd    ON todos(updated_at);
-- 逾期与今日到期只看未完成的活动行
CREATE INDEX idx_todos_due    ON todos(due_at) WHERE deleted_at IS NULL AND done_at IS NULL;

CREATE TABLE note_folders (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  name       TEXT    NOT NULL,
  parent_id  TEXT,
  sort       REAL    NOT NULL DEFAULT 0
);

CREATE TABLE notes (
  id          TEXT PRIMARY KEY,
  rev         INTEGER NOT NULL DEFAULT 1,
  created_at  INTEGER NOT NULL,
  created_by  TEXT    NOT NULL,
  updated_at  INTEGER NOT NULL,
  updated_by  TEXT    NOT NULL,
  deleted_at  INTEGER,
  title       TEXT    NOT NULL,
  body_md     TEXT    NOT NULL DEFAULT '',
  folder_id   TEXT    REFERENCES note_folders(id),
  pinned      INTEGER NOT NULL DEFAULT 0,
  trashed_at  INTEGER,
  created_day INTEGER
);
CREATE INDEX idx_notes_folder ON notes(folder_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_trash  ON notes(trashed_at DESC);

CREATE TABLE tags (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  name       TEXT    NOT NULL,
  color      TEXT,
  -- 活动行之间名字唯一，删掉的名字可以被后来者重新使用。
  -- SQLite 不接受 `UNIQUE(name, COALESCE(deleted_at,0))` 这种表达式约束，
  -- 所以把「活动名字」物化成生成列：墓碑行的 name_live 是 NULL，UNIQUE 里 NULL 互不相犯。
  name_live  TEXT GENERATED ALWAYS AS (CASE WHEN deleted_at IS NULL THEN name ELSE NULL END) STORED,
  UNIQUE (name_live)
);

CREATE TABLE todo_tag_links (
  todo_id    TEXT    NOT NULL REFERENCES todos(id),
  tag_id     TEXT    NOT NULL REFERENCES tags(id),
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  PRIMARY KEY (todo_id, tag_id)
);

CREATE TABLE note_tag_links (
  note_id    TEXT    NOT NULL REFERENCES notes(id),
  tag_id     TEXT    NOT NULL REFERENCES tags(id),
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  PRIMARY KEY (note_id, tag_id)
);

CREATE TABLE resource_zones (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  name       TEXT    NOT NULL,
  sort       REAL    NOT NULL DEFAULT 0
);

CREATE TABLE resources (
  id               TEXT PRIMARY KEY,
  rev              INTEGER NOT NULL DEFAULT 1,
  created_at       INTEGER NOT NULL,
  created_by       TEXT    NOT NULL,
  updated_at       INTEGER NOT NULL,
  updated_by       TEXT    NOT NULL,
  deleted_at       INTEGER,
  kind             TEXT    NOT NULL CHECK (kind IN ('app', 'web', 'file')),
  name             TEXT    NOT NULL,
  target           TEXT    NOT NULL,
  args             TEXT    NOT NULL DEFAULT '',
  icon_path        TEXT,
  zone_id          TEXT    REFERENCES resource_zones(id),
  use_count        INTEGER NOT NULL DEFAULT 0,
  last_launched_at INTEGER,
  note             TEXT    NOT NULL DEFAULT '',
  -- 原 PLAN §5.2 的 secret_note：本机 AES-256-GCM 密文，前缀带密钥 id，
  -- 跨机恢复解不开时明说解不开（§2 原则 5），不显示乱码。
  secret_cipher    TEXT
);
CREATE INDEX idx_res_zone ON resources(zone_id, use_count DESC);

CREATE TABLE stickies (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  slot       INTEGER NOT NULL,
  content    TEXT    NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX uq_sticky_slot ON stickies(slot) WHERE deleted_at IS NULL;

CREATE TABLE countdowns (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  label      TEXT    NOT NULL DEFAULT '',
  mode       TEXT    NOT NULL CHECK (mode IN ('duration', 'timed', 'daily', 'interval')),
  spec       TEXT    NOT NULL DEFAULT '{}',
  ends_at    INTEGER,
  paused     INTEGER NOT NULL DEFAULT 0,
  state      TEXT    NOT NULL DEFAULT 'idle' CHECK (state IN ('idle', 'running', 'paused', 'finished'))
);

CREATE TABLE linked_snapshots (
  source       TEXT NOT NULL,
  kind         TEXT NOT NULL,
  fetched_at   INTEGER NOT NULL,
  sig          TEXT NOT NULL,
  status       TEXT NOT NULL CHECK (status IN ('ok', 'missing', 'stale', 'error')),
  detail       TEXT NOT NULL DEFAULT '',
  payload_json TEXT NOT NULL,
  PRIMARY KEY (source, kind)
);

CREATE TABLE backup_log (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  at          INTEGER NOT NULL,
  ok          INTEGER NOT NULL,
  bytes       INTEGER,
  remote_path TEXT,
  error       TEXT,
  kind        TEXT NOT NULL DEFAULT 'daily' CHECK (kind IN ('daily', 'manual', 'restore', 'skip'))
);

CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT NOT NULL
);
