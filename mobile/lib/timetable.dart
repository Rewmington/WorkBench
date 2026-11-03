/// 课表的显示侧计算 —— 桌面 `src/lib/timetable.ts` 的 Dart 移植。
///
/// 「最近的空档」在手机上也只有这一个实现：今天屏那行摘要与课表屏共用它
/// （PLAN-MOBILE §5.3、§5.4；桌面 PLAN §8.8 第 7 条同一条规则）。
/// 两边各写一遍迟早给出两个答案，所以这里对着 TS 逐条搬，连它注释里的三条一起。
library;

/// 空档至少这么长才算「有空」。界面上要把这个数写出来（§2 原则 5：不藏着门槛）。
const int minGapMinutes = 30;

/// 本地自然日整数 yyyymmdd，与 Rust 的 `days::day_of_ms`、前端的 `dayInt` 同语义。
int dayInt(DateTime d) => d.year * 10000 + d.month * 100 + d.day;

/// 两个 yyyymmdd 之间的自然日数。**不能整数相减**：20261029 - 20260930 = 99，真实是 29。
int daysBetween(int from, int to) {
  final a = DateTime.utc(from ~/ 10000, (from ~/ 100) % 100, from % 100);
  final b = DateTime.utc(to ~/ 10000, (to ~/ 100) % 100, to % 100);
  return b.difference(a).inDays;
}

/// `yyyy-mm-dd` → yyyymmdd；**格式不对时返回 null，不是抛异常**。
///
/// 这个值可能来自同步快照的 meta（`saveMeta` 已经挡一道，但本机库里也可能躺着旧版本
/// 或手改留下的坏值）。它今天是「算不算得出第几周」的输入，不该是一条 UI 崩溃的入口：
/// 一个 `"x"` 让 `int.parse` 抛掉整个 Today 屏，代价远大于那一格显示不出来。
int? isoToDayOrNull(String iso) {
  final parts = iso.trim().split('-');
  if (parts.length != 3 || parts[0].length != 4) return null;
  final y = int.tryParse(parts[0]);
  final mo = int.tryParse(parts[1]);
  final d = int.tryParse(parts[2]);
  if (y == null || mo == null || d == null || mo < 1 || mo > 12 || d < 1 || d > 31) {
    return null;
  }
  // 日历里没有的那一天（2026-02-31）会被 `DateTime.utc` 顺到 3 月，不能照字面收下；
  // 桌面 `iso_to_day` 用 `from_ymd_opt` 拒掉，这里对齐同一条口径。
  final day = DateTime.utc(y, mo, d);
  if (day.year != y || day.month != mo || day.day != d) return null;
  return y * 10000 + mo * 100 + d;
}

/// 今天在第几周。开学日没设 / 还没开学 / 学期已结束 → null，
/// 而不是 0 或者硬猜 —— 猜错会把一整个学期的课表显示成另一周的样子。
int? weekOf(String? week1Monday, DateTime today, int? totalWeeks) {
  if (week1Monday == null || totalWeeks == null) return null;
  final start = isoToDayOrNull(week1Monday);
  if (start == null) return null;
  final elapsed = daysBetween(start, dayInt(today));
  if (elapsed < 0) return null;
  final week = elapsed ~/ 7 + 1;
  return week <= totalWeeks ? week : null;
}

/// 分钟数 → `08:30`。
String hhmm(int min) =>
    '${(min ~/ 60).toString().padLeft(2, '0')}:${(min % 60).toString().padLeft(2, '0')}';

/// 一分钟的时刻，当 [nextFreeGap] 的 nowMin 用。
int minutesOfDay(DateTime d) => d.hour * 60 + d.minute;

/// 当前时刻之后、长度 ≥ [minGapMinutes] 的第一段空档；没有就 null。
///
/// 三件事都得对：已经上完的课不能把游标推回去；**重叠的课要合成一段**（调课很常见，
/// 不合并就会凭空造出一段不存在的空档）；最后一节之后不算空档 —— 那是放学，不是「有空」。
({int start, int end})? nextFreeGap(List<({int startMin, int endMin})> spans, int nowMin) {
  final sorted = [...spans]..sort((a, b) => a.startMin.compareTo(b.startMin));
  var cursor = nowMin;
  for (final s in sorted) {
    if (s.endMin <= cursor) continue;
    if (s.startMin - cursor >= minGapMinutes) return (start: cursor, end: s.startMin);
    cursor = cursor > s.endMin ? cursor : s.endMin;
  }
  return null;
}

/// 周次集合命中判断，与 Rust 的 `repos::timetable::weeks_include` 同一条规则：
/// `"1-10,12-16"` 或 `"1,3,5,7,9"`。规范化在 PC 导入时就做完了，这里只管读。
bool weeksInclude(String spec, int week) {
  for (final raw in spec.split(',')) {
    final part = raw.trim();
    final dash = part.indexOf('-');
    if (dash > 0) {
      final a = int.tryParse(part.substring(0, dash));
      final b = int.tryParse(part.substring(dash + 1));
      if (a != null && b != null && a <= week && week <= b) return true;
    } else if (int.tryParse(part) == week) {
      return true;
    }
  }
  return false;
}

/// 周次集合里最大的那一周，与 Rust `repos::timetable::weeks_high` 同一条规则：
/// 每段取最后一个数（`1-10` → 10，`12` → 12），再取最大。
int? weeksHigh(String spec) {
  int? max;
  for (final part in spec.split(',')) {
    final n = int.tryParse(part.trim().split('-').last.trim());
    if (n != null && (max == null || n > max)) max = n;
  }
  return max;
}

/// 课表里的一行 = 一个「星期 × 节次 × 周次集合」的上课时段，不是一门课。
/// 字段与 `classes` 的列一一对应（快照里那行 JSON 的键就是这些蛇形名）。
class ClassRow {
  final String id;
  final String term;
  final String name;
  final String teacher;
  final String room;

  /// 1 = 周一 … 7 = 周日
  final int day;
  final int startMin;
  final int endMin;

  /// 节次标签原样（如「第一大节 [01-02]节」），界面直接显示，不自己编号
  final String period;
  final String weeks;

  /// 软删墓碑；同步靠它传播删除，所以模型里必须留着它
  final int? deletedAt;

  const ClassRow({
    required this.id,
    required this.term,
    required this.name,
    required this.teacher,
    required this.room,
    required this.day,
    required this.startMin,
    required this.endMin,
    required this.period,
    required this.weeks,
    this.deletedAt,
  });
}

/// 某天本周实际上的一批课，按开始时间排。
List<ClassRow> classesOn(List<ClassRow> all, int dow, int week) => [
      for (final c in all)
        if (c.deletedAt == null && c.day == dow && weeksInclude(c.weeks, week)) c,
    ]..sort((a, b) => a.startMin.compareTo(b.startMin));
