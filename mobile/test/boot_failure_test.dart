/// 起不来那一屏（批次 8 的 N16，MuMu 上把库设成只读拍出来的）。
///
/// 真机实测：`anvil.db` 不可写时 `main()` 里那条 await 直接抛，`runApp` 从没执行，
/// 屏幕上只剩启动图标 —— 而"点开没反应"最容易让人去卸载重装，那会连数据一起删。
/// 所以这一屏存在的意义就是那一句「先别卸载重装」。
library;

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:anvil_mobile/main.dart';

void main() {
  testWidgets('启动失败那一屏：说清别卸载、给常见原因、带上原始那句', (tester) async {
    await tester.pumpWidget(const BootFailureApp(
      reason: 'DatabaseException(not an error (code 0 SQLITE_OK): '
          'Could not open the database in read/write mode.)',
    ));

    expect(find.text('Anvil 打不开本机的数据'), findsOneWidget);
    expect(find.textContaining('先别卸载重装'), findsOneWidget,
        reason: '卸载会连数据一起删，这句必须最先看到');
    expect(find.textContaining('read/write mode'), findsOneWidget,
        reason: '原始原因要原样带出去，否则没法查');
    // 这一屏不许溢出：真机横屏（逻辑高只有 ~410dp）时最容易翻红屏
    expect(tester.takeException(), isNull);
  });

  testWidgets('横屏窄高也不溢出', (tester) async {
    tester.view.physicalSize = const Size(1920, 1080);
    tester.view.devicePixelRatio = 2.625;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(const BootFailureApp(reason: 'x'));
    expect(tester.takeException(), isNull);
    expect(find.text('Anvil 打不开本机的数据'), findsOneWidget);
  });

  testWidgets('身份读不回来时说的是那一行，不是「磁盘满了」', (tester) async {
    await tester.pumpWidget(const BootFailureApp(
      reason: 'Bad state: settings 里 device.id 那条解不开 JSON，本机身份不能换',
    ));
    expect(find.textContaining('本机身份那一行'), findsOneWidget);
    expect(find.textContaining('冲突时谁赢'), findsOneWidget,
        reason: '要说清为什么宁可起不来：换了身份，历史每行都会变成别人写的');
    expect(find.textContaining('常见原因'), findsNothing,
        reason: '那不是环境问题。给一套无关的排查清单，人会去删数据目录');
    expect(find.textContaining('先别卸载重装'), findsOneWidget);
  });

  testWidgets('身份那段在横屏窄高里也不溢出', (tester) async {
    tester.view.physicalSize = const Size(1920, 1080);
    tester.view.devicePixelRatio = 2.625;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(const BootFailureApp(
      reason: 'Bad state: settings 里 device.id 那条解不开 JSON',
    ));
    expect(tester.takeException(), isNull);
  });
}
