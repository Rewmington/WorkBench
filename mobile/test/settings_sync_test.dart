/// 设置页在「传输层真的接上了」之后的样子：三行同步入口都在、360 宽不溢出、
/// 账号表单能打开。填完之后的网络动作在 `sync_service_test.dart` 里对桩验，
/// 这里只量界面 —— 别让一个真请求把 widget 测试挂住。
library;

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/credentials.dart';
import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/medicine_service.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/sync/service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

void main() {
  setUpAll(sqfliteFfiInit);

  testWidgets('设置页三行同步入口都在，360 宽不溢出，账号表单打得开', (tester) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    // sqflite 的 ffi 实现跑在真 isolate 上，widget 测试的假时钟里 await 它会永远等下去，
    // 所以开库这一步必须放进 runAsync。
    final store = await tester.runAsync(() async {
      final db = await openMemoryDatabase(databaseFactoryFfi);
      final repo = await PhoneRepo.open(db, now: DateTime.now().millisecondsSinceEpoch);
      final service = SyncService(
        db: db,
        repo: repo,
        vault: MemoryPasswordVault(),
        nowMs: () => DateTime.now().millisecondsSinceEpoch,
        // 不会真的连：这个测试不点「同步」
        base: 'http://127.0.0.1:1/dav/workbench-sync/',
      );
      return Store.load(repo, clock: Clock.live(), syncer: service);
    });
    await tester.pumpWidget(AnvilApp(
      store: store!,
      initialPage: 'settings',
      forcedBrightness: Brightness.dark,
    ));
    await tester.pumpAndSettle();

    expect(find.text('坚果云账号'), findsOneWidget);
    expect(find.text('立即同步'), findsOneWidget);
    expect(find.text('远端设备'), findsOneWidget);
    // 没填账号时是可执行的两个字，而不是「已同步」
    expect(find.text('未填'), findsOneWidget);
    expect(find.text('还没列过'), findsOneWidget);
    expect(tester.takeException(), isNull, reason: '360 宽下不能溢出');

    await tester.tap(find.text('坚果云账号'));
    await tester.pumpAndSettle();
    expect(find.text('与电脑备份用的是同一组凭据'), findsOneWidget);
    expect(
        find.byWidgetPredicate(
            (w) => w is TextField && w.decoration?.hintText == '注册邮箱'),
        findsOneWidget);
    expect(
        find.byWidgetPredicate((w) =>
            w is TextField && w.decoration?.hintText == '应用密码（留空 = 沿用已存那条）'),
        findsOneWidget);

    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(find.text('与电脑备份用的是同一组凭据'), findsNothing);
    expect(tester.takeException(), isNull);

    // 没填账号时点「立即同步」不该是哑的：它要把人送到账号表单
    expect(find.text('去填'), findsOneWidget);
    await tester.tap(find.text('立即同步'));
    await tester.pumpAndSettle();
    expect(find.text('与电脑备份用的是同一组凭据'), findsOneWidget);
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  /// 批次 9 的 F5：这台手机**只在打开 app 时同步一次**，不刷后台也不定时（PLAN-MOBILE §1）。
  /// 那个窄口径本身是有意的，但从前这里只说「打开 app 时也会自动同步一次」，
  /// 听着像它平时也会自己刷 —— 于是「电脑上改了、手机没更新」没人知道是没刷而不是没同步上。
  testWidgets('账号已填、没有错误时，那一行要说清「只有打开时才同步」', (tester) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    final store = await tester.runAsync(() async {
      final db = await openMemoryDatabase(databaseFactoryFfi);
      final repo = await PhoneRepo.open(db, now: DateTime.now().millisecondsSinceEpoch);
      final service = SyncService(
        db: db,
        repo: repo,
        vault: MemoryPasswordVault('app-pass'),
        nowMs: () => DateTime.now().millisecondsSinceEpoch,
        base: 'http://127.0.0.1:1/dav/workbench-sync/',
      );
      // 只存凭据，不发请求：这一条要看的是「配好了、还没试过同步」那一格文案
      await service.saveAccount('me@example.com', 'app-pass');
      return Store.load(repo, clock: Clock.live(), syncer: service);
    });
    await tester.pumpWidget(AnvilApp(
      store: store!,
      initialPage: 'settings',
      forcedBrightness: Brightness.dark,
    ));
    await tester.pumpAndSettle();

    expect(store.accountSaved, isTrue);
    expect(find.textContaining('只在打开 app 时同步'), findsOneWidget);
    expect(find.textContaining('平时放着不会自己更新'), findsOneWidget);
    expect(find.textContaining('也会自动同步'), findsNothing, reason: '那句会让人以为它平时会自己刷');
    expect(tester.takeException(), isNull, reason: '句子变长后 360 宽仍不许溢出');
  });

  testWidgets('药箱有独立账号入口：留空就用上面那组', (tester) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    final store = await tester.runAsync(() async {
      final db = await openMemoryDatabase(databaseFactoryFfi);
      final repo = await PhoneRepo.open(db, now: DateTime.now().millisecondsSinceEpoch);
      final sync = SyncService(
        db: db,
        repo: repo,
        vault: MemoryPasswordVault(),
        nowMs: () => DateTime.now().millisecondsSinceEpoch,
        base: 'http://127.0.0.1:1/dav/workbench-sync/',
      );
      final med = MedicineService(
        repo: repo,
        vault: MemoryPasswordVault(),
        fallbackVault: MemoryPasswordVault(),
        nowMs: () => DateTime.now().millisecondsSinceEpoch,
        base: 'http://127.0.0.1:1/dav/medicine-sync/',
      );
      return Store.load(
        repo,
        clock: Clock.live(),
        syncer: sync,
        medicineService: med,
      );
    });
    await tester.pumpWidget(AnvilApp(
      store: store!,
      initialPage: 'settings',
      forcedBrightness: Brightness.dark,
    ));
    await tester.pumpAndSettle();

    await tester.scrollUntilVisible(find.text('药箱账号'), 200,
        scrollable: find.byType(Scrollable).first);
    expect(find.text('药箱账号'), findsOneWidget);
    expect(find.text('同上方'), findsOneWidget);
    expect(find.text('先填药箱账号（或用上面那组）'), findsOneWidget);
    expect(tester.takeException(), isNull, reason: '360 宽下不能溢出');

    await tester.tap(find.text('药箱账号'));
    await tester.pumpAndSettle();
    expect(find.text('药箱在另一个坚果云账号时填这里；邮箱留空 = 用上面那组'), findsOneWidget);
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('设置在无路由可弹时，返回键先回今天页而不是退出 app', (tester) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    final store = await tester.runAsync(() async {
      final db = await openMemoryDatabase(databaseFactoryFfi);
      final repo = await PhoneRepo.open(db, now: DateTime.now().millisecondsSinceEpoch);
      return Store.load(repo, clock: Clock.live());
    });
    await tester.pumpWidget(AnvilApp(
      store: store!,
      initialPage: 'today',
      forcedBrightness: Brightness.dark,
    ));
    await tester.pumpAndSettle();

    // 从今天页点齿轮进设置
    await tester.tap(find.text('⚙'));
    await tester.pumpAndSettle();
    expect(find.text('设置'), findsOneWidget);

    // 安卓系统返回：应回到今天页
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();
    expect(find.text('设置'), findsNothing);
    expect(find.text('⚙'), findsWidgets);
    expect(tester.takeException(), isNull);
  });
}
