/// 今天屏的用药卡：药箱的**只读镜像**。
///
/// 这里不提供打卡 —— 打卡会写药箱的库，而 Anvil 对药箱只有 GET/PROPFIND。
/// 卡片说的是可验证的事：三环（今日服药 / 本周依从 / 连续打卡）、下一次是什么、
/// 有没有药快没了。三个数的口径全部来自药箱自己那段算法（见 `medicine.dart`），
/// 与桌面端「药箱联动」页算出来的必须是同一个数。
/// 时间一律写「药箱发布于 12:03」，不假装是此刻（§5.5 同一条纪律）。
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../medicine.dart';
import '../palette.dart';
import '../reltime.dart';
import '../store.dart';

class MedicineCard extends StatelessWidget {
  final Store store;
  const MedicineCard(this.store, {super.key});

  @override
  Widget build(BuildContext context) {
    if (!store.medicineEnabled) return const SizedBox.shrink();
    final view = store.medicine;
    final plan = store.todayMedicine;
    if (view == null || view.isEmpty || plan == null) {
      // 读挂了以前和「没药 / 还没缓存过」长得一模一样：整卡不画，于是"用药"这件事在主屏
      // 完全沉默，而那句错误只在设置页的一小格里。补一行矮卡，主屏就能看见它坏了。
      final err = store.medicineError;
      if (err == null) return const SizedBox.shrink();
      final skin = skinOf(context);
      return Padding(
        padding: const EdgeInsets.only(bottom: 9),
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.fromLTRB(11, 9, 11, 9),
          decoration: BoxDecoration(
            color: skin.surf,
            border: Border.all(color: skin.line),
            borderRadius: BorderRadius.circular(Metrics.radiusSm),
          ),
          child: Text('药箱这次没读到：$err',
              style: TextStyle(fontSize: Type.metaWide, color: skin.red)),
        ),
      );
    }

    final s = skinOf(context);
    final now = store.clock.date;
    final status = _status(plan, s);
    final stock = _stockRow(view, s);
    final stats = view.weekStats(now);
    final streak = view.streakDays(now);
    return Padding(
      padding: const EdgeInsets.only(bottom: 9),
      child: InkWell(
        onTap: () => showMedicineSheet(context, store),
        borderRadius: BorderRadius.circular(Metrics.radiusSm),
        child: Container(
          padding: const EdgeInsets.fromLTRB(11, 9, 11, 9),
          decoration: BoxDecoration(
            color: s.surf,
            border: Border.all(color: s.line),
            borderRadius: BorderRadius.circular(Metrics.radiusSm),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Text('用药',
                      style: TextStyle(
                          fontSize: Type.kicker,
                          letterSpacing: .12,
                          fontWeight: FontWeight.w700,
                          color: s.t3)),
                  const Spacer(),
                  Text(
                    view.publishedAt == null
                        ? '药箱发布时间未知'
                        : '药箱 ${agoLabel(store.clock.now, view.publishedAt!)}',
                    style: TextStyle(fontSize: Type.meta, color: s.t3),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  _ActivityRings(
                    today: plan.planned > 0 ? plan.taken / plan.planned : 0,
                    week: stats.hasPlan ? stats.rate : 0,
                    streak: streak / PeriodStats.weekDays,
                    skin: s,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      children: [
                        _RingRow(
                            color: s.acc,
                            label: '今日服药',
                            value: plan.planned > 0
                                ? '${plan.taken} / ${plan.planned}'
                                : '没有安排'),
                        _RingRow(
                            color: s.blue,
                            label: '本周依从',
                            value: stats.hasPlan
                                ? '${(stats.rate * 100).round()}%'
                                : '没有安排'),
                        _RingRow(
                            color: s.amber,
                            label: '连续打卡',
                            value: '$streak 天'),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 5),
              Text(status.text,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: Type.metaWide, color: status.color)),
              if (stock != null) ...[
                const SizedBox(height: 3),
                stock,
              ],
            ],
          ),
        ),
      ),
    );
  }

  ({String text, Color color}) _status(TodayPlan plan, Skin s) {
    if (plan.slots.isEmpty) return (text: '今天没有用药安排', color: s.t3);
    if (plan.planned == 0) return (text: '今天都标记了没吃', color: s.t3);
    final due = plan.due;
    if (due != null) {
      return (text: '待记 ${formatMinutes(due.minute)} · ${due.medicine.name}', color: s.amber);
    }
    final next = plan.next;
    if (next != null) {
      return (
        text: '下一次 ${formatMinutes(next.minute)} · ${next.medicine.name}',
        color: s.t2,
      );
    }
    return (text: '今天都记了', color: s.acc);
  }

  /// 已用完优先说，其次才是偏低；两种都有就都写，中间用「·」隔开。
  /// 两段分开着色：「已用完」是红，「剩 N 天」是琥珀 —— 一句话全红会夸大次要那条。
  /// 名单也按还剩几天排：排最前的那个名字就是最该去补的。
  Widget? _stockRow(MedicineView view, Skin s) {
    final stock = view.byStock;
    final out = [for (final m in stock) if (m.isOut) m.name];
    final low = [
      for (final m in stock)
        if (!m.isOut && m.isLow) '${m.name} 剩 ${m.daysLeft!.ceil()} 天',
    ];
    if (out.isEmpty && low.isEmpty) return null;
    return Wrap(
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        if (out.isNotEmpty)
          Text('已用完：${out.join('、')}',
              style: TextStyle(fontSize: Type.meta, color: s.red)),
        if (out.isNotEmpty && low.isNotEmpty)
          Text(' · ', style: TextStyle(fontSize: Type.meta, color: s.t3)),
        if (low.isNotEmpty)
          Text(low.join('、'), style: TextStyle(fontSize: Type.meta, color: s.amber)),
      ],
    );
  }
}

/// 三色活动环：今日服药 / 本周依从 / 连续打卡。
///
/// 几何照药箱今天屏那张（它 `theme.dart` 的 `ActivityRings`），颜色只从 [Skin] 里取，
/// 分工与桌面端 `AdherenceRings.vue` 一致：绿=今天、蓝=本周、金=连续。
class _ActivityRings extends StatelessWidget {
  final double today;
  final double week;
  final double streak;
  final Skin skin;

  const _ActivityRings({
    required this.today,
    required this.week,
    required this.streak,
    required this.skin,
  });

  @override
  Widget build(BuildContext context) => CustomPaint(
        size: const Size(72, 72),
        painter: _RingsPainter(
          values: [
            today.clamp(0.0, 1.0),
            week.clamp(0.0, 1.0),
            streak.clamp(0.0, 1.0),
          ],
          track: skin.line2,
          colors: [skin.acc, skin.blue, skin.amber],
        ),
      );
}

class _RingsPainter extends CustomPainter {
  final List<double> values;
  final Color track;
  final List<Color> colors;

  const _RingsPainter({
    required this.values,
    required this.track,
    required this.colors,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final stroke = size.width * 0.086;
    for (var i = 0; i < values.length; i++) {
      final radius = size.width / 2 - stroke / 2 - i * stroke * 1.36;
      canvas.drawCircle(
        c,
        radius,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = stroke
          ..color = track,
      );
      final v = values[i];
      if (v <= 0) continue; // 没有安排就留空环，不画成「一圈满」
      canvas.drawArc(
        Rect.fromCircle(center: c, radius: radius),
        -math.pi / 2,
        2 * math.pi * v,
        false,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = stroke
          ..strokeCap = StrokeCap.round
          ..color = colors[i],
      );
    }
  }

  @override
  bool shouldRepaint(_RingsPainter old) {
    for (var i = 0; i < values.length; i++) {
      if (old.values[i] != values[i]) return true;
    }
    return old.track != track || old.colors != colors;
  }
}

class _RingRow extends StatelessWidget {
  final Color color;
  final String label;
  final String value;

  const _RingRow({required this.color, required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        children: [
          Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(color: color, shape: BoxShape.circle),
          ),
          const SizedBox(width: 7),
          Expanded(
            child: Text(label,
                style: TextStyle(fontSize: Type.metaWide, color: s.t2)),
          ),
          Text(value,
              style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w700,
                  color: s.t1,
                  fontFeatures: const [FontFeature.tabularFigures()])),
        ],
      ),
    );
  }
}

Future<void> showMedicineSheet(BuildContext context, Store store) {
  final s = skinOf(context);
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    backgroundColor: s.surf,
    shape: const RoundedRectangleBorder(
      borderRadius: BorderRadius.vertical(top: Radius.circular(Metrics.radius)),
    ),
    builder: (_) => MedicineSheet(store: store),
  );
}

class MedicineSheet extends StatelessWidget {
  final Store store;
  const MedicineSheet({required this.store, super.key});

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final view = store.medicine;
    final plan = view?.todayPlan(store.clock.date);
    return SafeArea(
      top: false,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * .82),
        child: ListView(
          shrinkWrap: true,
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 18),
          children: [
            Row(
              children: [
                Text('用药',
                    style: TextStyle(
                        fontSize: 17, fontWeight: FontWeight.w700, color: s.t1)),
                const SizedBox(width: 8),
                Text('只读', style: TextStyle(fontSize: Type.meta, color: s.t3)),
                const Spacer(),
                IconButton(
                  onPressed: () => Navigator.pop(context),
                  constraints:
                      const BoxConstraints(minWidth: Metrics.touch, minHeight: Metrics.touch),
                  icon: Icon(Icons.close, size: 20, color: s.t2),
                ),
              ],
            ),
            _source(context, view),
            const SizedBox(height: 12),
            if (view == null || view.isEmpty)
              Text('药箱里还没有药', style: TextStyle(fontSize: Type.metaWide, color: s.t3))
            else
              for (final m in view.byStock) _medicine(context, m, plan),
            const SizedBox(height: 6),
            Text('这里只读；回药箱打卡并同步后才会更新',
                style: TextStyle(fontSize: Type.meta, color: s.t3)),
          ],
        ),
      ),
    );
  }

  Widget _source(BuildContext context, MedicineView? view) {
    final s = skinOf(context);
    final err = store.medicineError;
    final published = view?.publishedAt;
    final fetched = view?.fetchedAt;
    // 这两处都用 `agoLabel` 而不是钟点：药箱那份可能是好几天前发布的，
    // 「药箱 09:12」会被读成"今天 09:12"，而用药这件事最需要的就是知道它旧不旧
    if (err != null) {
      return Text(
        published == null
            ? '这次没读到：$err'
            : '这次没读到：$err · 屏上那份是 ${agoLabel(store.clock.now, published)}的',
        style: TextStyle(fontSize: Type.meta, height: 1.5, color: s.amber),
      );
    }
    if (published != null) {
      return Text(
        '药箱发布于 ${agoLabel(store.clock.now, published)}'
        '${fetched == null ? '' : ' · 读取于 ${agoLabel(store.clock.now, fetched)}'}',
        style: TextStyle(fontSize: Type.meta, color: s.t3),
      );
    }
    if (fetched != null) {
      return Text('药箱还没同步过', style: TextStyle(fontSize: Type.meta, color: s.t3));
    }
    return Text('还没读过药箱', style: TextStyle(fontSize: Type.meta, color: s.t3));
  }

  Widget _medicine(BuildContext context, Medicine m, TodayPlan? plan) {
    final s = skinOf(context);
    final slots = plan?.slots.where((x) => x.medicine.id == m.id).toList() ?? const [];
    final days = m.daysLeft;
    final badge = m.isOut
        ? (text: '已用完', color: s.red)
        : m.isLow
            ? (text: '剩 ${days!.ceil()} 天', color: s.amber)
            : (text: days == null ? '未设用量' : '剩 ${days.ceil()} 天', color: s.t3);
    return Padding(
      padding: const EdgeInsets.only(bottom: 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(m.name,
                    style: TextStyle(
                        fontSize: Metrics.body,
                        fontWeight: FontWeight.w600,
                        color: s.t1)),
              ),
              Text(badge.text, style: TextStyle(fontSize: Type.metaWide, color: badge.color)),
            ],
          ),
          const SizedBox(height: 3),
          if (slots.isEmpty)
            Text('今天没有安排', style: TextStyle(fontSize: Type.meta, color: s.t3))
          else
            Wrap(
              spacing: 10,
              runSpacing: 2,
              children: [
                for (final slot in slots)
                  Text(
                    _slotText(slot, plan!),
                    style: TextStyle(
                      fontSize: Type.metaWide,
                      color: switch (slot) {
                        DoseSlot(taken: true) => s.t2,
                        DoseSlot(skipped: true) => s.t3,
                        _ => slot.minute + 30 < plan.nowMinute ? s.amber : s.t3,
                      },
                    ),
                  ),
              ],
            ),
          const SizedBox(height: 3),
          Text(
            '${m.doseLabel} · 余 ${_trim(m.remaining)} ${m.unit}',
            style: TextStyle(fontSize: Type.meta, color: s.t3),
          ),
        ],
      ),
    );
  }

  String _slotText(DoseSlot slot, TodayPlan plan) {
    final t = formatMinutes(slot.minute);
    if (slot.taken) return '$t 已服';
    if (slot.skipped) return '$t 没吃';
    if (slot.minute + 30 < plan.nowMinute) return '$t 待记';
    return '$t 未到';
  }

  static String _trim(double v) =>
      v == v.roundToDouble() ? v.toInt().toString() : v.toStringAsFixed(1);
}
