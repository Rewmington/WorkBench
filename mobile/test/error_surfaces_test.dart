/// 两处"故障长得像缺席"的钉子（批次 8 的 N18 / N19，都在 MuMu 复核过前提）。
library;

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/ui/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const Size frame = Size(360, 800);

Future<void> pumpStore(WidgetTester tester, Store store) async {
  tester.view.physicalSize = frame;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(AnvilApp(store: store, forcedBrightness: Brightness.dark));
  await tester.pumpAndSettle();
}

void main() {
  group('药箱读挂了 ≠ 没药', () {
    testWidgets('没有缓存又读到错误：主屏要给一行，不能整卡不画', (tester) async {
      await pumpStore(
          tester, Store(clock: Clock.demo(2026, 11, 2, 9, 0), medicineError: '401 Unauthorized'));
      expect(find.textContaining('药箱这次没读到'), findsOneWidget,
          reason: '用药这件事以前在主屏完全沉默，错误只写在设置页那一小格里');
      expect(find.textContaining('401'), findsWidgets);
    });

    testWidgets('真的没配 / 没药时仍然不画，不制造噪音', (tester) async {
      await pumpStore(tester, Store(clock: Clock.demo(2026, 11, 2, 9, 0)));
      expect(find.textContaining('药箱这次没读到'), findsNothing);
    });
  });

  group('同步失败 ≠ 还没同步过（真机上一句 401 曾被「还没同步过」盖掉）', () {
    testWidgets('试过但没成：那行要红着说「上次同步没成」', (tester) async {
      final store = Store(clock: Clock.demo(2026, 11, 2, 9, 0));
      store.syncError = '坚果云拒绝了凭据：检查账号是否填的是注册邮箱';
      await pumpStore(tester, store);
      expect(find.textContaining('上次同步没成'), findsOneWidget);
      expect(find.text('还没同步过'), findsNothing);
    });

    testWidgets('真没试过：才说「还没同步过」', (tester) async {
      await pumpStore(tester, Store(clock: Clock.demo(2026, 11, 2, 9, 0)));
      expect(find.text('还没同步过'), findsOneWidget);
      expect(find.textContaining('上次同步没成'), findsNothing);
    });
  });

  group('只改邮箱、密码留空 = 换掉加密口令', () {    // 口令是 `邮箱:应用密码` 派生的（SyncService.run），而"留空 = 沿用已存"是明写的约定。
    // 两条叠起来，改个拼错的邮箱就会让新旧文件互相读不懂 —— 保存前必须先问。
    test('该问的只有一种：有旧邮箱、邮箱变了、密码留空', () {
      expect(
          changesTheKey(
              previousEmail: 'a@x.com', nextEmail: 'b@x.com', passwordLeftBlank: true),
          isTrue);
      expect(
          changesTheKey(
              previousEmail: 'a@x.com', nextEmail: 'a@x.com', passwordLeftBlank: true),
          isFalse,
          reason: '邮箱没动就不存在换钥匙');
      expect(
          changesTheKey(
              previousEmail: 'a@x.com', nextEmail: 'b@x.com', passwordLeftBlank: false),
          isFalse,
          reason: '连密码一起重填 = 用户本来就在换一组凭据，不该再多问一句');
      expect(
          changesTheKey(
              previousEmail: null, nextEmail: 'b@x.com', passwordLeftBlank: true),
          isFalse,
          reason: '第一次填账号没有旧口令可换');
      expect(
          changesTheKey(
              previousEmail: '', nextEmail: 'b@x.com', passwordLeftBlank: true),
          isFalse);
    });
  });
}
