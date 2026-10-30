/// 待办的本机模型与分区规则。
///
/// 列名与 `todos` 表一致（同步行 JSON 的键就是这些蛇形名），这样 M11d 接 SQLite 时
/// 一侧的字段不需要翻译。`dirty` 是**本机**那一行还没传出去的标记，
/// 它上不了云，但手机上要有它 —— §5.5 那个行末小黄点读的就是它。
library;

import 'reltime.dart';
import 'timetable.dart';

/// §5.3 的分区顺序：逾期 → 今天 → 随时 → 已完成。
enum Section { late, today, any, done }

extension SectionName on Section {
  String get label => switch (this) {
        Section.late => '逾期',
        Section.today => '今天',
        Section.any => '随时',
        Section.done => '已完成',
      };
}

/// 优先级 0 最急（与 Rust `model::todo::priority` 同一套编号）。
/// 界面只在「需要你先看到」时挂角标：普通与低挂出来是噪音，
/// 而紧急/高两个词本身已经把差别说清了 —— 状态不只靠颜色（§2 原则 3）。
String? priorityLabel(int p) => switch (p) {
      0 => '紧急',
      1 => '高',
      _ => null,
    };

class Todo {
  final String id;
  final int rev;
  final int createdAt;
  final String createdBy;
  final int updatedAt;
  final String updatedBy;
  final int? deletedAt;

  final String title;
  final String note;
  final int? doneAt;
  final int? dueAt;
  final int priority;
  final bool pinned;
  final double sortKey;
  final String zone;
  final bool completed;

  /// 本机写入且还没传出去（行末小黄点）
  final bool dirty;

  const Todo({
    required this.id,
    this.rev = 1,
    this.createdAt = 0,
    this.createdBy = '',
    this.updatedAt = 0,
    this.updatedBy = '',
    this.deletedAt,
    required this.title,
    this.note = '',
    this.doneAt,
    this.dueAt,
    this.priority = 2,
    this.pinned = false,
    this.sortKey = 0,
    this.zone = 'inbox',
    this.completed = false,
    this.dirty = false,
  });

  /// 从 SQLite 那行读出来。列名就是快照 JSON 的键（§3.2），所以这里没有第二套命名。
  factory Todo.fromRow(Map<String, Object?> r) => Todo(
        id: r['id'] as String,
        rev: (r['rev'] as int?) ?? 1,
        createdAt: (r['created_at'] as int?) ?? 0,
        createdBy: (r['created_by'] as String?) ?? '',
        updatedAt: (r['updated_at'] as int?) ?? 0,
        updatedBy: (r['updated_by'] as String?) ?? '',
        deletedAt: r['deleted_at'] as int?,
        title: (r['title'] as String?) ?? '',
        note: (r['note'] as String?) ?? '',
        doneAt: r['done_at'] as int?,
        dueAt: r['due_at'] as int?,
        priority: (r['priority'] as int?) ?? 2,
        pinned: ((r['pinned'] as int?) ?? 0) != 0,
        sortKey: ((r['sort_key'] as num?) ?? 0).toDouble(),
        zone: (r['zone'] as String?) ?? 'inbox',
        completed: ((r['completed'] as int?) ?? 0) != 0,
        dirty: ((r['dirty'] as int?) ?? 0) != 0,
      );

  Todo copyWith({
    bool? completed,
    int? doneAt,
    bool? dirty,
    int? updatedAt,
    String? updatedBy,
  }) =>
      Todo(
        id: id,
        rev: rev + 1,
        createdAt: createdAt,
        createdBy: createdBy,
        updatedAt: updatedAt ?? this.updatedAt,
        updatedBy: updatedBy ?? this.updatedBy,
        deletedAt: deletedAt,
        title: title,
        note: note,
        doneAt: doneAt ?? this.doneAt,
        dueAt: dueAt,
        priority: priority,
        pinned: pinned,
        sortKey: sortKey,
        zone: zone,
        completed: completed ?? this.completed,
        dirty: dirty ?? this.dirty,
      );

  bool get isTombstone => deletedAt != null;

  /// 这一条落在哪个区。
  ///
  /// 规则与桌面同一条，不是照抄界面上的四个词：
  /// - 勾掉的进已完成（`completed`，不是 `done_at` —— 见 PLAN §5.1 那条 0002 的教训）；
  /// - **逾期独立成区**，所以逾期的那条不会再出现在「今天」；
  /// - 「今天」= 到期在今天，**或用户自己把它放进了 `today` 区**（桌面 `todos::focus`
  ///   的判据就是这个或：`due_at <= 今日末 OR zone = 'today'`）。没有截止时间又没被
  ///   放进 today 区的，落「随时」，不混进今天（§5.3）。
  /// - 有截止时间但在明天以后 → 也归「随时」，靠行末那句「明天 18:00」表达真实到期，
  ///   手机上不再造一个「以后」区（导航只有两项、这一屏只问今天该干什么）。
  Section sectionOf(int now) {
    if (completed) return Section.done;
    if (dueAt == null) return zone == 'today' ? Section.today : Section.any;
    if (now > dueAt!) return Section.late;
    if (dayInt(DateTime.fromMillisecondsSinceEpoch(dueAt!)) ==
        dayInt(DateTime.fromMillisecondsSinceEpoch(now))) {
      return Section.today;
    }
    return Section.any;
  }

  /// 行末那行小字。没到期时间但被放进了 today 区，就说「今天」——
  /// 这是用户自己给的判断，不是界面替它编的时刻。
  String subtitle(int now) {
    if (completed) return doneLabel(now, doneAt);
    if (dueAt != null) return dueLabel(now, dueAt);
    return zone == 'today' ? '今天' : '没定日子';
  }
}

/// 一屏怎么摆：分区顺序固定，区内顺序与桌面同一句 SQL 一致（置顶优先，再按拖拽键）。
Map<Section, List<Todo>> partition(List<Todo> all, int now) {
  final live = all.where((t) => !t.isTombstone).toList()
    ..sort((a, b) {
      if (a.pinned != b.pinned) return a.pinned ? -1 : 1;
      return a.sortKey.compareTo(b.sortKey);
    });
  return {
    for (final s in Section.values)
      s: [for (final t in live) if (t.sectionOf(now) == s) t],
  };
}
