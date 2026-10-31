/// 手机端的应用状态：待办、课表骨架、同步状态，以及那一屏要用的分区结果。
///
/// 界面读内存里的这份快照，写入先改内存再落 SQLite（`repo.dart`）；
/// 同步结束后 `reload()` 把库里的新行重新读上来。SQLite 是真相，内存只是投影。
library;

import 'dart:math';

import 'package:flutter/foundation.dart';

import 'medicine.dart';
import 'medicine_service.dart';
import 'repo.dart';
import 'sync/service.dart';
import 'timetable.dart';
import 'todo.dart';

/// 演示时钟：`--dart-define=ANVIL_DEMO=full` 那一档把「现在」钉在对比稿的同一时刻，
/// 截图才敢和 `design/workbench-mobile-ui.html` 逐像素对（桌面 M9c 冻时钟对句是同一条做法）。
/// 真机用 [Clock.live]，每次读都是当下 —— 否则一个会话里新写的待办全盖上启动那一刻。
class Clock {
  final int Function() _read;
  const Clock(this._read);

  static Clock demo(int y, int mo, int d, int h, int mi) =>
      Clock(() => DateTime(y, mo, d, h, mi).millisecondsSinceEpoch);

  factory Clock.live() => Clock(() => DateTime.now().millisecondsSinceEpoch);

  DateTime get date => DateTime.fromMillisecondsSinceEpoch(now);

  int get now => _read();

  /// 周一 = 1 … 周日 = 7，与 Rust `time::days::weekday` 同一约定
  int get dow => date.weekday;
}

/// 同步状态。**四态四句**（§5.5），一句都不多写：不出现「已同步」、不出现转圈、
/// 真空态与「还没同步过」不共用一句。
///
/// 用 sealed 子类而不是工厂构造函数：界面上要 `switch` 它，而 Dart 的模式匹配只认类型。
sealed class SyncState {
  const SyncState();
}

/// 一次都没成功过。与「今天确实没待办」是两句话，不能共用（§5.5 末行）
final class NeverSynced extends SyncState {
  const NeverSynced();
}

/// 最后一次成功同步于 atMs。那句「N 条待上传」不在这里存 —— 它由 [Store.pending]
/// 从 dirty 数出来，两处存同一个数迟早对不上（§3.4 计数来源唯一）。
final class SyncedAt extends SyncState {
  final int atMs;
  const SyncedAt(this.atMs);
}

/// 这次没连上：手上传的还是 atMs 那一份，明说到什么时候
final class OfflineAt extends SyncState {
  final int atMs;
  const OfflineAt(this.atMs);
}

class Store extends ChangeNotifier {
  final Clock clock;

  /// 真相在 SQLite；演示/截图那几档没有库，就只剩内存。
  final PhoneRepo? repo;
  final SyncService? syncer;
  final MedicineService? medicineService;

  List<Todo> todos;
  List<ClassRow> classes;
  SyncState sync;

  /// 开学第一个周一（`yyyy-mm-dd`）与总周数：手机上这两个值跟着快照 meta 来
  String? week1Monday;
  int? totalWeeks;

  /// 坚果云账号是否已填（应用密码进 Keystore，本机不存明文，所以这里只有「填没填」）
  bool accountSaved;

  /// 已在设置里填过的邮箱；没填过为 null（界面显示「未填」）
  String? accountEmail;

  /// 同步进行中（设置页那行据此说「正在同步」，不是转圈）
  bool syncing = false;

  /// 上次同步失败的那句话；成功或还没试过时为 null
  String? syncError;
  List<String> peers = const [];

  /// 药箱只读视图。null = 还没读过；空视图 = 药箱还没同步过 / 没有药。
  MedicineView? medicine;
  bool medicineEnabled = true;
  bool medicineFetching = false;
  String? medicineError;
  int? medicineErrorAt;

  /// 药箱单独填的邮箱；null = 用同步那组
  String? medicineAccountEmail;

  /// 药箱凭据是否配齐（独立账号或回退同步账号）
  bool medicineConfigured = false;

  Store({
    required this.clock,
    this.repo,
    this.syncer,
    this.medicineService,
    this.todos = const [],
    this.classes = const [],
    this.sync = const NeverSynced(),
    this.week1Monday,
    this.totalWeeks,
    this.accountSaved = false,
    this.medicine,
    this.medicineEnabled = true,
    this.medicineError,
  });

  /// 真机路径：从库里读一遍再交给界面。
  static Future<Store> load(
    PhoneRepo repo, {
    Clock? clock,
    SyncService? syncer,
    MedicineService? medicineService,
  }) async {
    final store = Store(
      clock: clock ?? Clock.live(),
      repo: repo,
      syncer: syncer,
      medicineService: medicineService,
    );
    await store.reload();
    return store;
  }

  /// 库是真相：同步完、保存账号后都走这里，界面不自己拼状态。
  Future<void> reload() async {
    final r = repo;
    if (r != null) {
      todos = await r.todos();
      week1Monday = await r.week1Monday();
      final term = await r.term();
      classes = await r.classesFor(term);
      totalWeeks = await r.totalWeeks(term);
      final state = await r.syncState();
      _applyState(state);
      final s = syncer;
      accountEmail = s == null ? null : await s.email;
      accountSaved = s != null && await s.configured;
      medicineEnabled = await r.medicineEnabled();
      medicine = await r.medicineView();
      medicineAccountEmail = await r.medicineEmail();
      medicineConfigured =
          medicineService != null && await medicineService!.configured;
      final err = await r.medicineError();
      medicineError = err.error;
      medicineErrorAt = err.at;
    }
    notifyListeners();
  }

  void _applyState(Map<String, Object?> state) {
    final at = state['lastSyncAt'];
    final errAt = state['lastErrorAt'];
    final err = state['lastError'];
    syncError = err is String && err.isNotEmpty ? err : null;
    final rawPeers = state['peers'];
    peers = rawPeers is List
        ? [for (final p in rawPeers) if (p is String) p]
        : const [];
    sync = switch (at) {
      int ms when errAt is int && errAt > ms => OfflineAt(ms),
      int ms => SyncedAt(ms),
      _ => const NeverSynced(),
    };
  }

  int get now => clock.now;

  /// 待上传计数唯一来源：本机写过的行数（§3.4，与桌面同一个口径）。
  int get pending => todos.where((t) => t.dirty).length;

  int? get currentWeek => weekOf(week1Monday, clock.date, totalWeeks);

  Map<Section, List<Todo>> get sections => partition(todos, now);

  /// 整学期的节次骨架：同一张大节里不同课的开始时刻可能差几分钟，
  /// 所以取「这个节次标签下的最早开始 ~ 最晚结束」。骨架不跟着本周走 ——
  /// 某节这周恰好没课，那一行还在，只是写着「空闲」（与桌面 §8.8 第 7 条同一条理由）。
  List<({String label, int start, int end})> get periodSkeleton {
    final byLabel = <String, ({String label, int start, int end})>{};
    for (final c in classes) {
      if (c.deletedAt != null) continue;
      final key = c.period.isEmpty ? '第 ${c.startMin ~/ 60} 节' : c.period;
      final cur = byLabel[key];
      byLabel[key] = cur == null
          ? (label: key, start: c.startMin, end: c.endMin)
          : (
              label: key,
              start: c.startMin < cur.start ? c.startMin : cur.start,
              end: c.endMin > cur.end ? c.endMin : cur.end,
            );
    }
    return byLabel.values.toList()..sort((a, b) => a.start.compareTo(b.start));
  }

  /// 今天这一天的课（已按本周过滤）
  List<ClassRow> classesToday() {
    final week = currentWeek;
    if (week == null) return const [];
    return classesOn(classes, clock.dow, week);
  }

  /// 某天这一天的课。7 日条切到别的日子时用；`week` 只在课表屏翻到别周时给，
  /// 不给就是本周（[currentWeek]）。
  List<ClassRow> classesOnDay(int dow, {int? week}) {
    final w = week ?? currentWeek;
    if (w == null) return const [];
    return classesOn(classes, dow, w);
  }

  /// 今天这一天的时间形状。**今天屏那行与课表屏底部那行共用它**（§5.4：同值同函数），
  /// 两种情况不能共写一句：
  /// - 今天没课 / 还没同步到课表 → 各说一句，别说「今天没有课」冒充已知；
  /// - 还有下一节 → 报最近那段空档的起止；
  /// - 最后一节已经上完 → 说「课已经上完」。这时候 `nextFreeGap` 也返回 null，
  ///   但那是放学不是有空，把两种情况写成一句就是谎（桌面 §8.8 第 7 条同一件事）。
  ({
    int classes,
    ({int start, int end})? gap,
    bool noTimetable,
    bool afterLast,
  }) todayShape() {
    if (classes.isEmpty) {
      return (classes: 0, gap: null, noTimetable: true, afterLast: false);
    }
    if (currentWeek == null) {
      // 开学日没设：周次算不出来，也就无从知道「今天」有没有课
      return (classes: 0, gap: null, noTimetable: true, afterLast: false);
    }
    final rows = classesToday();
    if (rows.isEmpty) {
      return (classes: 0, gap: null, noTimetable: false, afterLast: false);
    }
    final nowMin = minutesOfDay(clock.date);
    final gap = nextFreeGap(
      [for (final r in rows) (startMin: r.startMin, endMin: r.endMin)],
      nowMin,
    );
    final lastEnd = rows.map((r) => r.endMin).reduce((a, b) => a > b ? a : b);
    return (
      classes: rows.length,
      gap: gap,
      noTimetable: false,
      afterLast: gap == null && nowMin >= lastEnd,
    );
  }

  // ── 手机上能做的两件事：勾选、新建（§1 的范围，别再往下加） ──

  /// 勾选 / 取消勾选。取消勾选**不清空 doneAt**（PLAN §5.1），只推 rev。
  Future<void> setDone(String id, bool done) async {
    final i = todos.indexWhere((t) => t.id == id);
    if (i < 0) return;
    final next = todos[i].copyWith(
      completed: done,
      // 完成时刻只在勾上的那一刻写；取消时留着旧值，统计那格不能空
      doneAt: done ? now : null,
      dirty: true,
      updatedAt: now,
      updatedBy: repo?.deviceId,
    );
    todos = [...todos]..[i] = next;
    notifyListeners();
    await repo?.saveDone(next);
  }

  /// 新建。没有截止时间 → 落「随时」（§5.3 的第三条规则），不在这里偷偷替用户选今天。
  Future<Todo> create(String title) async {
    final trimmed = title.trim();
    final dev = repo?.deviceId ?? 'phone-a';
    final maxKey =
        todos.isEmpty ? 0.0 : todos.map((t) => t.sortKey).reduce((a, b) => a > b ? a : b);
    final t = Todo(
      id: '$dev-${now.toRadixString(36)}-${Random().nextInt(1 << 32).toRadixString(16)}',
      title: trimmed,
      createdAt: now,
      createdBy: dev,
      updatedAt: now,
      updatedBy: dev,
      sortKey: maxKey + 1024,
      zone: 'inbox',
      dirty: true,
    );
    todos = [...todos, t];
    notifyListeners();
    await repo?.insertTodo(t);
    return t;
  }

  /// 手动同步（设置页那颗）。打开 app 时也走同一条。
  /// 没有服务（演示/截图）时什么都不做 —— 不去画一个按下去什么都不做的按钮。
  Future<void> syncNow() async {
    final s = syncer;
    if (s == null || syncing) return;
    syncing = true;
    notifyListeners();
    try {
      await s.run();
    } finally {
      syncing = false;
    }
    await reload();
  }

  /// 今天用药计划；没有药箱视图或没有药时返回 null（界面据此不画卡片）。
  TodayPlan? get todayMedicine {
    final m = medicine;
    if (m == null || m.isEmpty) return null;
    return m.todayPlan(clock.date);
  }

  /// 关掉即停：不再发请求，也不画卡片（PLAN-MOBILE §5.5 的隐藏即停）。
  Future<void> setMedicineEnabled(bool on) async {
    medicineEnabled = on;
    notifyListeners();
    await repo?.setMedicineEnabled(on, now);
    if (on) await refreshMedicine();
  }

  /// 读一次药箱云端。没配凭据时静默返回 —— 设置页会说「先填账号」。
  Future<void> refreshMedicine() async {
    final s = medicineService;
    if (s == null || !medicineEnabled || medicineFetching) return;
    if (!await s.configured) return;
    medicineFetching = true;
    notifyListeners();
    try {
      await s.refresh();
    } catch (_) {
      // 失败已经写进 medicine.lastError；旧视图保留，界面说「这次没读到」
    } finally {
      medicineFetching = false;
    }
    await _reloadMedicine();
  }

  Future<void> _reloadMedicine() async {
    final r = repo;
    if (r != null) {
      medicineEnabled = await r.medicineEnabled();
      medicine = await r.medicineView();
      medicineAccountEmail = await r.medicineEmail();
      medicineConfigured =
          medicineService != null && await medicineService!.configured;
      final err = await r.medicineError();
      medicineError = err.error;
      medicineErrorAt = err.at;
    }
    notifyListeners();
  }
}
