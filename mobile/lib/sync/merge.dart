/// 把对端那份快照**并进**本地表：行级 LWW、整行替换、墓碑跟着行走。
///
/// 与 Rust `src-tauri/src/sync/merge.rs` 逐条同规则（那边有同一批测试，两边一起改）：
/// 1. 先比 `updated_at`，平票比 `updated_by` 的字典序 —— 两台机器算出同一个赢家；
/// 2. 整行替换，不做字段级合并；
/// 3. 不用 `INSERT OR REPLACE`：REPLACE 先删后插，删的那一刻
///    `parent_id REFERENCES todos(id)` 会当场失败（外键开着），改父项就会炸子项；
/// 4. 合并进来的行 `dirty = 0` —— 它已经在云上那份里了，不是本机欠账。
library;

import 'package:sqflite/sqflite.dart';

import 'snapshot.dart';

/// 手机接受哪几张表：`todos` 对等；`classes` 单向 PC→手机，只读。
/// 别的表名（哪怕对端发来）一张都不并，并在报告里点名。
const phoneAccepts = ['todos', 'classes'];

class MergeReport {
  /// 对端的版本更新，写进本地的行数（含墓碑行）
  final int applied;

  /// 本地版本更新或打平，保留原样的行数
  final int kept;

  /// 快照里有、但这台机器不接受的表名
  final List<String> refused;

  const MergeReport({this.applied = 0, this.kept = 0, this.refused = const []});
}

/// SQLite 的列类型只决定 affinity。`"2"` 进 INTEGER 会被悄悄转成 2，`123` 进 TEXT
/// 会变成字符串；本地不报错，下一次统计/比较时才表现成"数据莫名变了形状"。所以这里按
/// 声明类型拒绝，和 Rust `sync::merge` 用同一份 affinity 规则。
String _affinityOf(String declaredType) {
  final t = declaredType.toUpperCase();
  if (t.contains('INT')) return 'INTEGER';
  if (t.contains('CHAR') || t.contains('CLOB') || t.contains('TEXT')) return 'TEXT';
  if (t.contains('BLOB') || t.isEmpty) return 'BLOB';
  if (t.contains('REAL') || t.contains('FLOA') || t.contains('DOUB')) return 'REAL';
  return 'NUMERIC';
}

bool _shapeMatches(String affinity, Object? v) {
  if (v == null) return true;
  if (v is bool) {
    return affinity == 'INTEGER' || affinity == 'REAL' || affinity == 'NUMERIC';
  }
  if (v is int) return affinity != 'TEXT' && affinity != 'BLOB';
  if (v is double) {
    if (!v.isFinite) return false;
    if (affinity == 'TEXT' || affinity == 'BLOB') return false;
    if (affinity == 'INTEGER') return v == v.roundToDouble();
    return true;
  }
  if (v is String) return affinity == 'TEXT' || affinity == 'BLOB';
  return false;
}

/// JSON 值 → SQL 值。数组/对象一律拒绝：这几张表里没有 JSON 列，
/// 真出现了就是 schema 与传输对不上，存成文本会让下次读回来的是带引号的字符串。
Object? _toSql(String table, String id, String col, String declaredType, Object? v) {
  final affinity = _affinityOf(declaredType);
  if (!_shapeMatches(affinity, v)) {
    final kind = v is bool
        ? '布尔值'
        : v is num
            ? '数字'
            : v is String
                ? '字符串'
                : '数组/对象';
    throw SnapshotException(
        '$table 的行 $id 列 $col 本地声明是 $declaredType，对端却发来$kind；拒绝让 SQLite 猜一个转换');
  }
  if (v == null || v is int || v is double || v is String) return v;
  if (v is bool) return v ? 1 : 0;
  throw SnapshotException('$table 的行 $id 列 $col 是数组/对象，同步表里没有这种列，拒绝写入');
}

({String id, int updatedAt, String updatedBy}) _stampOf(
    String table, Map<String, Object?> row) {
  final id = row['id'];
  if (id is! String || id.isEmpty) {
    throw SnapshotException('$table 里有一行没有 id');
  }
  final at = row['updated_at'];
  if (at is! int) {
    throw SnapshotException('$table 的行 $id 没有 updated_at，无法裁决新旧');
  }
  final by = row['updated_by'];
  if (by is! String) {
    throw SnapshotException('$table 的行 $id 没有 updated_by，平票时无法裁决');
  }
  return (id: id, updatedAt: at, updatedBy: by);
}

/// 行级 Last-Write-Wins，与 Rust `sync::lww::is_newer` 同一句：
/// 先比墙钟，平票比设备 id 字典序。设备 id 是 ASCII，两边排序一致。
bool isNewer((int, String) candidate, (int, String) current) =>
    candidate.$1 > current.$1 ||
    (candidate.$1 == current.$1 && candidate.$2.compareTo(current.$2) > 0);

Future<({int applied, int kept})> _applyTable(
  DatabaseExecutor db,
  String table,
  List<Map<String, Object?>> rows,
) async {
  // 课表是「PC 那一份的完整副本」：桌面导入是物理删 + 每轮新 id（`timetable::import`），
  // 所以这份快照里没有的那一行就是被删掉的课。不删就会在手机上攒一批永远删不掉的幽灵课 ——
  // 而课表在手机只读，用户连手动清掉的地方都没有。
  //
  // 两处保守：只删「这份快照里出现过的 term」，不牵连别的学期；
  // 快照没带 classes、或带的是空数组时**一律不动** —— 「这轮没发课表」和「PC 上真的没课」
  // 在字节上长得一样，猜错的代价是清空用户手机里的课表，那是不可逆的。
  if (table == 'classes' && rows.isNotEmpty) {
    final terms = <String>{};
    for (final r in rows) {
      final t = r['term'];
      if (t is! String) {
        throw SnapshotException('classes 有一行没带 term，无法判断该替换哪一学期');
      }
      terms.add(t);
    }
    for (final term in terms) {
      await db.rawDelete('DELETE FROM classes WHERE term = ?', [term]);
    }
  }

  final localCols = (await wireColumns(db, table)).toSet();
  final typeRows = await db.rawQuery('PRAGMA table_info($table)');
  final declaredTypes = <String, String>{
    for (final r in typeRows)
      if (r['name'] is String) r['name'] as String: (r['type'] as String?) ?? '',
  };
  // `todos` 有 dirty，`classes` 没有（手机对课表只读，整表随快照发布即可）。
  // 所以「并进来的行不算本机欠账」这句只在有该列的表上写成 SET dirty = 0。
  final hasDirty = declaredTypes.containsKey('dirty');

  // 一次读光现有版本戳，别每行查一次：全量合并本来就要跑几百上千行
  final current = <String, (int, String)>{};
  for (final r in await db.rawQuery('SELECT id, updated_at, updated_by FROM $table')) {
    current[r['id'] as String] = (r['updated_at'] as int, r['updated_by'] as String);
  }

  var applied = 0;
  var kept = 0;
  for (final row in rows) {
    final stamp = _stampOf(table, row);
    final here = current[stamp.id];
    if (here != null) {
      if (!isNewer((stamp.updatedAt, stamp.updatedBy), here)) {
        kept++;
        continue;
      }
      // 对端赢：把它发来的每一列都盖上去；dirty 置 0
      final sets = [
        for (final k in row.keys)
          if (k != 'id' && localCols.contains(k)) k,
      ];
      if (sets.isEmpty) {
        throw SnapshotException('$table 的行 ${stamp.id} 除了 id 什么都没带');
      }
      final sql = 'UPDATE $table SET ${sets.map((k) => '"$k" = ?').join(', ')}'
          '${hasDirty ? ', dirty = 0' : ''} WHERE id = ?';
      final args = [
        for (final k in sets) _toSql(table, stamp.id, k, declaredTypes[k] ?? '', row[k]),
        stamp.id,
      ];
      await db.rawUpdate(sql, args);
      applied++;
    } else {
      // 本地没有这一行：对端发来的列原样插进去，没发的列由建表时的 DEFAULT 兜。
      // 若某个 NOT NULL 且无默认值的列没发，SQLite 直接报错 —— 宁可失败也别编一个值。
      final cols = [
        for (final k in row.keys)
          if (localCols.contains(k)) k,
      ];
      if (cols.isEmpty) {
        throw SnapshotException('$table 的行 ${stamp.id} 一列可用都没有');
      }
      final sql = 'INSERT INTO $table (${cols.map((k) => '"$k"').join(', ')}'
          '${hasDirty ? ', dirty' : ''}) '
          'VALUES (${List.filled(cols.length, '?').join(', ')}${hasDirty ? ', 0' : ''})';
      final args = [for (final k in cols) _toSql(table, stamp.id, k, declaredTypes[k] ?? '', row[k])];
      try {
        await db.rawInsert(sql, args);
      } catch (e) {
        throw SnapshotException(
            '$table 插不进对端的行 ${stamp.id}：$e（多半是这一版没有它要求的某一列）');
      }
      applied++;
    }
  }
  return (applied: applied, kept: kept);
}

/// 并进本地。整批一个事务：一半生效一半失败的库，比整批没发生更难解释。
Future<MergeReport> applySnapshot(
  Database db,
  Snapshot snap,
  List<String> accepts,
) async {
  var applied = 0;
  var kept = 0;
  final refused = <String>[];
  await db.transaction((txn) async {
    for (final entry in snap.tables.entries) {
      final table = entry.key;
      if (!accepts.contains(table)) {
        refused.add(table);
        continue;
      }
      final exists = await txn.rawQuery(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
        [table],
      );
      if (exists.isEmpty) {
        throw SnapshotException('对端发来了 $table，这台机器的库里没有这张表（版本差得太远）');
      }
      final r = await _applyTable(txn, table, entry.value);
      applied += r.applied;
      kept += r.kept;
    }
  });
  return MergeReport(applied: applied, kept: kept, refused: refused);
}

/// PUT 成功之后清脏。**比的是行自己的 `updated_at`，不是本机 now**（§3.3 第 2 条）：
/// 序列化之后、响应回来之前新写的那一条必须留在 outbox 里，否则它会被下一次合并悄悄盖掉。
///
/// `sealedAt` 必须是那份快照自己的时间戳（`Snapshot.sealedAt`），调用方不许传 now。
Future<int> clearDirty(DatabaseExecutor db, int sealedAt) => db.rawUpdate(
      'UPDATE todos SET dirty = 0 WHERE dirty = 1 AND updated_at <= ?',
      [sealedAt],
    );

/// 「N 条待上传」的唯一计数来源（§3.4）。别在界面或命令层里另写一条 SQL。
Future<int> pending(DatabaseExecutor db) async {
  final rows = await db.rawQuery('SELECT count(*) AS n FROM todos WHERE dirty = 1');
  return (rows.first['n'] as int?) ?? 0;
}
