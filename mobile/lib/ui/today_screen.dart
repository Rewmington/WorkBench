/// 今天屏 = 候选 A（清单 + 常驻输入条），版式逐条对着对比稿搬。
///
/// 三件事在这屏上是有主见的，不是随手定的：
/// - 分区顺序 **逾期 → 今天 → 随时 → 已完成**，一条一行（行高 54）；
/// - 勾选 = **延迟消失 + 撤销条**：窄屏上划掉的东西常驻着最容易连点误触；
/// - 课**不进这个列表**，只以那行空档摘要出现，且与课表屏同一个函数算。
library;

import 'dart:async';

import 'package:flutter/material.dart';

import '../palette.dart';
import '../reltime.dart';
import '../store.dart';
import '../todo.dart';
import 'medicine_card.dart';
import 'parts.dart';

class TodayScreen extends StatefulWidget {
  final Store store;
  final VoidCallback onOpenSettings;
  final ValueChanged<bool> onTab;

  /// 演示用：截图那一档要让撤销条先亮着（对比稿画的就是那一瞬间）
  final String? initialUndoTitle;
  const TodayScreen({
    required this.store,
    required this.onOpenSettings,
    required this.onTab,
    this.initialUndoTitle,
    super.key,
  });

  @override
  State<TodayScreen> createState() => _TodayScreenState();
}

class _TodayScreenState extends State<TodayScreen> {
  final _input = TextEditingController();
  Timer? _undoTimer;
  Timer? _exitTimer;

  /// 刚勾掉、正在滑走的那几条：记着它们**原来在哪个区**，滑走期间仍显示在原处（划掉的样子）。
  /// 直接跳去「已完成」也行，但那意味着点一下就消失 —— 窄屏上连点误触没处后悔。
  final Map<String, Section> _exiting = {};

  /// 滑走期间把每一区「都有谁、按什么顺序」冻在切换前那一版。
  /// 不冻的话它会掉到区尾 —— 而 ListView 是懒建的，区尾那格常常已经在屏幕外，
  /// 于是"留在原位划掉"会变成"点一下它凭空消失"（这条是被测试逼出来的，不是想当然）。
  Map<Section, List<String>>? _frozen;
  String? _undoTitle;

  @override
  void initState() {
    super.initState();
    if (widget.initialUndoTitle != null) _armUndo(widget.initialUndoTitle!);
  }

  @override
  void dispose() {
    _undoTimer?.cancel();
    _exitTimer?.cancel();
    _input.dispose();
    super.dispose();
  }

  /// 撤销条停 4 秒：够伸手点回去，又不会一直挡着输入条
  void _armUndo(String title) {
    _undoTimer?.cancel();
    setState(() => _undoTitle = title);
    _undoTimer = Timer(const Duration(seconds: 4), () {
      if (mounted) setState(() => _undoTitle = null);
    });
  }

  Map<Section, List<String>> _idsBySection(Store store) => {
        for (final section in Section.values)
          section: (store.sections[section] ?? const <Todo>[]).map((t) => t.id).toList(),
      };

  void _toggle(Todo t) {
    if (t.completed) {
      // 取消勾选（撤销条那条路也走这里）：立刻回到原来的位置
      unawaited(widget.store.setDone(t.id, false));
      setState(() {
        _exiting.remove(t.id);
        _frozen = null;
      });
      return;
    }
    final before = _idsBySection(widget.store);
    final from = t.sectionOf(widget.store.now);
    unawaited(widget.store.setDone(t.id, true));
    setState(() {
      _exiting[t.id] = from;
      _frozen = before;
    });
    _armUndo(t.title);
    _exitTimer?.cancel();
    _exitTimer = Timer(const Duration(milliseconds: 520), () {
      if (mounted) {
        setState(() {
          _exiting.remove(t.id);
          _frozen = null;
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final store = widget.store;
    final zones = _zones(store);
    final live = zones.values.fold<int>(0, (a, b) => a + b.length);
    final undoTarget = _undoTitle == null
        ? null
        : store.todos
            .where((t) => t.title == _undoTitle && t.completed)
            .fold<Todo?>(null, (a, b) => a ?? b);

    return Scaffold(
      backgroundColor: s.bg,
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(Metrics.padX, 2, Metrics.padX, 14),
                children: [
                  _head(store, zones),
                  SyncLine(store),
                  GapLine(store),
                  MedicineCard(store),
                  for (final section in Section.values)
                    if (zones[section]!.isNotEmpty) ...[
                      SectionHead(section, zones[section]!.length),
                      for (final t in zones[section]!)
                        TodoRow(
                          key: ValueKey(t.id),
                          todo: t,
                          now: store.now,
                          exiting: _exiting.containsKey(t.id),
                          onToggle: () => _toggle(t),
                        ),
                    ],
                  if (live == 0) _empty(store),
                ],
              ),
            ),
            // 撤销条让到输入条上面（对比稿实测：默认位置会盖住输入条，看不见自己刚写的那句）
            if (_undoTitle != null && undoTarget != null)
              UndoBar(
                title: _undoTitle!,
                onUndo: () => _toggle(undoTarget),
              ),
            _foot(context, store),
          ],
        ),
      ),
    );
  }

  /// 分区结果。滑走期间用切换前那份**顺序**，但行内容取当前值（所以它是划掉的样子的同时
  /// 又留在原地），滑走结束就回到实时分区。
  Map<Section, List<Todo>> _zones(Store store) {
    final byId = {for (final t in store.todos) t.id: t};
    final source = _frozen ?? _idsBySection(store);
    return {
      for (final section in Section.values)
        section: [
          for (final id in source[section] ?? const <String>[])
            if (byId[id] != null && !byId[id]!.isTombstone) byId[id]!,
        ],
    };
  }

  Widget _head(Store store, Map<Section, List<Todo>> sections) {
    final s = skinOf(context);
    return Stack(
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 6, bottom: 10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(headDateLine(store.clock, store.currentWeek),
                  style: TextStyle(fontSize: Type.metaWide, color: s.t3)),
              const SizedBox(height: 2),
              Text('今天',
                  style: TextStyle(
                    fontSize: Metrics.h2,
                    fontWeight: FontWeight.w800,
                    letterSpacing: -.02 * Metrics.h2,
                    color: s.t1,
                  )),
              const SizedBox(height: 11),
              Row(
                children: [
                  _kpi(s, '${sections[Section.late]!.length}', '逾期', red: true),
                  const SizedBox(width: 14),
                  _kpi(s, '${sections[Section.today]!.length}', '今天'),
                  const SizedBox(width: 14),
                  _kpi(s, '${sections[Section.done]!.length}', '已完成', acc: true),
                ],
              ),
            ],
          ),
        ),
        Positioned(
          right: 0,
          top: 14,
          child: IconButton48('⚙', onTap: widget.onOpenSettings),
        ),
      ],
    );
  }

  Widget _kpi(Skin s, String n, String label, {bool red = false, bool acc = false}) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(n,
              style: TextStyle(
                fontSize: Type.kpi,
                fontWeight: FontWeight.w800,
                letterSpacing: -.03 * Type.kpi,
                color: red ? s.red : acc ? s.acc : s.t1,
                fontFeatures: const [FontFeature.tabularFigures()],
              )),
          const SizedBox(height: 1),
          Text(label,
              style: TextStyle(
                fontSize: Type.kicker,
                letterSpacing: .06,
                color: s.t3,
              )),
        ],
      );

  /// 空态那两句话分得清：「今天确实没有」与「还没同步过、可能不是真的没有」（§5.5 末两行）
  Widget _empty(Store store) {
    final s = skinOf(context);
    final text = switch (store.sync) {
      NeverSynced() => '还没同步过 · 这里可能是空的',
      OfflineAt(:final atMs) => '今天没有待办 —— 显示的是 ${clockTime(atMs)} 那份',
      SyncedAt(:final atMs) => '今天没有待办 —— 上次同步于 ${clockTime(atMs)}',
    };
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 22),
      child: Text(text,
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: Type.metaWide, color: s.t3)),
    );
  }

  Widget _foot(BuildContext context, Store store) {
    final s = skinOf(context);
    return Container(
      decoration: BoxDecoration(
        color: s.bg,
        border: Border(top: BorderSide(color: s.line)),
      ),
      padding: const EdgeInsets.fromLTRB(Metrics.padX, 8, Metrics.padX, 10),
      child: Column(
        children: [
          Row(
            children: [
              Expanded(
                child: Container(
                  constraints: const BoxConstraints(minHeight: Metrics.touch),
                  padding: const EdgeInsets.symmetric(horizontal: 15),
                  decoration: BoxDecoration(
                    color: s.surf,
                    border: Border.all(color: s.line2),
                    borderRadius: BorderRadius.circular(999),
                  ),
                  alignment: Alignment.center,
                  child: TextField(
                    controller: _input,
                    // 正文 -1：输入条那一行比列表小半号，不然整屏都是 15 会糊
                    style: TextStyle(fontSize: Type.input, color: s.t1),
                    cursorColor: s.acc,
                    textInputAction: TextInputAction.done,
                    decoration: InputDecoration(
                      isCollapsed: true,
                      hintText: '记一条…',
                      hintStyle: TextStyle(fontSize: Type.input, color: s.t3),
                      border: InputBorder.none,
                    ),
                    onSubmitted: (_) => _submit(),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              // 发送按钮 48×48 实心圆：这是这一屏唯一的强调色填充（§8.8 同一条克制）
              InkWell(
                onTap: _submit,
                customBorder: const CircleBorder(),
                child: Container(
                  width: Metrics.touch,
                  height: Metrics.touch,
                  decoration: BoxDecoration(color: s.acc, shape: BoxShape.circle),
                  alignment: Alignment.center,
                  child: Icon(Icons.arrow_upward, size: 18, color: s.accInk),
                ),
              ),
            ],
          ),
          const SizedBox(height: 7),
          BottomNav(onTimetable: false, onTap: widget.onTab),
        ],
      ),
    );
  }

  void _submit() {
    final title = _input.text.trim();
    if (title.isEmpty) return;
    // 新建不该被滑走动画挡住：冻着的那一版里没有它
    setState(() => _frozen = null);
    unawaited(widget.store.create(title));
    _input.clear();
    // 新建的那条落「随时」——§5.3 的第三条规则：没有截止时间的不混进今天
  }
}
