/// 凭据与设置页那几行的数据源：应用密码只进 Keystore（测试里是内存实现），
/// 密码框留空 = 沿用已存那条；没配齐就同步，返回的是「先去填」而不是一条失败记录。
library;

import 'dart:convert';

import 'package:anvil_mobile/credentials.dart';
import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/sync/service.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

import 'support/dav_stub.dart';

const fixedNow = 1_700_000_000_000;
const user = 'me@example.com';
const pass = 'app-pass';

void main() {
  setUpAll(sqfliteFfiInit);

  test('没配凭据时 run() 返回 null，settings 里也不该有密码', () async {
    final db = await openMemoryDatabase(databaseFactoryFfi);
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final vault = MemoryPasswordVault();
    final service = SyncService(
      db: db,
      repo: repo,
      vault: vault,
      nowMs: () => fixedNow,
      base: 'http://127.0.0.1:1/dav/workbench-sync/',
    );

    expect(await service.configured, isFalse);
    expect(await service.run(), isNull, reason: '那是「先去填」，不是失败记录');

    await service.saveAccount(user, pass);
    expect(await service.configured, isTrue);
    expect(await service.email, user);
    expect(await vault.read(), pass);

    // 密码只进 Keystore：整张 settings 不该出现它
    final rows = await db.query('settings');
    final dump = jsonEncode(rows);
    expect(dump.contains(pass), isFalse, reason: '应用密码不进数据库（§4）');
  });

  test('密码框留空 = 沿用已存那条，不是清空', () async {
    final db = await openMemoryDatabase(databaseFactoryFfi);
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final vault = MemoryPasswordVault(pass);
    final service = SyncService(
      db: db,
      repo: repo,
      vault: vault,
      nowMs: () => fixedNow,
    );

    await service.saveAccount(user, '');
    expect(await vault.read(), pass);
    expect(await service.configured, isTrue, reason: '邮箱新填、密码沿用，就算配齐了');
  });

  test('配齐后对着本地桩真跑一次：成功、待上传清零、状态入库', () async {
    final stub = await DavStub.start(user: user, pass: pass);
    addTearDown(stub.close);
    final db = await openMemoryDatabase(databaseFactoryFfi);
    await writeSetting(db, 'device.id', 'phone-a', fixedNow, 'phone-a');
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final service = SyncService(
      db: db,
      repo: repo,
      vault: MemoryPasswordVault(pass),
      nowMs: () => fixedNow,
      base: stub.base,
    );
    await service.saveAccount(user, '');
    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    await store.create('配齐了');

    final out = await service.run();
    expect(out, isNotNull);
    expect(out!.ok, isTrue, reason: out.error ?? '');
    expect(out.uploaded, isTrue);
    expect(out.pendingAfter, 0);
    expect(stub.files.containsKey('device-phone-a.enc'), isTrue);

    final state = await repo.syncState();
    expect(state['lastSyncAt'], fixedNow);
  });
}
