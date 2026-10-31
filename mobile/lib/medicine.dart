/// 药箱的**只读**视图模型。规则逐条照 `D:\ADevelop\Repositories\Mine\medicine`
/// 的 `lib/models.dart` 与 `lib/app.dart` 搬，不自己发明一套：
///
/// - 库存唯一真相是 `remaining`（个数），`boxes` 只是展示量；
/// - `daysLeft = remaining / (dosesPerDay * amountPerDose)`，null = 没设用量；
/// - 服药时间点来自 `times`（"08:00,20:00"），为空时按次数在 7:30–21:00 均匀生成；
/// - 打卡按「就近优先」匹配到时间槽，0 剂量记录 = 明确标记「这顿没吃」；
/// - 今天的分母扣掉「没吃」的格子（药箱 `todayProgress` 同一条）。
///
/// 药箱是**外部程序的只读源**：这里只解析它同步到坚果云的 JSON，绝不写回。
library;

/// 一条药品记录，字段名与药箱 `medicines` 表一一对应（云快照里就是这些蛇形键）。
class Medicine {
  final String id;
  final String name;
  final double perBox;
  final double remaining;
  final double dosesPerDay;
  final double amountPerDose;
  final String unit;
  final int lowDays;
  final String note;
  final String times;
  final int createdAt;
  final int updatedAt;
  final bool deleted;

  const Medicine({
    required this.id,
    required this.name,
    required this.perBox,
    required this.remaining,
    required this.dosesPerDay,
    required this.amountPerDose,
    required this.unit,
    required this.lowDays,
    required this.note,
    required this.times,
    required this.createdAt,
    required this.updatedAt,
    required this.deleted,
  });

  factory Medicine.fromRow(Map<String, Object?> r) {
    final perBox = (r['per_box'] as num?)?.toDouble() ?? 0;
    final boxes = (r['boxes'] as num?)?.toDouble() ?? 0;
    return Medicine(
      id: r['id'] as String,
      name: (r['name'] as String?) ?? '',
      perBox: perBox,
      remaining: (r['remaining'] as num?)?.toDouble() ?? boxes * perBox,
      dosesPerDay: (r['doses_per_day'] as num?)?.toDouble() ?? 0,
      amountPerDose: (r['amount_per_dose'] as num?)?.toDouble() ?? 0,
      unit: (r['unit'] as String?) ?? '片',
      lowDays: (r['low_days'] as num?)?.toInt() ?? 7,
      note: (r['note'] as String?) ?? '',
      times: (r['times'] as String?) ?? '',
      createdAt: (r['created_at'] as num?)?.toInt() ?? 0,
      updatedAt: (r['updated_at'] as num?)?.toInt() ?? 0,
      deleted: ((r['deleted'] as num?) ?? 0) != 0,
    );
  }

  double get dailyUse => dosesPerDay * amountPerDose;

  /// 还能吃几天；null 表示未设置用量（无法计算）。
  double? get daysLeft {
    if (dailyUse <= 0) return null;
    return remaining / dailyUse;
  }

  bool get isLow {
    final d = daysLeft;
    return d != null && d <= lowDays;
  }

  bool get isOut => remaining <= 0;

  String get doseLabel =>
      '每天 ${_trim(dosesPerDay)} 次 × 每次 ${_trim(amountPerDose)} $unit';

  /// 服药时间点；times 为空时按次数在 7:30–21:00 均匀生成（与药箱同一段代码）。
  List<int> get scheduleMinutes {
    final parsed = times
        .split(RegExp(r'[,，;；\s]+'))
        .where((s) => s.contains(':'))
        .map((s) {
          final parts = s.split(':');
          if (parts.length != 2) return null;
          final h = int.tryParse(parts[0]);
          final m = int.tryParse(parts[1]);
          if (h == null || m == null || h > 23 || m > 59) return null;
          return h * 60 + m;
        })
        .whereType<int>()
        .toList()
      ..sort();
    if (parsed.isNotEmpty) return parsed;
    final n = dosesPerDay.round();
    if (n <= 0) return const [];
    if (n == 1) return const [8 * 60];
    const start = 7 * 60 + 30;
    const end = 21 * 60;
    return List.generate(n, (i) => (start + (end - start) * i / (n - 1)).round());
  }

  static String _trim(double v) =>
      v == v.roundToDouble() ? v.toInt().toString() : v.toStringAsFixed(2);
}

/// 库存顺序：快没的那支排最前。口径照药箱自己的统计页（`stats_page.dart`）——
/// 算不出天数的记成 9999 沉底、同值按名字；桌面端 Rust 的 `by_stock_risk` 是同一条。
/// 两边排出来的顺序得一样，否则用户对照着看会以为其中一边算错了。
int byStockRisk(Medicine a, Medicine b) {
  const unknown = 9999.0;
  final d = (a.daysLeft ?? unknown).compareTo(b.daysLeft ?? unknown);
  return d != 0 ? d : a.name.compareTo(b.name);
}

/// 一段窗口里的服药计数。字段与算法都照药箱 `PeriodStats`（`models.dart`）。
class PeriodStats {
  /// 本周这一个窗口长度：桌面 Rust 的 `WEEK_DAYS` 与这里必须是一个数。
  static const weekDays = 7;

  final int taken;

  /// 已到期、且没被标记「这顿没吃」的格子数
  final int owed;
  final int skipped;

  const PeriodStats({required this.taken, required this.owed, required this.skipped});

  /// 药箱 `PeriodStats.rate` 原样：应服为 0 时，只有整窗都标了没吃才算满，
  /// 压根没有安排就是 0 —— 没有安排不能吹成 100% 依从。
  double get rate =>
      owed == 0 ? (skipped > 0 ? 1.0 : 0.0) : (taken / owed).clamp(0.0, 1.0);

  /// false 时那个 0 不是「漏吃」，界面上不能画一个空环骗人。
  bool get hasPlan => owed > 0 || skipped > 0;
}

/// 一次服药打卡。0 剂量 = 标记「这顿没吃」。
class DoseLog {
  final String id;
  final String medicineId;
  final int takenAt;
  final double amount;
  final String note;
  final int updatedAt;
  final bool deleted;

  const DoseLog({
    required this.id,
    required this.medicineId,
    required this.takenAt,
    required this.amount,
    required this.note,
    required this.updatedAt,
    required this.deleted,
  });

  factory DoseLog.fromRow(Map<String, Object?> r) => DoseLog(
        id: r['id'] as String,
        medicineId: (r['medicine_id'] as String?) ?? '',
        takenAt: (r['taken_at'] as num?)?.toInt() ?? 0,
        amount: (r['amount'] as num?)?.toDouble() ?? 0,
        note: (r['note'] as String?) ?? '',
        updatedAt: (r['updated_at'] as num?)?.toInt() ?? 0,
        deleted: ((r['deleted'] as num?) ?? 0) != 0,
      );

  bool get isSkipped => amount <= 0;
}

String formatMinutes(int m) =>
    '${(m ~/ 60).toString().padLeft(2, '0')}:${(m % 60).toString().padLeft(2, '0')}';

/// 把打卡时刻一对一分配给当天的时间点槽位；**就近优先**，不是从早到晚依次点亮。
/// 只打了 20:00 的卡时点亮 20:00 那一格，而不是最早那一格 —— 药箱 `matchSlots` 原样。
List<int?> matchSlots(List<int> slotMinutes, List<int> takenAts, int dayStartMs) {
  final slots = [for (final m in slotMinutes) dayStartMs + m * 60000];
  final result = List<int?>.filled(slots.length, null);
  final used = List<bool>.filled(slots.length, false);
  final times = [...takenAts]..sort();
  for (final t in times) {
    var best = -1;
    var bestDist = double.infinity;
    for (var i = 0; i < slots.length; i++) {
      if (used[i]) continue;
      final d = (slots[i] - t).abs().toDouble();
      if (d < bestDist) {
        bestDist = d;
        best = i;
      }
    }
    if (best >= 0) {
      used[best] = true;
      result[best] = t;
    }
  }
  return result;
}

/// 一台设备发布的那份 `device-<id>.json`。
class MedicineDoc {
  final String device;
  final int publishedAt;
  final List<Map<String, Object?>> medicines;
  final List<Map<String, Object?>> logs;

  const MedicineDoc({
    required this.device,
    required this.publishedAt,
    required this.medicines,
    required this.logs,
  });

  factory MedicineDoc.fromJson(Map<String, Object?> json) {
    List<Map<String, Object?>> rows(Object? v) => [
          for (final row in (v as List? ?? const []))
            if (row is Map<String, Object?>) row,
        ];
    return MedicineDoc(
      device: (json['device'] as String?) ?? '',
      publishedAt: (json['published_at'] as num?)?.toInt() ?? 0,
      medicines: rows(json['medicines']),
      logs: rows(json['dose_logs']),
    );
  }
}

/// 今天的一个服药格子。
class DoseSlot {
  final Medicine medicine;
  final int minute;
  final bool taken;
  final bool skipped;
  final int? takenAtMs;

  const DoseSlot({
    required this.medicine,
    required this.minute,
    required this.taken,
    required this.skipped,
    this.takenAtMs,
  });
}

/// 合并后的只读视图。`fetchedAt` 是本机最近一次读云端的时间；
/// `publishedAt` 是药箱最近一次发布的时间 —— 两个时间都必须能说出来。
class MedicineView {
  final List<Medicine> medicines;
  final List<DoseLog> logs;
  final int? publishedAt;
  final int devices;
  final int? fetchedAt;

  const MedicineView({
    required this.medicines,
    required this.logs,
    required this.publishedAt,
    required this.devices,
    this.fetchedAt,
  });

  bool get isEmpty => medicines.isEmpty;

  /// 按还剩几天升序的那份清单（口径见 [byStockRisk]）：快没的排最前。
  ///
  /// 排在渲染这一层而不是 `merge` 里：演示数据与本机缓存回来的那份都不走 merge，
  /// 放在这里才不管数据从哪来都是同一个顺序。
  List<Medicine> get byStock => [...medicines]..sort(byStockRisk);

  /// 多台设备各发一份全量。规则与药箱一致：同一 id 取 `updated_at` 更新的那条；
  /// 平票时取发布更晚的那份、再按设备 id，保证每次算出来一样（药箱那侧平票保留本机，
  /// 但这里是只读视图，必须有一个与读取顺序无关的答案）。
  factory MedicineView.merge(List<MedicineDoc> docs, {int? fetchedAt}) {
    final ordered = [...docs]..sort((a, b) {
        final p = a.publishedAt.compareTo(b.publishedAt);
        return p != 0 ? p : a.device.compareTo(b.device);
      });

    final medicines = <String, Map<String, Object?>>{};
    final medAt = <String, int>{};
    final logs = <String, Map<String, Object?>>{};
    final logAt = <String, int>{};

    for (final doc in ordered) {
      for (final row in doc.medicines) {
        final id = row['id'];
        if (id is! String) continue;
        final at = (row['updated_at'] as num?)?.toInt() ?? 0;
        if (medAt[id] == null || at >= medAt[id]!) {
          medicines[id] = row;
          medAt[id] = at;
        }
      }
      for (final row in doc.logs) {
        final id = row['id'];
        if (id is! String) continue;
        final at = (row['updated_at'] as num?)?.toInt() ?? 0;
        if (logAt[id] == null || at >= logAt[id]!) {
          logs[id] = row;
          logAt[id] = at;
        }
      }
    }

    final active = [
      for (final row in medicines.values)
        if (((row['deleted'] as num?) ?? 0) == 0) Medicine.fromRow(row),
    ];
    final activeIds = {for (final m in active) m.id};
    final visibleLogs = [
      for (final row in logs.values)
        if (((row['deleted'] as num?) ?? 0) == 0)
          if (activeIds.contains((row['medicine_id'] as String?) ?? ''))
            DoseLog.fromRow(row),
    ]..sort((a, b) => a.takenAt.compareTo(b.takenAt));

    final published = docs.isEmpty
        ? null
        : docs.map((d) => d.publishedAt).reduce((a, b) => a > b ? a : b);

    return MedicineView(
      medicines: active,
      logs: visibleLogs,
      publishedAt: published == 0 ? null : published,
      devices: docs.length,
      fetchedAt: fetchedAt,
    );
  }

  Map<String, Object?> toJson() => {
        'fetchedAt': fetchedAt,
        'publishedAt': publishedAt,
        'devices': devices,
        // 存合并后的原始行：与云端同形状，下次读缓存不必再解析一遍
        'medicines': [
          for (final m in medicines)
            {
              'id': m.id,
              'name': m.name,
              'per_box': m.perBox,
              'remaining': m.remaining,
              'doses_per_day': m.dosesPerDay,
              'amount_per_dose': m.amountPerDose,
              'unit': m.unit,
              'low_days': m.lowDays,
              'note': m.note,
              'times': m.times,
              'created_at': m.createdAt,
              'updated_at': m.updatedAt,
              'deleted': 0,
            },
        ],
        'dose_logs': [
          for (final l in logs)
            {
              'id': l.id,
              'medicine_id': l.medicineId,
              'taken_at': l.takenAt,
              'amount': l.amount,
              'note': l.note,
              'updated_at': l.updatedAt,
              'deleted': 0,
            },
        ],
      };

  factory MedicineView.fromJson(Map<String, Object?> json) {
    final meds = [
      for (final row in (json['medicines'] as List? ?? const []))
        if (row is Map<String, Object?>) Medicine.fromRow(row),
    ];
    final logs = [
      for (final row in (json['dose_logs'] as List? ?? const []))
        if (row is Map<String, Object?>) DoseLog.fromRow(row),
    ];
    return MedicineView(
      medicines: meds,
      logs: logs,
      publishedAt: (json['publishedAt'] as num?)?.toInt(),
      devices: (json['devices'] as num?)?.toInt() ?? 0,
      fetchedAt: (json['fetchedAt'] as num?)?.toInt(),
    );
  }

  /// 今日计划：药 × 时间点，按时间排序。规则与药箱 `todayPlan` 同一句。
  TodayPlan todayPlan(DateTime now) {    final dayStart = DateTime(now.year, now.month, now.day).millisecondsSinceEpoch;
    final dayEnd = dayStart + const Duration(days: 1).inMilliseconds;
    final slots = <DoseSlot>[];
    for (final m in medicines) {
      final minutes = m.scheduleMinutes;
      final todayLogs = logs
          .where((l) => l.medicineId == m.id && l.takenAt >= dayStart && l.takenAt < dayEnd)
          .toList()
        ..sort((a, b) => a.takenAt.compareTo(b.takenAt));
      final matched = matchSlots(minutes, [for (final l in todayLogs) l.takenAt], dayStart);
      for (var i = 0; i < minutes.length; i++) {
        final at = matched[i];
        final log = at == null ? null : todayLogs.where((l) => l.takenAt == at).firstOrNull;
        slots.add(DoseSlot(
          medicine: m,
          minute: minutes[i],
          taken: log != null && !log.isSkipped,
          skipped: log != null && log.isSkipped,
          takenAtMs: at,
        ));
      }
    }
    slots.sort((a, b) {
      final m = a.minute.compareTo(b.minute);
      return m != 0 ? m : a.medicine.name.compareTo(b.medicine.name);
    });
    return TodayPlan(slots, nowMinute: now.hour * 60 + now.minute);
  }

  /// 近 7 天的依从统计 = 药箱 `computeStats` 的直译，三条不显然的口径跟着一起搬：
  /// 建档之前的格子不算、今天只算已过点的格子、同一天多打几次封顶到格子数。
  /// 桌面端 Rust（`stats_over`）是同一条算法的另一份抄本 —— 同一个数在两个程序里得一样。
  PeriodStats weekStats(DateTime now) {
    final todayStart = DateTime(now.year, now.month, now.day);
    final nowMin = now.hour * 60 + now.minute;
    var taken = 0, owed = 0, skipped = 0;

    for (var back = PeriodStats.weekDays - 1; back >= 0; back--) {
      final dayStart = todayStart.subtract(Duration(days: back));
      final dayStartMs = dayStart.millisecondsSinceEpoch;
      final dayEndMs = dayStartMs + const Duration(days: 1).inMilliseconds;
      final isToday = back == 0;

      for (final m in medicines) {
        final slots = m.scheduleMinutes.where((min) {
          if (dayStartMs + min * 60000 < m.createdAt) return false;
          if (isToday && min > nowMin) return false;
          return true;
        }).toList();
        if (slots.isEmpty) continue;
        final mine = logs
            .where((l) =>
                l.medicineId == m.id && l.takenAt >= dayStartMs && l.takenAt < dayEndMs)
            .toList();
        final hits = mine.where((l) => !l.isSkipped).length;
        final marked = (mine.length - hits).clamp(0, slots.length);
        taken += hits > slots.length ? slots.length : hits;
        owed += slots.length - marked;
        skipped += marked;
      }
    }
    return PeriodStats(taken: taken, owed: owed, skipped: skipped);
  }

  /// 连续打卡天数 = 药箱 `streakDays` 原样：只看近 7 天里**有非零剂量记录**的连续日子。
  /// 今天还没打不算断（早上 8 点的药不该在凌晨就把连续天数清零），但也没提前 +1。
  int streakDays(DateTime now) {
    final today = DateTime(now.year, now.month, now.day);
    final dayCounts = List<int>.filled(PeriodStats.weekDays, 0);
    for (final l in logs) {
      if (l.isSkipped) continue; // 「这顿没吃」不算打卡
      final d = DateTime.fromMillisecondsSinceEpoch(l.takenAt);
      final diff = today.difference(DateTime(d.year, d.month, d.day)).inDays;
      if (diff >= 0 && diff < PeriodStats.weekDays) dayCounts[PeriodStats.weekDays - 1 - diff]++;
    }
    var i = PeriodStats.weekDays - 1;
    if (dayCounts[i] == 0) i--; // 今天还没打不算断
    var streak = 0;
    while (i >= 0 && dayCounts[i] > 0) {
      streak++;
      i--;
    }
    return streak;
  }
}

/// 今天的计划与进度。分母扣掉「没吃」的格子（药箱 `todayProgress` 同一条）。
class TodayPlan {
  final List<DoseSlot> slots;
  final int nowMinute;

  const TodayPlan(this.slots, {required this.nowMinute});

  Iterable<DoseSlot> get _open => slots.where((s) => !s.taken && !s.skipped);

  int get planned => slots.where((s) => !s.skipped).length;
  int get taken => slots.where((s) => s.taken).length;
  int get skipped => slots.where((s) => s.skipped).length;

  /// 过了 30 分钟还没记的格子（与药箱 `DayPlan.overdue` 同一条门槛）
  DoseSlot? get due {
    for (final s in _open) {
      if (s.minute + 30 < nowMinute) return s;
    }
    return null;
  }

  /// 还没到点的下一格
  DoseSlot? get next {
    for (final s in _open) {
      if (s.minute >= nowMinute) return s;
    }
    return null;
  }

  bool get allDone => planned > 0 && taken >= planned;
}

extension _FirstOrNull<T> on Iterable<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
