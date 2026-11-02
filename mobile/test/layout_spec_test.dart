/// M11c 验收的**数字**部分（PLAN-MOBILE §6）：360×800dp、正文 15px、触控 ≥48dp、
/// 导航两项 + 齿轮、撤销条不盖输入条、空档摘要两屏同值、空态两句话分得清。
///
/// 为什么不靠截图判：截图只能证明「看起来对」，而这些条全是可以量的。
/// 对比稿底部那一行实测（一屏可见几条 / 溢出多少像素 / 最小触控多少）在这里
/// 变成断言 —— 改坏了会在测试里红，而不是等肉眼看图才发现。
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/demo.dart';
import 'package:anvil_mobile/palette.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/todo.dart';
import 'package:anvil_mobile/ui/parts.dart';

const Size frame = Size(360, 800);

Future<Store> pump(WidgetTester tester, DemoCase c, {Size size = frame}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(AnvilApp(
      store: demoStore(c), demo: c, forcedBrightness: Brightness.dark));
  await tester.pumpAndSettle();
  return tester.widget<AnvilApp>(find.byType(AnvilApp)).store;
}

List<TodoRow> rows(WidgetTester tester) =>
    tester.widgetList<TodoRow>(find.byType(TodoRow)).toList();

/// 视口内完整可见的待办标题数（对比稿那行「首屏只露 N 条」的同一条量法）
int visibleRows(WidgetTester tester) {
  final viewport = tester.getRect(find.byType(Scrollable).first);
  var n = 0;
  for (final row in rows(tester)) {
    final r = tester.getRect(find.byWidget(row));
    if (r.top >= viewport.top - 0.5 && r.bottom <= viewport.bottom + 0.5) n++;
  }
  return n;
}

double scrollOverflow(WidgetTester tester) =>
    tester.state<ScrollableState>(find.byType(Scrollable).first).position.maxScrollExtent;

void main() {
  testWidgets('今天屏：满数据那一屏的形状与数字', (tester) async {
    final store = await pump(tester, DemoCase.full);

    // 正文 15px：桌面 13px 在手机上不适用，但也不许随手放大
    final firstRowWidget = find.byType(TodoRow).first;
    final title = tester
        .widgetList<Text>(find.descendant(of: firstRowWidget, matching: find.byType(Text)))
        .first;
    expect(title.style?.fontSize, Metrics.body, reason: '正文必须是 15px 这条基线');

    // 行高 54：一屏能露几条就是被它和 7px 缝隙决定的
    for (final row in rows(tester)) {
      final h = tester.getSize(find.byWidget(row)).height;
      expect(h, greaterThanOrEqualTo(Metrics.rowH), reason: '一条一行的行高下限是 54');
    }

    // 同步那行：两个数都可验证 —— 时间来自最后一次成功同步，条数来自 dirty
    expect(find.text('上次同步于 31 分钟前 · 2 条待上传'), findsOneWidget);
    expect(store.pending, 2, reason: '那句「2 条」不是写死的，它和计数同源');

    // 空档摘要（课表屏那条测试再来对一次）
    expect(find.textContaining('今天 3 节课 · 最近的空档', findRichText: true), findsOneWidget,
        reason: 'Text.rich 里的文字只有 findRichText 才看得见');

    // 导航只有两项 + 齿轮，设置不占第三个 tab（§5.2）
    expect(find.byType(BottomNav), findsOneWidget);
    expect(find.text('课表'), findsOneWidget);
    expect(find.text('设置'), findsNothing);
    expect(find.widgetWithText(IconButton48, '⚙'), findsOneWidget);

    // 触控下限 48：勾一下就要命中的那块，视觉 21dp 但点击区 48×48
    final cb = find.byKey(const ValueKey('cb:demo-t3'));
    expect(cb, findsOneWidget);
    final size = tester.getSize(cb);
    expect(size.width, greaterThanOrEqualTo(Metrics.touch));
    expect(size.height, greaterThanOrEqualTo(Metrics.touch));

    // 对比稿实测：800dp 高下首屏露 6 条、内容溢出靠滚动，不缩字号
    // 对比稿实测「首屏只露 6 条」——这里量出来同为 6 条，两条路对上了。
    // 溢出靠滚动解决，不缩字号（§5.3 明写的代价）。
    expect(visibleRows(tester), 6, reason: '首屏可见条数由行高决定，改了行高要回看 §5.3');
    expect(scrollOverflow(tester), greaterThan(300), reason: '溢出用滚动解决');
  });

  testWidgets('分区顺序：逾期 → 今天 → 随时 → 已完成（§5.3）', (tester) async {
    // 360×800 下 ListView 是懒建的，屏幕外的区根本不在树里 ——
    // 顺序这件事要在"全都建出来"的高度上量，首屏露几条则由上一条测试量。
    await pump(tester, DemoCase.full, size: const Size(360, 2400));
    final heads = tester
        .widgetList<SectionHead>(find.byType(SectionHead))
        .map((w) => w.section)
        .toList();
    expect(heads, Section.values, reason: '分区顺序是定稿的一部分，不跟着数据条数走');
    // 逾期那两条不会同时又出现在「今天」里
    final ids = tester.widgetList<TodoRow>(find.byType(TodoRow)).map((r) => r.todo.id).toList();
    expect(ids.toSet().length, ids.length, reason: '一条待办只能在一个区');
  });

  testWidgets('撤销条让到输入条上面（对比稿实测它原本会盖住）', (tester) async {
    final store = await pump(tester, DemoCase.undo);
    final undo = find.byType(UndoBar);
    expect(undo, findsOneWidget);
    final undoRect = tester.getRect(undo);
    final inputRect = tester.getRect(find.byType(TextField));
    expect(undoRect.bottom, lessThanOrEqualTo(inputRect.top + 0.5),
        reason: '撤销条压住输入条 = 看不见自己刚敲的那句');

    await tester.tap(find.text('撤销'));
    await tester.pumpAndSettle();
    expect(find.byType(UndoBar), findsNothing);
    expect(store.todos.firstWhere((t) => t.id == undoDemoId).completed, false);
  });

  testWidgets('勾选 → 延迟消失 + 撤销条', (tester) async {
    final store = await pump(tester, DemoCase.full);
    // 挑一条**本来没带黄点**的：这样「手机写过 → 待上传 +1」这件事才看得见
    // （演示数据里 交实验报告 / 买猫粮 那两条本来就是 dirty=1）
    final target = store.todos.firstWhere((t) => t.title == '跑一次备份还原演练');
    expect(target.completed, false);
    expect(target.dirty, false);
    final pendingBefore = store.pending;

    await tester.tap(find.byKey(ValueKey('cb:${target.id}')));
    await tester.pump();
    // 还留在原区（划掉的样子），不是立刻跳去已完成
    expect(find.text('跑一次备份还原演练'), findsOneWidget);
    expect(find.byType(UndoBar), findsOneWidget);

    await tester.pump(const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    final after = store.todos.firstWhere((t) => t.id == target.id);
    expect(after.completed, true);
    expect(after.dirty, true, reason: '手机写的这一条还没传出去，行末该亮黄点');
    expect(store.pending, pendingBefore + 1);
  });

  testWidgets('空态那两句话分得清（§5.5 末两行）', (tester) async {
    await pump(tester, DemoCase.empty);
    expect(find.text('今天没有待办 —— 上次同步于 31 分钟前'), findsOneWidget,
        reason: '真空态必须带同步时间，否则和「没同步到」长得一样');

    await pump(tester, DemoCase.firstRun);
    expect(find.text('还没同步过 · 这里可能是空的'), findsOneWidget);
    expect(find.textContaining('今天没有待办'), findsNothing,
        reason: '一次都没同步过时，不许说「今天没有」');
  });

  testWidgets('离线态不伪装实时、不转圈', (tester) async {
    await pump(tester, DemoCase.offline);
    expect(find.text('没连上坚果云 · 那份数据是 31 分钟前的'), findsOneWidget);
    expect(find.textContaining('已同步'), findsNothing);
    expect(find.byType(CircularProgressIndicator), findsNothing, reason: '原生软件从不转圈');
  });

  testWidgets('空档摘要：今天屏那行与课表屏那行是同一个值', (tester) async {
    final store = await pump(tester, DemoCase.full);
    final t = store.todayShape();
    expect(t.gap, isNotNull);
    final sentence = '今天 ${t.classes} 节课 · 最近的空档';
    expect(find.textContaining(sentence, findRichText: true), findsOneWidget);

    await tester.tap(find.text('课表'));
    await tester.pumpAndSettle();
    expect(find.textContaining(sentence, findRichText: true), findsOneWidget,
        reason: '两屏共用同一个函数算出来的那句（桌面 §8.8 第 7 条同一条硬约束）');
  });

  testWidgets('课表屏：空闲显式写出来、「下一节」不只靠颜色', (tester) async {
    await pump(tester, DemoCase.full);
    await tester.tap(find.text('课表'));
    await tester.pumpAndSettle();

    expect(find.text('一'), findsOneWidget);
    expect(find.text('日'), findsOneWidget);
    // 周五五大节只有三节有课，剩下两节必须写着「空闲」而不是留白
    expect(find.text('空闲'), findsNWidgets(2), reason: '这张表最有用的一件事是找空档');
    // 「下一节」= 描边 + 文字两个通道，而且是下一节不是「此刻」
    expect(find.text('下一节'), findsOneWidget);
    expect(find.textContaining('14:00'), findsWidgets);
  });

  testWidgets('‹ › 翻的是周：日期与 7 日条跟着走，且不冒充今天', (tester) async {
    await pump(tester, DemoCase.full);
    await tester.tap(find.text('课表'));
    await tester.pumpAndSettle();

    // 演示时钟是 2026-10-30 周五，第 9 周
    expect(find.text('第 9 周'), findsOneWidget);
    expect(find.text('10 月 30 日 · 周五 · 第 9 周'), findsOneWidget);
    expect(find.text('下一节'), findsOneWidget, reason: '本周的今天确实有下一节');

    await tester.tap(find.text('›'));
    await tester.pumpAndSettle();

    expect(find.text('第 10 周'), findsOneWidget);
    expect(find.text('11 月 6 日 · 周五 · 第 10 周'), findsOneWidget,
        reason: '同一格星期，日期要整周挪过去');
    expect(find.text('回本周'), findsOneWidget);
    expect(find.text('左右滑切天'), findsNothing, reason: '那一刻要的是回去的路，不是手势提示');
    expect(find.text('下一节'), findsNothing, reason: '别周排出来的是预告，不是此刻');

    await tester.tap(find.text('回本周'));
    await tester.pumpAndSettle();
    expect(find.text('第 9 周'), findsOneWidget);
    expect(find.text('左右滑切天'), findsOneWidget);
    expect(find.text('下一节'), findsOneWidget);
    expect(tester.takeException(), isNull, reason: '顶栏那一行最挤的一档是「第 N 周 + 提示 + 两个 48dp」');
  });

  testWidgets('翻到学期头尾：箭头按不动，也看得出来按不动', (tester) async {
    await pump(tester, DemoCase.full);
    await tester.tap(find.text('课表'));
    await tester.pumpAndSettle();

    for (var i = 0; i < 8; i++) {
      await tester.tap(find.text('‹'));
      await tester.pumpAndSettle();
    }
    expect(find.text('第 1 周'), findsOneWidget);

    final back = tester.widget<IconButton48>(
        find.ancestor(of: find.text('‹'), matching: find.byType(IconButton48)));
    expect(back.onTap, isNull, reason: '到头的按钮该淡掉，而不是点了什么都不发生');
    // 另一头还在：往回翻一格翻得动
    await tester.tap(find.text('›'));
    await tester.pumpAndSettle();
    expect(find.text('第 2 周'), findsOneWidget);
  });

  testWidgets('7 日条每格 ≥48dp（360dp 宽下跟着页边距缩进去就只有 45dp）', (tester) async {
    await pump(tester, DemoCase.full);
    await tester.tap(find.text('课表'));
    await tester.pumpAndSettle();

    for (final name in ['一', '二', '三', '四', '五', '六', '日']) {
      final cell = find.ancestor(of: find.text(name), matching: find.byType(InkWell)).first;
      final size = tester.getSize(cell);
      expect(size.width, greaterThanOrEqualTo(Metrics.touch),
          reason: '「$name」这一格只有 ${size.width.toStringAsFixed(1)}dp');
      expect(size.height, greaterThanOrEqualTo(Metrics.touch));
    }
  });

  testWidgets('输入条常驻：记一条就落「随时」，并立刻算待上传', (tester) async {
    final store = await pump(tester, DemoCase.full);
    final pendingBefore = store.pending;

    await tester.enterText(find.byType(TextField), '回导师的邮件');
    await tester.tap(find.byIcon(Icons.arrow_upward));
    await tester.pumpAndSettle();

    expect(store.todos.any((t) => t.title == '回导师的邮件'), true);
    expect(store.pending, pendingBefore + 1, reason: '手机上写的这条还没传出去');
    // 没有截止时间 → 「随时」（§5.3），不在这里替用户选今天
    final created = store.todos.firstWhere((t) => t.title == '回导师的邮件');
    expect(created.dueAt, isNull);
    expect(created.sectionOf(store.now), Section.any);
    expect(store.sections[Section.today]!.any((t) => t.id == created.id), false);

    // 它在屏幕外，滚动到它 —— 证明"落进列表"这件事真发生在界面上，不只是状态里
    await tester.scrollUntilVisible(find.text('回导师的邮件'), 200,
        scrollable: find.byType(Scrollable).first);
    expect(find.text('回导师的邮件'), findsOneWidget);
    expect(find.byKey(ValueKey('cb:${created.id}')), findsOneWidget);
  });
}
