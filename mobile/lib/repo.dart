/// 手机本机库的读写口。界面只通过它碰 SQLite —— 合并走的是另一条原始 SQL 的路
/// （`sync/merge.dart`），因为那边要按对端发来的列名动态拼 SQL。
///
/// 写路径只有两条：勾选、新建。每条都 `dirty = 1`（PLAN-MOBILE §3.4）：
/// 漏一处不会报错，只会让那一行永远上不了云，所以测试里逐条数脏行数。
library;

import 'dart:convert';
import 'dart:math';

import 'package:sqflite/sqflite.dart';

import 'medicine.dart';
import 'timetable.dart';
import 'todo.dart';

/// settings 里手机与 PC 共用的两条基准键（快照 meta 白名单，见 Rust `sync::snapshot`）。
const keyTerm = 'timetable.term';
const keyWeek1 = 'timetable.week1Monday';

/// 同步状态在 settings 里的键，与桌面 `settings::KEY_SYNC_STATE` 同名。
const keySyncState = 'sync.state';

/// 坚果云邮箱。应用密码不走这张表（见 `credentials.dart`）。
const keyEmail = 'webdav.email';

/// 药箱只读视图的本地缓存与开关。都是本机 UX/缓存，不上云。
const keyMedicineEnabled = 'medicine.enabled';
const keyMedicineView = 'medicine.view';
const keyMedicineError = 'medicine.lastError';
const keyMedicineErrorAt = 'medicine.lastErrorAt';

Future<Object?> readSetting(Database db, String key) async {
  final rows = await db.query(
    'settings',
    columns: ['value_json'],
    where: 'key = ?',
    whereArgs: [key],
    limit: 1,
  );
  if (rows.isEmpty) return null;
  final raw = rows.first['value_json'];
  if (raw is! String) return null;
  try {
    return jsonDecode(raw);
  } catch (_) {
    return null;
  }
}

Future<void> writeSetting(
  Database db,
  String key,
  Object? value,
  int now,
  String device,
) =>
    db.insert(
      'settings',
      {
        'key': key,
        'value_json': jsonEncode(value),
        'updated_at': now,
        'updated_by': device,
      },
      conflictAlgorithm: ConflictAlgorithm.replace,
    );

/// 本机设备 id。首次启动生成一次，之后一直用它 —— `updated_by` 的平票裁决靠它。
///
/// 存库而不是存文件：手机没有「整库恢复」这条路，库就是身份本身。
/// 前缀 `phone-` 让远端 listing 一眼看出是哪个端（PC 那份是 `pc-…`）。
Future<String> loadOrCreateDeviceId(Database db, int now) async {
  final existing = await readSetting(db, 'device.id');
  if (existing is String && existing.trim().isNotEmpty) return existing.trim();
  final rnd = Random.secure();
  final hex = List.generate(16, (_) => rnd.nextInt(256).toRadixString(16).padLeft(2, '0')).join();
  final id = 'phone-$hex';
  await writeSetting(db, 'device.id', id, now, id);
  return id;
}

class PhoneRepo {
  final Database db;
  final String deviceId;

  PhoneRepo(this.db, this.deviceId);

  static Future<PhoneRepo> open(Database db, {required int now}) async =>
      PhoneRepo(db, await loadOrCreateDeviceId(db, now));

  Future<List<Todo>> todos() async {
    final rows = await db.query('todos');
    return [for (final r in rows) Todo.fromRow(r)];
  }

  /// 当前学期。手机对课表只读，学期来自快照 meta（PC 的导入是唯一写入方）。
  Future<String?> term() async {
    final v = await readSetting(db, keyTerm);
    return v is String && v.trim().isNotEmpty ? v.trim() : null;
  }

  Future<String?> week1Monday() async {
    final v = await readSetting(db, keyWeek1);
    return v is String && v.trim().isNotEmpty ? v.trim() : null;
  }

  /// 某学期的课，含墓碑行（周次骨架不用墓碑，但同步要看得见它们）。
  Future<List<ClassRow>> classesFor(String? term) async {
    if (term == null) return const [];
    final rows = await db.query('classes', where: 'term = ?', whereArgs: [term]);
    return [
      for (final r in rows)
        ClassRow(
          id: r['id'] as String,
          term: r['term'] as String,
          name: r['name'] as String,
          teacher: (r['teacher'] as String?) ?? '',
          room: (r['room'] as String?) ?? '',
          day: (r['day'] as int?) ?? 1,
          startMin: (r['start_min'] as int?) ?? 0,
          endMin: (r['end_min'] as int?) ?? 0,
          period: (r['period'] as String?) ?? '',
          weeks: (r['weeks'] as String?) ?? '',
          deletedAt: r['deleted_at'] as int?,
        ),
    ];
  }

  /// 总周数从各课周次集合的最大端推，与桌面 `repos::timetable` 同一条规则。
  Future<int?> totalWeeks(String? term) async {
    if (term == null) return null;
    final rows = await db.query(
      'classes',
      columns: ['weeks'],
      where: 'term = ? AND deleted_at IS NULL',
      whereArgs: [term],
    );
    int? max;
    for (final r in rows) {
      final high = weeksHigh((r['weeks'] as String?) ?? '');
      if (high != null && (max == null || high > max)) max = high;
    }
    return max;
  }

  /// 勾选 / 取消勾选只动这几列：模型里没有的列（提醒、完成日、创建时的日/时）
  /// 原样留在这行里，不被这次写入抹掉。
  Future<void> saveDone(Todo next) => db.update(
        'todos',
        {
          'rev': next.rev,
          'completed': next.completed ? 1 : 0,
          'done_at': next.doneAt,
          'updated_at': next.updatedAt,
          'updated_by': next.updatedBy,
          'dirty': 1,
        },
        where: 'id = ?',
        whereArgs: [next.id],
      );

  Future<void> insertTodo(Todo t) => db.insert('todos', {
        'id': t.id,
        'rev': t.rev,
        'created_at': t.createdAt,
        'created_by': t.createdBy,
        'updated_at': t.updatedAt,
        'updated_by': t.updatedBy,
        'deleted_at': t.deletedAt,
        'title': t.title,
        'note': t.note,
        'done_at': t.doneAt,
        'due_at': t.dueAt,
        'priority': t.priority,
        'pinned': t.pinned ? 1 : 0,
        'parent_id': null,
        'sort_key': t.sortKey,
        'zone': t.zone,
        'created_day': dayInt(DateTime.fromMillisecondsSinceEpoch(t.createdAt)),
        'created_hour': DateTime.fromMillisecondsSinceEpoch(t.createdAt).hour,
        'completed': t.completed ? 1 : 0,
        'dirty': 1,
      });

  /// 快照 meta 落进本机 settings：学期与开学日决定「今天第几周」，
  /// 它们不是本机偏好，是解释课表的基准（PLAN-MOBILE §3.2 补记）。
  Future<void> saveMeta(Map<String, String> meta, int now) async {
    for (final entry in meta.entries) {
      if (entry.key != keyTerm && entry.key != keyWeek1) continue;
      await writeSetting(db, entry.key, entry.value, now, deviceId);
    }
  }

  Future<Map<String, Object?>> syncState() async {
    final v = await readSetting(db, keySyncState);
    return v is Map<String, Object?> ? v : const {};
  }

  Future<void> saveSyncState(Map<String, Object?> state, int now) =>
      writeSetting(db, keySyncState, state, now, deviceId);

  Future<String?> email() async {
    final v = await readSetting(db, keyEmail);
    return v is String && v.trim().isNotEmpty ? v.trim() : null;
  }

  Future<void> saveEmail(String email, int now) =>
      writeSetting(db, keyEmail, email.trim(), now, deviceId);

  /// 药箱视图默认开：手机端只读、不改任何东西，用户在设置里可以关掉（关掉即停）。
  Future<bool> medicineEnabled() async {
    final v = await readSetting(db, keyMedicineEnabled);
    return v is bool ? v : true;
  }

  Future<void> setMedicineEnabled(bool on, int now) =>
      writeSetting(db, keyMedicineEnabled, on, now, deviceId);

  Future<MedicineView?> medicineView() async {
    final v = await readSetting(db, keyMedicineView);
    return v is Map<String, Object?> ? MedicineView.fromJson(v) : null;
  }

  Future<void> saveMedicineView(MedicineView view, int now) =>
      writeSetting(db, keyMedicineView, view.toJson(), now, deviceId);

  Future<({String? error, int? at})> medicineError() async {
    final err = await readSetting(db, keyMedicineError);
    final at = await readSetting(db, keyMedicineErrorAt);
    return (
      error: err is String && err.isNotEmpty ? err : null,
      at: at is int ? at : null,
    );
  }

  Future<void> saveMedicineError(String? error, int at) async {
    await writeSetting(db, keyMedicineError, error, at, deviceId);
    await writeSetting(db, keyMedicineErrorAt, at, at, deviceId);
  }
}
