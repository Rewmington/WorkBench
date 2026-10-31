/// 课表屏 = 候选 B（单日竖列 + 7 日条）。
///
/// 选 B 而不选竞品那种压缩周网格，是为了**课名全文可读**；代价（「这周哪几天空」要滑 7 次
/// 才拼得出来）在 PLAN-MOBILE §5.4 里已经写明并接受。这一屏上有三条不许破的规矩：
/// - 空档显式写「空闲」，不许留白（这张表最有用的一件事就是找空档）；
/// - 「下一节」是描边 + 文字两个通道，且**不是「此刻」** —— 课间空档里没有正在上的课；
/// - 底部那行空档摘要与今天屏同值同函数。
library;

import 'package:flutter/material.dart';

import '../palette.dart';
import '../store.dart';
import '../timetable.dart';
import 'parts.dart';

class TimetableScreen extends StatefulWidget {
  final Store store;
  final VoidCallback onOpenSettings;
  final ValueChanged<bool> onTab;
  const TimetableScreen({
    required this.store,
    required this.onOpenSettings,
    required this.onTab,
    super.key,
  });

  @override
  State<TimetableScreen> createState() => _TimetableScreenState();
}

class _TimetableScreenState extends State<TimetableScreen> {
  /// 7 日条选中的那天（1=周一…7=周日）
  late int _dow = widget.store.clock.dow;

  void _go(int dow) => setState(() => _dow = dow.clamp(1, 7));

  /// 除 7 日条以外，每一行都回到页面左右 14dp 的基线（§5.2 的 --p-padx）。
  Widget _pad(Widget w) =>
      Padding(padding: const EdgeInsets.symmetric(horizontal: Metrics.padX), child: w);

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final store = widget.store;

    return Scaffold(
      backgroundColor: s.bg,
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            Expanded(
              // 左右滑切天用整屏的横向手势，而不是把 PageView 塞进这一列：
              // 竖列自己要在纵向 ListView 里长高，嵌套滚动要么炸要么变成两处滚动互相抢。
              child: GestureDetector(
                onHorizontalDragEnd: (d) {
                  final v = d.primaryVelocity ?? 0;
                  if (v < -30) _go(_dow + 1);
                  if (v > 30) _go(_dow - 1);
                },
                child: ListView(
                  // 横向内边距不交给 ListView：7 日条要占满 360 宽才够 48dp 一格，
                  // 而负 padding 会被 Flutter 断言拦下。所以逐个子项自己决定左右留白。
                  padding: const EdgeInsets.only(top: 2, bottom: 14),
                  children: [
                    _pad(
                      Stack(
                        children: [
                          Padding(
                            padding: const EdgeInsets.only(top: 6, bottom: 10),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(headDateLine(store.clock, store.currentWeek),
                                    style: TextStyle(fontSize: Type.metaWide, color: s.t3)),
                                const SizedBox(height: 2),
                                Text('课表',
                                    style: TextStyle(
                                      fontSize: Metrics.h2,
                                      fontWeight: FontWeight.w800,
                                      letterSpacing: -.02 * Metrics.h2,
                                      color: s.t1,
                                    )),
                              ],
                            ),
                          ),
                          Positioned(
                              right: 0,
                              top: 14,
                              child: IconButton48('⚙', onTap: widget.onOpenSettings)),
                        ],
                      ),
                    ),
                    _pad(SyncLine(store)),
                    if (store.currentWeek == null)
                      _pad(_noWeek(context, store))
                    else ...[
                      _pad(_weekBar(store)),
                      _dayStrip(store),
                      _pad(_dayColumn(context, store, _dow)),
                    ],
                    _pad(GapLine(store)),
                  ],
                ),
              ),
            ),
            Container(
              decoration: BoxDecoration(
                color: s.bg,
                border: Border(top: BorderSide(color: s.line)),
              ),
              padding: const EdgeInsets.fromLTRB(Metrics.padX, 8, Metrics.padX, 10),
              child: BottomNav(onTimetable: true, onTap: widget.onTab),
            ),
          ],
        ),
      ),
    );
  }

  /// 没设开学日 / 还没同步到课表：这一屏说不出「今天第几周」，就明说，不硬排一版。
  Widget _noWeek(BuildContext context, Store store) {
    final s = skinOf(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 26, horizontal: 2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(store.classes.isEmpty ? '这台手机还没同步到课表' : '电脑上的开学日还没设',
              style: TextStyle(fontSize: Metrics.body, color: s.t1)),
          const SizedBox(height: 6),
          Text('课表要在电脑上导入并同步一次，才会出现在这里',
              style: TextStyle(fontSize: Type.meta, color: s.t3)),
        ],
      ),
    );
  }

  Widget _weekBar(Store store) {
    final s = skinOf(context);
    return Padding(
      padding: const EdgeInsets.only(top: 2, bottom: 9),
      child: Row(
        children: [
          Text('第 ${store.currentWeek} 周',
              style: TextStyle(
                  fontSize: 13, fontWeight: FontWeight.w800, color: s.t1)),
          const SizedBox(width: 6),
          Text('左右滑切天', style: TextStyle(fontSize: Type.metaWide, color: s.t3)),
          const Spacer(),
          IconButton48('‹', outlined: true, onTap: () => _go(_dow - 1)),
          const SizedBox(width: 4),
          IconButton48('›', outlined: true, onTap: () => _go(_dow + 1)),
        ],
      ),
    );
  }

  /// 7 日条。360dp 宽下如果也跟着页边距缩进去，每格就只有 45dp，达不到 Android 最小触控
  /// —— 所以这一条自己只留 2dp 边，实测每格 49.1dp（§5.4 的来由）。
  Widget _dayStrip(Store store) {
    final monday = store.clock.date.subtract(Duration(days: store.clock.dow - 1));
    const names = ['一', '二', '三', '四', '五', '六', '日'];
    return Padding(
      padding: const EdgeInsets.only(top: 2, bottom: 12),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 2),
        child: Row(
          children: [
            for (var i = 0; i < 7; i++)
              Expanded(
                child: Padding(
                  padding: EdgeInsets.only(left: i == 0 ? 0 : 2),
                  child: _stripCell(
                      dow: i + 1,
                      name: names[i],
                      date: monday.add(Duration(days: i)),
                      store: store),
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _stripCell({required int dow, required String name, required DateTime date, required Store store}) {
    final s = skinOf(context);
    final hasClass = store.classesOnDay(dow).isNotEmpty;
    final on = dow == _dow;
    final isToday = dow == store.clock.dow;
    return InkWell(
      onTap: () => _go(dow),
      borderRadius: BorderRadius.circular(Metrics.radiusSm),
      child: Container(
        constraints: const BoxConstraints(minHeight: Metrics.touch),
        decoration: BoxDecoration(
          color: on ? s.accSoft : Colors.transparent,
          border: Border.all(color: on ? Colors.transparent : s.line),
          borderRadius: BorderRadius.circular(Metrics.radiusSm),
        ),
        child: Stack(
          alignment: Alignment.center,
          children: [
            Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(name,
                    style: TextStyle(
                        fontSize: Type.kicker,
                        fontWeight: FontWeight.w700,
                        color: on ? s.acc : s.t3)),
                const SizedBox(height: 1),
                Text('${date.day}',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: on ? FontWeight.w800 : FontWeight.w400,
                      color: on ? s.acc : isToday ? s.t1 : s.t2,
                      fontFeatures: const [FontFeature.tabularFigures()],
                    )),
              ],
            ),
            if (hasClass)
              Positioned(
                bottom: 6,
                child: Container(
                  width: 4,
                  height: 4,
                  decoration: BoxDecoration(color: s.blue, shape: BoxShape.circle),
                ),
              ),
            // 跨月的那个 1 号标出来，不然「1」会被读成一号的课而不是下个月
            if (date.day == 1 && !on)
              Positioned(
                right: 3,
                top: 4,
                child: Text('月',
                    style: TextStyle(fontSize: 8, color: s.t3.withValues(alpha: .7))),
              ),
          ],
        ),
      ),
    );
  }

  Widget _dayColumn(BuildContext context, Store store, int dow) {
    final rows = dow == store.clock.dow
        ? store.classesToday()
        : store.classesOnDay(dow);
    final skeleton = store.periodSkeleton;
    final nowMin = minutesOfDay(store.clock.date);
    final isToday = dow == store.clock.dow;

    // 「下一节」= 今天里最早的那节还没开始的课。正在上课的时候标的也是它，
    // 而不是把正在上的那节说成「此刻」—— 课间空档里没有正在上的课（§5.4）
    final next = isToday
        ? rows.where((c) => c.startMin > nowMin).fold<ClassRow?>(
            null, (a, b) => a == null || b.startMin < a.startMin ? b : a)
        : null;

    if (skeleton.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Text('这天没有课', style: TextStyle(fontSize: Metrics.body, color: skinOf(context).t3)),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final slot in skeleton)
          _periodRow(store, slot, rows, next?.id, nowMin),
      ],
    );
  }

  Widget _periodRow(
      Store store, ({String label, int start, int end}) slot, List<ClassRow> rows, String? nextId, int nowMin) {
    final hitting = rows.where((c) => c.startMin < slot.end && slot.start < c.endMin).toList()
      ..sort((a, b) => a.startMin.compareTo(b.startMin));
    if (hitting.isEmpty) {
      // 空档显式写「空闲」：留一格白的，用户得自己算这是有空还是没导入。
      // 对比稿这里原本是 CSS 的 dashed 边框；Flutter 没有原生虚线边框，所以空档那一格
      // 换成「无底色 + 同一条 1px 边 + 文字自己说空闲」，不为了像稿子去手画一个 CustomPainter。
      return _PeriodShell(
        free: true,
        start: slot.start,
        end: slot.end,
        label: '空闲',
        sub: slot.label,
      );
    }
    return Column(
      children: [
        for (final c in hitting)
          _PeriodShell(
            start: c.startMin,
            end: c.endMin,
            label: c.name,
            sub: '${c.room} · ${c.teacher} · ${c.weeks} 周',
            next: c.id == nextId,
          ),
      ],
    );
  }
}

/// 一节（或一段空档）那一行。
class _PeriodShell extends StatelessWidget {
  final int start;
  final int end;
  final String label;
  final String sub;
  final bool free;
  final bool next;

  const _PeriodShell({
    required this.start,
    required this.end,
    required this.label,
    required this.sub,
    this.free = false,
    this.next = false,
  });

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    return Container(
      margin: const EdgeInsets.only(bottom: Metrics.rowGap),
      constraints: const BoxConstraints(minHeight: Metrics.rowH),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 11),
      decoration: BoxDecoration(
        color: free ? Colors.transparent : s.surf,
        border: Border.all(
          color: next ? s.acc : s.line,
          width: next ? 1.5 : 1,
          strokeAlign: BorderSide.strokeAlignInside,
        ),
        borderRadius: BorderRadius.circular(Metrics.radiusSm),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 44,
            child: Text('${hhmm(start)}\n${hhmm(end)}',
                style: TextStyle(
                  fontSize: Type.meta,
                  height: 1.5,
                  color: s.t3,
                  fontFeatures: const [FontFeature.tabularFigures()],
                )),
          ),
          const SizedBox(width: 11),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(label,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: Metrics.body,
                            // 课名全文，不简称（§5.4）；空档那格压到常规字重，别抢课的视觉重量
                            fontWeight: free ? FontWeight.w400 : FontWeight.w600,
                            color: free ? s.t3 : s.t1,
                          )),
                    ),
                    if (next) ...[
                      const SizedBox(width: 6),
                      Container(
                        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
                        decoration: BoxDecoration(
                            color: s.accSoft, borderRadius: BorderRadius.circular(4)),
                        child: Text('下一节',
                            style: TextStyle(
                                fontSize: Type.chip,
                                fontWeight: FontWeight.w800,
                                color: s.acc)),
                      ),
                    ],
                  ],
                ),
                const SizedBox(height: 2),
                Text(sub, style: TextStyle(fontSize: Type.meta, color: s.t3)),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
