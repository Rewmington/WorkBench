/// 一次同步在两台机器之间传的那份「全量表快照」的 Dart 侧实现。
///
/// 与 Rust `src-tauri/src/sync/snapshot.rs` 是**同一份格式的两个实现**，不是两个格式：
/// ```text
/// {"ver":1,"device":"phone-…","sealed_at":毫秒,
///  "tables":{"todos":[{"列名":值,…},…],"classes":[…]},
///  "meta":{"timetable.term":"…","timetable.week1Monday":"…"}}
/// ```
/// 明文 gzip 之后进信封（`envelope.dart`），信封 PUT 成 `device-<id>.enc`。
///
/// 两点与 Rust 侧逐条对齐：
/// - 每一行是**列名 → 值**的对象，不是按位置对齐的数组 —— 两端版本不一致时
///   数组错一位会静默把数据写歪，对象则看得见「对面没有这一列」。
/// - `dirty` 不在里面：它是「这台机器还没传出去」，传给对面只会让对面误判。
///
/// `meta` 是 M11c 补的：手机要用 `timetable.week1Monday` 算「今天第几周」、
/// 用 `timetable.term` 过滤旧学期。它们不是本机 UX 偏好，是解释 `classes` 表的基准，
/// 而 `settings` 表整体不同步。白名单只放这两条。
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:sqflite/sqflite.dart';

const ver = 1;

/// 只在本地有意义、绝不上线的列。
const localOnly = ['dirty'];

/// 快照里允许跟表的行一起走的那几条 settings（键名就是 settings 里的原键）。
const metaKeys = ['timetable.term', 'timetable.week1Monday'];

class SnapshotException implements Exception {
  final String message;
  SnapshotException(this.message);
  @override
  String toString() => message;
}

class Snapshot {
  final String device;
  final int sealedAt;
  final Map<String, List<Map<String, Object?>>> tables;
  final Map<String, String> meta;

  Snapshot({
    required this.device,
    required this.sealedAt,
    required this.tables,
    this.meta = const {},
  });

  List<Map<String, Object?>> rows(String table) => tables[table] ?? const [];

  /// gzip 后的明文，等着进信封。
  Uint8List toBytes() {
    final plain = jsonEncode({
      'ver': ver,
      'device': device,
      'sealed_at': sealedAt,
      'tables': tables,
      'meta': meta,
    });
    return Uint8List.fromList(gzip.encode(utf8.encode(plain)));
  }

  static Snapshot fromBytes(List<int> bytes) {
    late List<int> plain;
    try {
      plain = gzip.decode(bytes);
    } catch (_) {
      throw SnapshotException('快照解不开 gzip：传输出来的不是 Anvil 的同步文件');
    }
    late Object? root;
    try {
      root = jsonDecode(utf8.decode(plain));
    } catch (_) {
      throw SnapshotException('快照不是合法 JSON：文件被改过或版本不认识');
    }
    if (root is! Map<String, Object?>) {
      throw SnapshotException('快照根节点不是对象');
    }
    final v = root['ver'];
    if (v is! int) throw SnapshotException('快照没有版本号');
    if (v != ver) throw SnapshotException('快照版本 $v 这台机器读不懂（当前写的是 v$ver）');

    final tables = <String, List<Map<String, Object?>>>{};
    final rawTables = root['tables'];
    if (rawTables is Map<String, Object?>) {
      for (final entry in rawTables.entries) {
        final rows = entry.value;
        if (rows is! List) {
          throw SnapshotException('快照里的 ${entry.key} 不是一行行的数组');
        }
        final parsed = <Map<String, Object?>>[];
        for (final row in rows) {
          if (row is! Map<String, Object?>) {
            throw SnapshotException('${entry.key} 里有一行不是列名→值的对象');
          }
          parsed.add(row);
        }
        tables[entry.key] = parsed;
      }
    }

    final meta = <String, String>{};
    final rawMeta = root['meta'];
    if (rawMeta is Map<String, Object?>) {
      for (final entry in rawMeta.entries) {
        if (!metaKeys.contains(entry.key)) continue;
        final s = entry.value;
        if (s is String && s.trim().isNotEmpty) meta[entry.key] = s.trim();
      }
    }

    return Snapshot(
      device: (root['device'] as String?) ?? '',
      sealedAt: (root['sealed_at'] as int?) ?? 0,
      tables: tables,
      meta: meta,
    );
  }
}

/// 表里实际存在、且允许上线的列。表名来自本模块的常量，列名来自库自己，
/// 所以拼进 SQL 的不可能是外部输入。
Future<List<String>> wireColumns(DatabaseExecutor db, String table) async {
  final info = await db.rawQuery('PRAGMA table_info($table)');
  if (info.isEmpty) throw SnapshotException('库里没有 $table 这张表');
  return [
    for (final row in info)
      if (row['name'] is String && !localOnly.contains(row['name'])) row['name'] as String,
  ];
}

/// 一张表的全部行，**含墓碑**：传播删除靠的就是那些行（PLAN-MOBILE §3.2）。
Future<List<Map<String, Object?>>> _dumpTable(DatabaseExecutor db, String table) async {
  final cols = await wireColumns(db, table);
  final list = cols.map((c) => '"$c"').join(', ');
  final rows = await db.rawQuery('SELECT $list FROM $table');
  return [
    for (final row in rows)
      {
        for (final c in cols) c: _checkValue(table, row['id']?.toString() ?? '', c, row[c]),
      },
  ];
}

/// 同步表里只有整数/浮点/文本/NULL。二进制列悄悄丢掉是最难查的一类错，所以拒绝。
Object? _checkValue(String table, String id, String col, Object? v) {
  if (v == null || v is int || v is double || v is String) return v;
  if (v is bool) return v ? 1 : 0;
  throw SnapshotException('$table 的行 $id 列 $col 是二进制或复杂值，同步表里没有这种列');
}

/// 本机要发出去的那份快照。手机只发 `todos`，`meta` 留空（基准是 PC 给的，不回声）。
Future<Snapshot> dumpSnapshot(
  DatabaseExecutor db,
  List<String> tables, {
  required String device,
  required int sealedAt,
  Map<String, String> meta = const {},
}) async {
  final out = <String, List<Map<String, Object?>>>{};
  for (final table in tables) {
    out[table] = await _dumpTable(db, table);
  }
  return Snapshot(device: device, sealedAt: sealedAt, tables: out, meta: meta);
}
