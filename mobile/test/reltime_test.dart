/// 「上次同步于 …」这一行的读法（批次 9 的 F5 附带）。
///
/// `agoLabel` 从前一小时之后退回 `clockTime`，于是**昨天 09:12 和今天 09:12 长得一样** ——
/// 而这正是同步那行用的函数：手机三天没连上，屏幕上仍是一个今天的钟点，
/// 没人会怀疑那份数据是旧的。现在与桌面 `relTime` 同形状：越远越具体。
library;

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/reltime.dart';
import 'package:anvil_mobile/store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

final now = DateTime(2026, 11, 2, 9, 0).millisecondsSinceEpoch;

int at(int y, int mo, int d, int h, int mi) =>
    DateTime(y, mo, d, h, mi).millisecondsSinceEpoch;

const int minute = 60000;
const int hour = 3600000;

void main() {
  group('agoLabel 越远越具体', () {
    test('十分钟之内是「刚刚」，一小时之内数分钟', () {
      expect(agoLabel(now, now - 10 * 1000), '刚刚');
      expect(agoLabel(now, now - 5 * minute), '5 分钟前');
      expect(agoLabel(now, now - 59 * minute), '59 分钟前');
    });

    test('同一天里数小时', () {
      expect(agoLabel(now, now - 3 * hour), '3 小时前');
    });

    test('跨过一天就换成日期，不再是光秃秃的钟点', () {
      expect(agoLabel(now, at(2026, 11, 1, 20, 0)), '昨天 20:00',
          reason: '这就是那条回归：以前它显示成 "20:00"，看着像今天');
      expect(agoLabel(now, at(2026, 10, 30, 9, 0)), '3 天前');
      expect(agoLabel(now, at(2026, 10, 20, 9, 0)), '10月20日');
      expect(agoLabel(now, at(2025, 12, 31, 9, 0)), '2025年12月31日');
    });

    test('没同步过与时间戳在未来各有自己的说法', () {
      expect(agoLabel(now, null), '还没同步过');
      // 写出时间戳的时钟与读它的进程之间能差几毫秒；不夹一下会念出负数
      expect(agoLabel(now, now + 4000), '刚刚');
    });
  });

  testWidgets('今天屏那颗 chip 会把「三天前」念出来', (tester) async {
    final store = Store(
      clock: Clock.demo(2026, 11, 2, 9, 0),
      sync: SyncedAt(at(2026, 10, 30, 9, 0)),
    );
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(AnvilApp(store: store, forcedBrightness: Brightness.dark));
    await tester.pumpAndSettle();

    // 空列表那一屏会把同一件事再说一遍（空态那句自带同步时间），所以这里取 chip 的原句
    expect(find.text('上次同步于 3 天前 · 0 条待上传'), findsOneWidget);
  });
}
