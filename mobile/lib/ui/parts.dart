/// 两屏共用的部件。每个尺寸都能在 `design/workbench-mobile-ui.html` 的那段 CSS 里
/// 找到同名的一条 —— 版式已经拍板（§5.3 / §5.4），这里不是重新设计，是搬运。
library;

import 'package:flutter/material.dart';

import '../palette.dart';
import '../reltime.dart';
import '../timetable.dart';
import '../store.dart';
import '../todo.dart';

/// 头部那一行小字：`10 月 30 日 · 周五 · 第 9 周`。
/// `at` 只在看别周时用（课表屏翻到第 N 周的那一天），不传就是今天 ——
/// 今天屏那条永远不该被别周带走。
String headDateLine(Clock clock, int? week, {DateTime? at}) {
  const dow = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
  final d = at ?? clock.date;
  final w = week == null ? '' : ' · 第 $week 周';
  return '${d.month} 月 ${d.day} 日 · ${dow[d.weekday - 1]}$w';
}

/// 同步那行。**四态四句**（§5.5），一句都不多写：
/// 不出现「已同步」、不出现转圈、真空态与「还没同步过」不共用一句。
class SyncLine extends StatelessWidget {
  final Store store;
  const SyncLine(this.store, {super.key});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final (text, color, warn) = switch (store.sync) {
      // 这一句比空态那句短：空态那一行会把「这里可能是空的」说完，两处别说同一句
      NeverSynced() => ('还没同步过', s.t3, false),
      final SyncedAt x => (
          '上次同步于 ${clockTime(x.atMs)} · ${store.pending} 条待上传',
          s.t3,
          false,
        ),
      final OfflineAt x => (
          '没连上坚果云 · 显示的是 ${clockTime(x.atMs)} 那份',
          s.amber,
          true,
        ),
    };
    return Container(
      margin: const EdgeInsets.only(bottom: 9),
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 7),
      decoration: BoxDecoration(
        color: s.surf2,
        borderRadius: BorderRadius.circular(Metrics.radiusSm),
      ),
      child: Row(
        children: [
          _Dot(color: warn ? s.amber : s.acc),
          const SizedBox(width: 7),
          Expanded(
            child: Text(text,
                style: TextStyle(fontSize: Type.metaWide, color: warn ? s.amber : s.t3)),
          ),
        ],
      ),
    );
  }
}

/// 空档那行。今天屏与课表屏**共用同一个函数**（§5.4：这两个值必须相等，
/// 桌面 M9c 是冻住时钟两边对句验的，这里由 `layout_spec_test.dart` 对同一件事）。
class GapLine extends StatelessWidget {
  final Store store;
  const GapLine(this.store, {super.key});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final t = store.todayShape();

    // 三种「说不出空档」的情况各一句话，不共用（共用就等于让"没同步到"
    // 长得像"今天真的没课"，那是 §5.5 点名的翻车方式）
    if (t.noTimetable) {
      return _line(context, '这台手机还没同步到课表');
    }
    if (t.classes == 0) {
      return _line(context, '今天没有课');
    }
    if (t.gap != null) {
      return Padding(
        padding: const EdgeInsets.only(left: 2, right: 2, bottom: 10),
        // Text.rich 而不是裸 RichText：界面文案要能被 find.text 查到，
        // 否则「两屏同一句」这种断言只能靠肉眼看图
        child: Text.rich(
          TextSpan(
            style: TextStyle(fontSize: Type.metaWide, color: s.t2),
            children: [
              TextSpan(text: '▦ 今天 ${t.classes} 节课 · 最近的空档 '),
              TextSpan(
                  text: '${hhmm(t.gap!.start)}–${hhmm(t.gap!.end)}',
                  style: TextStyle(color: s.t1, fontWeight: FontWeight.w700)),
            ],
          ),
        ),
      );
    }
    return _line(
      context,
      t.afterLast
          ? '▦ 今天 ${t.classes} 节课 · 课已经上完'
          // 门槛写在界面上（§2 原则 5：不藏着门槛），所以这句里的 30 不是修辞
          : '▦ 今天 ${t.classes} 节课 · 到下一节之前不足 $minGapMinutes 分钟',
    );
  }

  Widget _line(BuildContext context, String text) => Padding(
        padding: const EdgeInsets.only(left: 2, right: 2, bottom: 10),
        child: Text(text, style: TextStyle(fontSize: Type.metaWide, color: skinOf(context).t2)),
      );
}

/// 分区小标题 + 计数。逾期那一区的标题本身就是红的（文字也在喊，不只靠颜色 —— §2 原则 3）。
class SectionHead extends StatelessWidget {
  final Section section;
  final int count;
  const SectionHead(this.section, this.count, {super.key});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final late = section == Section.late;
    return Padding(
      padding: const EdgeInsets.only(left: 2, right: 2, top: 12, bottom: 6),
      child: Row(
        children: [
          Text(section.label.toUpperCase(),
              style: TextStyle(
                fontSize: Type.kicker,
                letterSpacing: .12,
                fontWeight: FontWeight.w700,
                color: late ? s.red : s.t3,
              )),
          const SizedBox(width: 6),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
            decoration: BoxDecoration(
              color: late ? s.redSoft : s.surf2,
              borderRadius: BorderRadius.circular(5),
            ),
            child: Text('$count',
                style: TextStyle(
                  fontSize: 10,
                  color: late ? s.red : s.t2,
                  fontFeatures: const [FontFeature.tabularFigures()],
                )),
          ),
        ],
      ),
    );
  }
}

/// 一条待办一行（行高 54，实测就是它）。黄点 = 这条还没传出去（§5.5 第三行）。
class TodoRow extends StatelessWidget {
  final Todo todo;
  final int now;
  final bool exiting;
  final VoidCallback onToggle;
  const TodoRow({
    required this.todo,
    required this.now,
    required this.onToggle,
    this.exiting = false,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final done = todo.completed;
    final label = priorityLabel(todo.priority);
    return Container(
      margin: const EdgeInsets.only(bottom: Metrics.rowGap),
      constraints: const BoxConstraints(minHeight: Metrics.rowH),
      // 上下各 3：54 - 6 = 48，正好把勾选框那块点击区整个装进去（行高这条不许让步）
      padding: const EdgeInsets.only(right: 9, top: 3, bottom: 3),
      decoration: BoxDecoration(
        color: s.surf,
        border: Border.all(color: s.line),
        borderRadius: BorderRadius.circular(Metrics.radiusSm),
      ),
      child: Row(
        // 勾选框画出来只有 21dp，但点击区必须 48×48 —— 触控下限说的是可点区域，不是视觉尺寸。
        // 代价是文字比对比稿右移 13dp（稿里 cb 在 9、文字在 41；这里 cb 在 9、文字在 54）：
        // 48 的点击区与 11 的缝隙放不下，宁可松一点也不给一个点不中的方块。
        children: [
          InkWell(
            onTap: onToggle,
            // 这块可点区是测试要量的对象（触控 ≥48dp 那条断言认它，不认 21dp 的视觉方块）
            key: ValueKey('cb:${todo.id}'),
            borderRadius: BorderRadius.circular(Metrics.radiusSm),
            child: SizedBox(
              width: Metrics.touch,
              height: Metrics.touch,
              child: Align(
                alignment: Alignment.centerLeft,
                child: Padding(
                  padding: const EdgeInsets.only(left: 9),
                  child: AnimatedOpacity(
                    opacity: exiting ? 0.35 : 1,
                    duration: const Duration(milliseconds: 180),
                    child: Container(
                      width: Metrics.checkbox,
                      height: Metrics.checkbox,
                      decoration: BoxDecoration(
                        color: done ? s.acc : Colors.transparent,
                        border: Border.all(color: done ? s.acc : s.line2, width: 1.5),
                        borderRadius: BorderRadius.circular(Metrics.checkRadius),
                      ),
                      child: done
                          ? Icon(Icons.check, size: 15, color: s.accInk)
                          : const SizedBox.shrink(),
                    ),
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(todo.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: Metrics.body,
                      color: done ? s.t3 : s.t1,
                      decoration: done ? TextDecoration.lineThrough : null,
                      decorationColor: s.t3,
                    )),
                const SizedBox(height: 2),
                Row(
                  children: [
                    if (label != null) ...[
                      Container(
                        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
                        decoration: BoxDecoration(
                          color: todo.priority == 0 || todo.priority == 1 ? s.redSoft : s.accSoft,
                          borderRadius: BorderRadius.circular(4),
                        ),
                        child: Text(label,
                            style: TextStyle(
                              fontSize: Type.chip,
                              fontWeight: FontWeight.w800,
                              letterSpacing: .04,
                              color: todo.priority == 0 || todo.priority == 1 ? s.red : s.acc,
                            )),
                      ),
                      const SizedBox(width: 6),
                    ],
                    Flexible(
                      child: Text(todo.subtitle(now),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: Type.meta, color: s.t3)),
                    ),
                  ],
                ),
              ],
            ),
          ),
          if (todo.dirty) ...[
            const SizedBox(width: 8),
            // 「这条还没传出去」只有一种表达：小黄点 + 一句 tooltip。它不是待办本身的状态
            Tooltip(
              message: '这条还没传出去',
              child: _Dot(color: s.amber, size: 7),
            ),
          ],
        ],
      ),
    );
  }
}

class _Dot extends StatelessWidget {
  final Color color;
  final double size;
  const _Dot({required this.color, this.size = 6});

  @override
  Widget build(BuildContext context) => Container(
        width: size,
        height: size,
        decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      );
}

/// 底部导航：**只有两项**（§5.2 的克制，超级课程表把查课表养成 86MB 平台是反面教材）。
class BottomNav extends StatelessWidget {
  final bool onTimetable;
  final ValueChanged<bool> onTap;
  const BottomNav({required this.onTimetable, required this.onTap, super.key});

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
            child: _navItem(context, Icons.check_circle_outline, '今天', !onTimetable,
                () => onTap(false))),
        const SizedBox(width: 6),
        Expanded(
            child:
                _navItem(context, Icons.grid_on, '课表', onTimetable, () => onTap(true))),
      ],
    );
  }

  // 图标走 Material 字体而不是 '☑' 这种 Unicode 字符：后者在 Windows/Android 上会被
  // 解析成彩色 emoji（实测截图里它就变成了一颗蓝紫色方块），令牌色板当场被穿帮。
  Widget _navItem(BuildContext context, IconData glyph, String text, bool on,
      VoidCallback tap) {
    final s = skinOf(context);
    return InkWell(
      onTap: tap,
      borderRadius: BorderRadius.circular(Metrics.radiusSm),
      child: Container(
        constraints: const BoxConstraints(minHeight: Metrics.navH),
        decoration: BoxDecoration(
          color: on ? s.accSoft : Colors.transparent,
          borderRadius: BorderRadius.circular(Metrics.radiusSm),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(glyph, size: 15, color: on ? s.acc : s.t3),
            const SizedBox(height: 2),
            Text(text,
                style: TextStyle(
                  fontSize: Type.metaWide,
                  fontWeight: FontWeight.w700,
                  color: on ? s.acc : s.t3,
                )),
          ],
        ),
      ),
    );
  }
}

/// 撤销条。它**必须让到输入条上面**（对比稿实测：默认位置会盖住输入条）。
class UndoBar extends StatelessWidget {
  final String title;
  final VoidCallback onUndo;
  const UndoBar({required this.title, required this.onUndo, super.key});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    return Container(
      margin: const EdgeInsets.only(left: 12, right: 12, bottom: 8),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(
        color: s.sel,
        borderRadius: BorderRadius.circular(Metrics.radiusSm),
        boxShadow: [
          BoxShadow(color: Colors.black.withValues(alpha: .35), blurRadius: 30, offset: const Offset(0, 10)),
        ],
      ),
      child: Row(
        children: [
          Expanded(
            child: Text('已完成「$title」',
                style: TextStyle(
                    fontSize: Type.undo, fontWeight: FontWeight.w700, color: s.selInk)),
          ),
          InkWell(
            onTap: onUndo,
            child: Container(
              constraints: const BoxConstraints(minWidth: Metrics.touch, minHeight: Metrics.touch),
              alignment: Alignment.center,
              child: Text('撤销',
                  style: TextStyle(
                      fontSize: Metrics.body, fontWeight: FontWeight.w800, color: s.accOnSel)),
            ),
          ),
        ],
      ),
    );
  }
}

/// 48dp 的图标按钮（齿轮、左右切天都用它）。视觉 15–17px，点击区整块 48×48。
/// 48dp 一格的图标按钮。`onTap` 传 null 就是「这一头已经到头」：按不动，
/// 边框和字形都要淡下去 —— 摆一个看着能按、按了什么都不发生的东西，比不摆更糟。
class IconButton48 extends StatelessWidget {
  final String glyph;
  final VoidCallback? onTap;
  final bool outlined;
  const IconButton48(this.glyph, {this.onTap, this.outlined = false, super.key});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(Metrics.radiusSm),
      child: Container(
        width: Metrics.touch,
        height: Metrics.touch,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          border: outlined ? Border.all(color: onTap == null ? Colors.transparent : s.line) : null,
          borderRadius: BorderRadius.circular(Metrics.radiusSm),
        ),
        child: Text(glyph,
            style: TextStyle(
                fontSize: 17, color: onTap == null ? s.t3.withValues(alpha: .45) : s.t2)),
      ),
    );
  }
}
