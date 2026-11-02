/// 手机端自己的库。表结构照 PC 的 `src-tauri/migrations/` 原样搬，
/// 只建手机真的会读写的那三张：`todos`（对等）、`classes`（只读）、`settings`
/// （设备 id、同步状态、开学日/学期）。
///
/// **列名一个都不翻译**：快照里那行 JSON 的键就是这里的列名（PLAN-MOBILE §3.2），
/// 所以合并时 SQL 的列名可以直接从 `PRAGMA table_info` 取，不存在两张映射表。
///
/// 不建笔记、标签、速达那几张：手机端的范围只有看待办/勾选/新建/看课表（§1）。
/// 对端真发来一张本机没有的表，合并会拒绝并说明原因，而不是悄悄丢掉。
library;

import 'package:path/path.dart' as p;
import 'package:sqflite/sqflite.dart';

/// 建表 DDL。与 PC 迁移后（0001…0006）的 `todos` / `classes` 对齐；
/// PC 加了列而这里没加时，合并会按「本机没有这一列」跳过它 —— 不会把整行写歪。
const _todosDdl = '''
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
  created_hour INTEGER,
  completed    INTEGER NOT NULL DEFAULT 0,
  reminded_at  INTEGER,
  dirty        INTEGER NOT NULL DEFAULT 0
);
''';

const _classesDdl = '''
CREATE TABLE classes (
  id         TEXT PRIMARY KEY,
  rev        INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  created_by TEXT    NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT    NOT NULL,
  deleted_at INTEGER,
  term       TEXT    NOT NULL,
  name       TEXT    NOT NULL,
  teacher    TEXT    NOT NULL DEFAULT '',
  room       TEXT    NOT NULL DEFAULT '',
  day        INTEGER NOT NULL CHECK (day BETWEEN 1 AND 7),
  start_min  INTEGER NOT NULL,
  end_min    INTEGER NOT NULL,
  period     TEXT    NOT NULL DEFAULT '',
  weeks      TEXT    NOT NULL DEFAULT ''
);
''';

const _settingsDdl = '''
CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT NOT NULL
);
''';

/// 打开应用库。Android 用 `sqflite` 的默认目录；测试传 `databaseFactoryFfi`
/// 与 `inMemoryDatabasePath`，走的是同一份建表代码。
Future<Database> openAppDatabase({DatabaseFactory? factory, String? path}) async {
  final f = factory ?? databaseFactory;
  final file = path ?? p.join(await getDatabasesPath(), 'anvil.db');
  return f.openDatabase(
    file,
    options: OpenDatabaseOptions(
      // 升 `version` 必须同时补 `onUpgrade` —— 现在只有 onCreate。
      // 版本号涨上去却没写迁移 = 老设备上的库永远停在旧结构，而 `sync/merge.dart`
      // 会按「本机没有这一列」把新列静默跳过（那是刻意的，见那边那段注释），
      // 于是同步看着成功、新字段却一直不存在。
      version: 1,
      // 测试每个用例都要一份干净的内存库；`:memory:` 同名会被缓存成同一个实例。
      // 应用只在启动时开一次，所以关掉缓存没有代价。
      singleInstance: false,
      // 与桌面 `db::setup::configure` 同一条：外键开着，合并父项时才不会静默留下孤儿。
      onConfigure: (db) => db.execute('PRAGMA foreign_keys = ON'),
      onCreate: (db, _) async {
        await db.execute(_todosDdl);
        await db.execute(_classesDdl);
        await db.execute(_settingsDdl);
        await db.execute('CREATE INDEX idx_todos_upd ON todos(updated_at)');
      },
    ),
  );
}

/// 测试用：无文件、无插件的内存库。`sqfliteFfiInit()` 与工厂由调用方给，
/// 所以这个文件本身不依赖桌面端才有的 ffi 实现。
Future<Database> openMemoryDatabase(DatabaseFactory factory) =>
    openAppDatabase(factory: factory, path: inMemoryDatabasePath);
