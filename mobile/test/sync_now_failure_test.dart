/// 同步这一路的"抛了但没人接"（批次 9 的 F8）。
///
/// `syncNow` 原先是 `try { await run(); } finally { syncing = false; }` 之后再一句
/// `await reload()` —— 后半句在 try 外面，而它的两个调用点都不 catch：开机那次是
/// `unawaited(store.syncNow())`，设置页那个 `try/finally` 只管把按钮放回原位。
/// 于是异常只进 logcat，屏幕停在「同步过了，列表还是旧的」。
/// 这里分别钉两条：`run()` 抛、`reload()` 抛。
library;

import 'package:anvil_mobile/app.dart';
import 'package:anvil_mobile/credentials.dart';
import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/sync/service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

const fixedNow = 1_700_000_000_000;

Future<Database> memoryDb() => openMemoryDatabase(databaseFactoryFfi);

/// Keystore 读不出应用密码那一种坏法（P0-4 的 c：从备份恢复后密钥不在）。
/// 先让它能读，是为了让 `Store.load` 走通；测的时候再翻脸。
class _FlakyVault implements PasswordVault {
  bool refusing = false;

  @override
  Future<String?> read() async {
    if (refusing) throw StateError('注入：Keystore 解不开');
    return 'app-pass';
  }

  @override
  Future<void> write(String password) async {}

  @override
  Future<void> clear() async {}
}

SyncService serviceFor(Database db, PhoneRepo repo, PasswordVault vault) => SyncService(
      db: db,
      repo: repo,
      vault: vault,
      nowMs: () => fixedNow,
      base: 'http://127.0.0.1:1/dav/workbench-sync/',
    );

void main() {
  setUpAll(sqfliteFfiInit);

  test('run() 抛：不外抛，并留下一句能指给用户的原话', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final vault = _FlakyVault();
    final service = serviceFor(db, repo, vault);
    await service.saveAccount('me@example.com', 'app-pass');
    final store =
        await Store.load(repo, clock: Clock(() => fixedNow), syncer: service);
    expect(store.accountSaved, isTrue);

    vault.refusing = true;
    await store.syncNow(); // 从前这一行直接把 StateError 抛给调用方

    expect(store.syncError, contains('这一轮同步没跑完'));
    expect(store.syncError, contains('Keystore 解不开'), reason: '原因要原样带出来');
    expect(store.syncing, isFalse, reason: '按钮不许卡在「同步中」');
  });

  test('远端那一步没炸、本机读回来时炸了：说的不是「还没同步过」', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    // 不存邮箱：run() 会在"还没配凭据"那一步干净返回 null，把 reload 单独留坏
    final store = await Store.load(repo,
        clock: Clock(() => fixedNow), syncer: serviceFor(db, repo, MemoryPasswordVault()));
    await db.execute('DROP TABLE todos'); // 库坏掉的一种：读回来时炸

    await store.syncNow();

    expect(store.syncError, contains('同步的结果没读回本机'));
    expect(store.syncError, contains('界面还是旧的'),
        reason: '这句要说的是"屏幕上的东西不可信"，不是"没同步过"');
    expect(store.syncing, isFalse);
  });

  testWidgets('那句原因在 chip 上看得见，不会被说成「还没同步过」', (tester) async {
    final store = Store(clock: Clock.demo(2026, 11, 2, 9, 0));
    store.syncError = '同步的结果没读回本机（界面还是旧的）：DatabaseException(no such table)';
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(AnvilApp(store: store, forcedBrightness: Brightness.dark));
    await tester.pumpAndSettle();

    expect(find.textContaining('上次同步没成'), findsOneWidget);
    expect(find.text('还没同步过'), findsNothing);
  });
}
