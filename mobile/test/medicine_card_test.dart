/// 今天屏用药卡：三种状态都在、点开是只读详情、没有打卡入口、360 宽不溢出。
library;

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/demo.dart';
import 'package:anvil_mobile/medicine.dart';
import 'package:anvil_mobile/store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const Size frame = Size(360, 800);

Future<Store> pump(WidgetTester tester, DemoCase c) async {
  tester.view.physicalSize = frame;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(AnvilApp(
    store: demoStore(c),
    demo: c,
    forcedBrightness: Brightness.dark,
  ));
  await tester.pumpAndSettle();
  return tester.widget<AnvilApp>(find.byType(AnvilApp)).store;
}

void main() {
  testWidgets('卡片说清三件事：今天几次、下一次、哪种药快没了', (tester) async {
    final store = await pump(tester, DemoCase.med);
    expect(store.todayMedicine?.taken, 2);
    expect(store.todayMedicine?.planned, 4);

    expect(find.text('用药'), findsOneWidget);
    expect(find.text('今天 2/4 次'), findsOneWidget);
    expect(find.text('下一次 20:00 · 钙片'), findsOneWidget);
    expect(find.text('药箱 12:03'), findsOneWidget, reason: '必须说清这份数据是什么时候发布的');
    expect(find.text('已用完：降压药'), findsOneWidget);
    expect(find.text('维生素D 剩 2 天'), findsOneWidget);
    expect(tester.takeException(), isNull, reason: '360 宽下不能溢出');
  });

  testWidgets('点开是只读详情：来源时间 + 每格状态 + 回药箱打卡的提示', (tester) async {
    await pump(tester, DemoCase.med);
    await tester.tap(find.text('用药'));
    await tester.pumpAndSettle();

    expect(find.text('只读'), findsOneWidget);
    expect(find.text('药箱发布于 12:03 · 读取于 12:34'), findsOneWidget);
    expect(find.text('这里只读；回药箱打卡并同步后才会更新'), findsOneWidget);
    expect(find.text('08:00 已服'), findsNWidgets(2));
    expect(find.text('20:00 未到'), findsNWidgets(2));
    // 只读：没有任何打卡按钮
    expect(find.widgetWithText(FilledButton, '打卡'), findsNothing);
    expect(find.widgetWithText(ElevatedButton, '打卡'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('关掉开关后卡片消失；没有药箱数据时也不画空卡', (tester) async {
    final store = await pump(tester, DemoCase.med);
    await store.setMedicineEnabled(false);
    await tester.pumpAndSettle();
    expect(find.text('用药'), findsNothing);

    await pump(tester, DemoCase.full);
    expect(find.text('用药'), findsNothing, reason: '没读过药箱时不该出现空卡');
  });

  testWidgets('全标成没吃时不说「0/0 次」', (tester) async {
    final now = demoClock().date;
    final dayStart = DateTime(now.year, now.month, now.day).millisecondsSinceEpoch;
    final view = MedicineView(
      medicines: [
        const Medicine(
          id: 'm1',
          name: '维生素D',
          perBox: 30,
          remaining: 20,
          dosesPerDay: 1,
          amountPerDose: 1,
          unit: '粒',
          lowDays: 7,
          note: '',
          times: '08:00',
          createdAt: 1,
          updatedAt: 1,
          deleted: false,
        ),
      ],
      logs: [
        DoseLog(
          id: 'l1',
          medicineId: 'm1',
          takenAt: dayStart + 8 * 3600000,
          amount: 0,
          note: '',
          updatedAt: 1,
          deleted: false,
        ),
      ],
      publishedAt: 1,
      devices: 1,
    );
    tester.view.physicalSize = frame;
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(AnvilApp(
      store: Store(clock: demoClock(), medicine: view),
      forcedBrightness: Brightness.dark,
    ));
    await tester.pumpAndSettle();
    expect(find.text('今天没有要吃的'), findsOneWidget);
    expect(find.text('今天都标记了没吃'), findsOneWidget);
  });
}
