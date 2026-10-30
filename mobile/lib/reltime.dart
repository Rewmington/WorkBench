/// 时间文案 —— 桌面 `src/lib/reltime.ts` 里手机端要用的那几条的移植。
///
/// 措辞必须与桌面一字不差：同一条待办在两个屏幕上说同一句话，
/// 否则「一眼是同一个产品」（PLAN-MOBILE §5.1）就只剩颜色像。
library;

import 'timetable.dart';

const int _minute = 60000;
const int _hour = 3600000;
const int _day = 86400000;

String _two(int n) => n.toString().padLeft(2, '0');

/// `14:05`。
String clockTime(int ms) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  return '${_two(d.hour)}:${_two(d.minute)}';
}

/// 相对日的说法。带时刻的那几条与桌面同一形状（「今天 18:00」而不是「今天」）。
String dayWordLabel(int now, int due) {
  final days = daysBetween(dayInt(DateTime.fromMillisecondsSinceEpoch(now)),
      dayInt(DateTime.fromMillisecondsSinceEpoch(due)));
  if (days == 0) return '今天 ${clockTime(due)}';
  if (days == 1) return '明天 ${clockTime(due)}';
  if (days == 2) return '后天 ${clockTime(due)}';
  if (days == -1) return '昨天 ${clockTime(due)}';
  final d = DateTime.fromMillisecondsSinceEpoch(due);
  final sameYear = d.year == DateTime.fromMillisecondsSinceEpoch(now).year;
  return sameYear ? '${d.month}月${d.day}日' : '${d.year}年${d.month}月${d.day}日';
}

/// 逾期时长。没逾期返回 null，让调用方去显示「今天 18:00」这类正常到期文案 ——
/// 状态不只靠颜色（§2 原则 3），所以逾期的表达是文字 + 图标，不是一片红。
String? overdueLabel(int now, int? due) {
  if (due == null || now <= due) return null;
  final diff = now - due;
  if (diff < _hour) return '逾期 ${(diff ~/ _minute).clamp(1, 1 << 30)} 分钟';
  if (diff < _day && dayInt(DateTime.fromMillisecondsSinceEpoch(now)) ==
      dayInt(DateTime.fromMillisecondsSinceEpoch(due))) {
    return '逾期 ${diff ~/ _hour} 小时';
  }
  return '逾期 ${daysBetween(dayInt(DateTime.fromMillisecondsSinceEpoch(due)),
          dayInt(DateTime.fromMillisecondsSinceEpoch(now))).clamp(1, 1 << 30)} 天';
}

/// 到期文案：未逾期走「今天/明天/后天/N 月 N 日」，逾期走 [overdueLabel]。
String dueLabel(int now, int? due) {
  final over = overdueLabel(now, due);
  if (over != null) return over;
  if (due == null) return '没定日子';
  return dayWordLabel(now, due);
}

/// 完成那一行的说法。今天完成给时刻，昨天与更早给日 —— 「11:20 完成」比「今天完成」有用。
String doneLabel(int now, int? doneAt) {
  if (doneAt == null) return '已完成';
  final days = daysBetween(dayInt(DateTime.fromMillisecondsSinceEpoch(doneAt)),
      dayInt(DateTime.fromMillisecondsSinceEpoch(now)));
  if (days == 0) return '${clockTime(doneAt)} 完成';
  if (days == 1) return '昨天完成';
  return '${DateTime.fromMillisecondsSinceEpoch(doneAt).month}月'
      '${DateTime.fromMillisecondsSinceEpoch(doneAt).day}日完成';
}

/// 「N 分钟前」，同步状态那行用。超过一小时就不装精确了。
String agoLabel(int now, int? then) {
  if (then == null) return '还没同步过';
  final diff = now - then;
  if (diff < _minute) return '刚刚';
  if (diff < _hour) return '${diff ~/ _minute} 分钟前';
  return clockTime(then);
}
