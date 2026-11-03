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

/// 药箱可能是另一个坚果云账号；没填时回退同步那组。
const keyMedicineEmail = 'medicine.email';

/// 一条 settings 的三种读法。
///
/// `absent` 与 `broken` 过去都折成 null，于是设置页把「坏掉的 JSON」说成「还没设 /
/// 还没同步过」—— 数据坏了和被用户清掉是两件事，界面上不能长成同一句。
enum SettingState { absent, ok, broken }

typedef SettingRead = ({Object? value, SettingState state});

Future<SettingRead> readSettingState(Database db, String key) async {
  final rows = await db.query(
    'settings',
    columns: ['value_json'],
    where: 'key = ?',
    whereArgs: [key],
    limit: 1,
  );
  if (rows.isEmpty) return (value: null, state: SettingState.absent);
  final raw = rows.first['value_json'];
  if (raw is! String) return (value: null, state: SettingState.broken);
  try {
    return (value: jsonDecode(raw), state: SettingState.ok);
  } catch (_) {
    return (value: null, state: SettingState.broken);
  }
}

Future<Object?> readSetting(Database db, String key) async {
  final read = await readSettingState(db, key);
  return read.state == SettingState.ok ? read.value : null;
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
  final read = await readSettingState(db, 'device.id');
  final existing = read.value;
  if (existing is String && existing.trim().isNotEmpty) return existing.trim();
  if (read.state == SettingState.broken) {
    // 读不出来**不等于没设过**，所以这里绝不写：换一把 id 会让本机历史上每一行的
    // `updated_by` 一夜之间变成别人写的 —— 平票裁决、待上传计数、"本机写过的最晚一条"
    // 垫高的时间下限，全押在这个串上。这是 P1-14 在手机上的更危险版本：PC 那边写回的
    // 是一个默认值，这边写掉的是身份本身。宁可起不来，也不留下一个不可解释的新身份。
    throw StateError('settings 里 device.id 那条解不开 JSON，本机身份不能换：'
        '把那一行修好（或整库恢复）再来。软件不替你生成一把新的。');
  }
  if (existing != null && existing is! String) {
    // 解得开、但根本不是字符串（如 `123`）：同样是坏行，不能借「读不到 id」顺手换身份。
    // 空字符串仍视为没设过，保留第一次初始化路径；这里只拦"有值但形状不对"。
    throw StateError('settings 里 device.id 那条不是字符串，本机身份不能换：'
        '把那一行修好（或整库恢复）再来。软件不替你生成一把新的。');
  }
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

  /// 手机侧「坏掉」与「没设」的分界（P1-14 的形状）。只有两处真需要这个区分，都直接走
  /// 模块级的 `readSettingState`：`loadOrCreateDeviceId` 坏了要**拒绝写**（身份不能换），
  /// `syncStateUnreadable` 坏了要让界面说出来（不许冒充「还没同步过」）。
  /// 其余键（学期、开学日、邮箱、药箱缓存）读不出来都只是那一块空着，重新同步一次就补上，
  /// 不值得为它们各开一条错误行 —— 那是新的一行界面，不是新的一份真相。

  static String? _asString(Object? v) =>
      v is String && v.trim().isNotEmpty ? v.trim() : null;

  /// 当前学期。手机对课表只读，学期来自快照 meta（PC 的导入是唯一写入方）。
  Future<String?> term() async => _asString(await readSetting(db, keyTerm));

  Future<String?> week1Monday() async => _asString(await readSetting(db, keyWeek1));

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

  /// 勾选 / 取消勾选只动这几列：模型里没有的列（提醒、创建时的日/时）
  /// 原样留在这行里，不被这次写入抹掉。
  ///
  /// **完成日是那条规则的例外**：`done_day` 与 `done_at` 是同一件事的两半，必须跟着写。
  /// 桌面那三处统计（今天完成数、热力图、连续天数）只读 `done_day` 不读 `done_at`，
  /// 手机不维护它 = 在手机上完成的那一格永远不存在。
  Future<void> saveDone(Todo next) async {
    // `doneAt` 是另一个库里的字段，Dart 不在 `if` 之后替它收窄成非空 —— 先落成局部量，
    // 免得为了过编译器在表达式里再绕一层 `!`（那个 `!` 恰好是这条分支要证明的事）。
    final doneAt = next.doneAt;
    await db.update(
      'todos',
      {
        'rev': next.rev,
        'completed': next.completed ? 1 : 0,
        'done_at': doneAt,
        // 与 PC `todos::set_done` 同一句规则：`completed` 是当前状态，`done_at`/`done_day`
        // 记的是**最后一次**完成于何时、取消勾选不清空。所以只在勾上去的时候写这一列，
        // 取消时连键都不带 —— 让库里那一格原样留着，才是两边一致的行为。
        if (next.completed && doneAt != null)
          'done_day': dayInt(DateTime.fromMillisecondsSinceEpoch(doneAt)),
        'updated_at': next.updatedAt,
        'updated_by': next.updatedBy,
        'dirty': 1,
      },
      where: 'id = ?',
      whereArgs: [next.id],
    );
  }

  Future<void> insertTodo(Todo t) async {
    final doneAt = t.doneAt;
    await db.insert('todos', {
      'id': t.id,
      'rev': t.rev,
      'created_at': t.createdAt,
      'created_by': t.createdBy,
      'updated_at': t.updatedAt,
      'updated_by': t.updatedBy,
      'deleted_at': t.deletedAt,
      'title': t.title,
      'note': t.note,
      'done_at': doneAt,
      // 新建时就带着完成状态的那一档（界面上走不到，恢复与测试会）：那一格也要落下
      'done_day': doneAt != null && t.completed
          ? dayInt(DateTime.fromMillisecondsSinceEpoch(doneAt))
          : null,
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
  }

  /// 快照 meta 落进本机 settings：学期与开学日决定「今天第几周」，
  /// 它们不是本机偏好，是解释课表的基准（PLAN-MOBILE §3.2 补记）。
  ///
  /// 开学日在落库前验一次格式。坏值一旦进库，今天屏只能算不出周次，但那条坏值会一直
  /// 留在本机；同步每次都会重新把它读出来，所以这里宁可少存一条、也不要存一条解释不了课表的。
  Future<void> saveMeta(Map<String, String> meta, int now) async {
    for (final entry in meta.entries) {
      if (entry.key != keyTerm && entry.key != keyWeek1) continue;
      final value = entry.value.trim();
      if (value.isEmpty) continue;
      if (entry.key == keyWeek1 && isoToDayOrNull(value) == null) continue;
      await writeSetting(db, entry.key, value, now, deviceId);
    }
  }

  Future<Map<String, Object?>> syncState() async {
    final v = await readSetting(db, keySyncState);
    return v is Map<String, Object?> ? v : const {};
  }

  /// 「上次同步的状态读不出来」与「还没同步过」是两句话，界面不能只说得出口径更宽的那一句。
  /// 解不开 JSON、或解出来根本不是那个对象 —— 两种都没法用来讲「上次同步于几点」，
  /// 于是都不许冒充成「还没同步过」；真缺席（absent）不算故障。
  Future<bool> syncStateUnreadable() async {
    final read = await readSettingState(db, keySyncState);
    if (read.state == SettingState.absent) return false;
    return read.value is! Map<String, Object?>;
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

  /// 药箱专用邮箱；null = 没单独填，药箱回退用同步那组。
  Future<String?> medicineEmail() async {
    final v = await readSetting(db, keyMedicineEmail);
    return v is String && v.trim().isNotEmpty ? v.trim() : null;
  }

  Future<void> saveMedicineEmail(String email, int now) =>
      writeSetting(db, keyMedicineEmail, email.trim(), now, deviceId);
}
