/// 撤销条找回"我刚勾的那一条"必须按 id（批次 8 从手机盲区审计里确认的一条）。
///
/// 原来它记的是标题，于是同名两条时撤销会去改**列表里排在前面的那条**：
/// 我勾掉的没回来、没动的那条被改成未完成，而这两次写都带 dirty=1 上传出去 ——
/// 手机上最容易撞名的正是「喝水」「交作业」这类标题。
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/todo.dart';
import 'package:anvil_mobile/ui/parts.dart';

const Size frame = Size(360, 800);

Future<Store> pumpStore(WidgetTester tester, List<Todo> todos) async {
  tester.view.physicalSize = frame;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  final store = Store(clock: Clock.demo(2026, 11, 2, 9, 0), todos: todos);
  await tester.pumpWidget(AnvilApp(store: store, forcedBrightness: Brightness.dark));
  await tester.pumpAndSettle();
  return store;
}

Todo todo(String id, {required String title, bool done = false}) => Todo(
      id: id,
      title: title,
      createdAt: 1,
      createdBy: 'pc-a',
      updatedAt: 2,
      updatedBy: 'pc-a',
      completed: done,
      doneAt: done ? 3 : null,
      sortKey: 1024,
    );

void main() {
  testWidgets('两条同名待办：撤销回到我刚勾的那一条，不是排在前面的那条', (tester) async {
    // 「先完成的那条」排在列表前面 —— 按标题找就会被它劫走，这正是这条测试要钉住的形状
    final store = await pumpStore(tester, [
      todo('a', title: '喝水', done: true),
      todo('b', title: '喝水'),
    ]);

    await tester.tap(find.byKey(const ValueKey('cb:b')));
    await tester.pump();
    expect(find.byType(UndoBar), findsOneWidget, reason: '勾掉之后撤销条要在');
    expect(store.todos.firstWhere((t) => t.id == 'b').completed, true);

    await tester.tap(find.text('撤销'));
    await tester.pumpAndSettle();

    expect(store.todos.firstWhere((t) => t.id == 'b').completed, false, reason: '我勾的那条必须回来');
    expect(store.todos.firstWhere((t) => t.id == 'a').completed, true, reason: '另一条同名的不该被动过');
  });

  testWidgets('撤销条上写的还是那条的标题', (tester) async {
    final store = await pumpStore(tester, [
      todo('a', title: '喝水', done: true),
      todo('b', title: '交实验报告'),
    ]);
    await tester.tap(find.byKey(const ValueKey('cb:b')));
    await tester.pump();
    expect(find.text('交实验报告'), findsWidgets);
    final bar = tester.widget<UndoBar>(find.byType(UndoBar));
    expect(bar.title, '交实验报告');
    expect(store.todos.firstWhere((t) => t.id == 'a').completed, true);
  });
}
