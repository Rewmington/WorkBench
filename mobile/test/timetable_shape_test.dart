/// 同一个状态在两屏必须说同一句话（批次 8 的 N15，MuMu 上拍出来的）。
///
/// 课已经同步进来了、只是开学日没设时，今天屏以前共用 `noTimetable` 那一格，于是说
/// 「这台手机还没同步到课表」—— 那句教的动作（去点同步）此时是错的，点了也不会变；
/// 而课表屏同一时刻说的是「电脑上的开学日还没设」。两屏互相打脸，正是 §5.5 点名的翻车方式。
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/timetable.dart';

const Size frame = Size(360, 800);

ClassRow aClass() => const ClassRow(
      id: 'c1',
      term: '2026-2027-1',
      name: '高等数学',
      teacher: '某老师',
      room: '3102',
      day: 1,
      startMin: 570,
      endMin: 650,
      period: '第一大节',
      weeks: '1-16',
    );

Future<Store> pumpStore(WidgetTester tester, {required List<ClassRow> classes, String? week1}) async {
  tester.view.physicalSize = frame;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  final store = Store(
    clock: Clock.demo(2026, 11, 2, 9, 0),
    classes: classes,
    week1Monday: week1,
  );
  await tester.pumpWidget(AnvilApp(store: store, forcedBrightness: Brightness.dark));
  await tester.pumpAndSettle();
  return store;
}

void main() {
  testWidgets('有课但没设开学日：今天屏说「开学日还没设」，不谎称没同步到课表', (tester) async {
    await pumpStore(tester, classes: [aClass()], week1: null);
    expect(find.text('这台手机还没同步到课表'), findsNothing, reason: '课明明在库里，那句会把人推去点同步');
    expect(find.text('电脑上的开学日还没设'), findsOneWidget);
  });

  testWidgets('真的还没同步到课表：才说那句', (tester) async {
    await pumpStore(tester, classes: const [], week1: null);
    expect(find.text('这台手机还没同步到课表'), findsOneWidget);
    expect(find.text('电脑上的开学日还没设'), findsNothing);
  });
}
