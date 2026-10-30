/// 药箱只读客户端：只发 PROPFIND/GET、只认 medicine-sync、解析 device-*.json；
/// 以及「失败不抹掉上次那份」的缓存规则。
library;

import 'dart:convert';

import 'package:anvil_mobile/credentials.dart';
import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/medicine.dart';
import 'package:anvil_mobile/medicine_service.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/sync/medicine_sync.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

import 'support/dav_stub.dart';

const user = 'me@example.com';
const pass = 'app-pass';
const fixedNow = 1_700_000_000_000;

String docJson(String device, int publishedAt, {String medName = '钙片', double remaining = 30}) => jsonEncode({
      'device': device,
      'published_at': publishedAt,
      'medicines': [
        {
          'id': 'm1',
          'name': medName,
          'per_box': 30.0,
          'remaining': remaining,
          'doses_per_day': 2.0,
          'amount_per_dose': 1.0,
          'unit': '片',
          'low_days': 7,
          'note': '',
          'times': '08:00,20:00',
          'created_at': 1,
          'updated_at': publishedAt,
          'deleted': 0,
        },
      ],
      'dose_logs': [
        {
          'id': 'l1',
          'medicine_id': 'm1',
          'taken_at': publishedAt,
          'amount': 1.0,
          'note': '',
          'updated_at': publishedAt,
          'deleted': 0,
        },
      ],
    });

Future<DavStub> medicineStub() async => DavStub.start(
      user: user,
      pass: pass,
      segment: '/dav/medicine-sync/',
    );

void main() {
  setUpAll(sqfliteFfiInit);

  test('目录钉子：只认 medicine-sync，拒绝工作台自己的两个目录', () {
    for (final bad in [
      'https://dav.jianguoyun.com/dav/workbench-sync/',
      'https://dav.jianguoyun.com/dav/workbench-backup/',
      'https://dav.jianguoyun.com/dav/medicine-sync',
      'https://dav.jianguoyun.com/dav/',
    ]) {
      expect(() => MedicineCloud(bad, user, pass), throwsA(isA<MedicineCloudException>()),
          reason: '$bad 不该被接受');
    }
    expect(() => MedicineCloud(medicineCloudBase, '', pass), throwsA(isA<MedicineCloudException>()));
  });

  test('读两台设备：只发 PROPFIND/GET，合并出最近发布的那份', () async {
    final stub = await medicineStub();
    addTearDown(stub.close);
    stub.dirExists = true;
    stub.files['device-phone-a.json'] = utf8.encode(docJson('phone-a', 200, remaining: 30));
    stub.files['device-phone-b.json'] = utf8.encode(docJson('phone-b', 100, remaining: 5));
    stub.files['notes.json'] = utf8.encode('{"x":1}');
    stub.files['device-old.txt'] = utf8.encode('nope');

    final cloud = MedicineCloud(stub.base, user, pass);
    final got = await cloud.fetch(nowMs: fixedNow);
    expect(got.dirMissing, isFalse);
    expect(got.docs.length, 2, reason: '只认 device-*.json');
    final view = MedicineView.merge(got.docs, fetchedAt: got.fetchedAt);
    expect(view.medicines.single.name, '钙片');
    expect(view.medicines.single.remaining, 30, reason: 'updated_at 更新的那份赢');
    expect(view.publishedAt, 200);
    expect(stub.methods.toSet(), {'PROPFIND', 'GET'},
        reason: '只读客户端类型上没有 MKCOL/PUT，一个都不该发');
  });

  test('目录不存在 = 药箱还没同步过，不当失败', () async {
    final stub = await medicineStub();
    addTearDown(stub.close);
    final cloud = MedicineCloud(stub.base, user, pass);
    final got = await cloud.fetch(nowMs: fixedNow);
    expect(got.dirMissing, isTrue);
    expect(got.docs, isEmpty);
  });

  test('401 翻成能照做的句子；坏 JSON 点名是哪个文件', () async {
    final stub = await medicineStub();
    addTearDown(stub.close);
    stub.dirExists = true;
    stub.files['device-a.json'] = utf8.encode('not json');

    final wrong = MedicineCloud(stub.base, user, 'wrong');
    await expectLater(
      wrong.fetch(nowMs: fixedNow),
      throwsA(predicate((e) => e.toString().contains('应用密码'))),
    );

    final cloud = MedicineCloud(stub.base, user, pass);
    await expectLater(
      cloud.fetch(nowMs: fixedNow),
      throwsA(predicate((e) => e.toString().contains('device-a.json'))),
    );
  });

  test('刷新成功写缓存；失败保留旧视图并记下这句话', () async {
    final stub = await medicineStub();
    stub.dirExists = true;
    stub.files['device-phone-a.json'] = utf8.encode(docJson('phone-a', 200));

    final db = await openMemoryDatabase(databaseFactoryFfi);
    await writeSetting(db, 'device.id', 'phone-a', fixedNow, 'phone-a');
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final service = MedicineService(
      repo: repo,
      vault: MemoryPasswordVault(pass),
      nowMs: () => fixedNow,
      base: stub.base,
    );
    await repo.saveEmail(user, fixedNow);

    final view = await service.refresh();
    expect(view?.medicines.single.name, '钙片');
    final cached = await repo.medicineView();
    expect(cached?.publishedAt, 200, reason: '线上那份的时间要留着，界面才敢说何时发布');
    expect(cached?.fetchedAt, fixedNow);

    // 云端断了：旧视图还在，错误单独记
    await stub.close();
    await expectLater(service.refresh(), throwsA(isA<MedicineCloudException>()));
    final kept = await repo.medicineView();
    expect(kept?.medicines.single.name, '钙片');
    final err = await repo.medicineError();
    expect(err.error, isNotNull);
    expect(err.error!.contains('\n'), isFalse, reason: '状态必须是一行话');
  });

  test('药箱独立账号：用自己的邮箱+密码，不拿同步那把去猜', () async {
    final stub = await DavStub.start(
      user: 'med@example.com',
      pass: 'med-pass',
      segment: '/dav/medicine-sync/',
    );
    addTearDown(stub.close);
    stub.dirExists = true;
    stub.files['device-phone-a.json'] = utf8.encode(docJson('phone-a', 200));

    final db = await openMemoryDatabase(databaseFactoryFfi);
    await writeSetting(db, 'device.id', 'phone-a', fixedNow, 'phone-a');
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await repo.saveEmail('sync@example.com', fixedNow);
    await repo.saveMedicineEmail('med@example.com', fixedNow);
    final medVault = MemoryPasswordVault('med-pass');
    final service = MedicineService(
      repo: repo,
      vault: medVault,
      fallbackVault: MemoryPasswordVault('sync-pass'),
      nowMs: () => fixedNow,
      base: stub.base,
    );

    expect(await service.configured, isTrue);
    expect((await service.refresh())?.medicines.single.name, '钙片');
  });

  test('填了药箱邮箱但没密码：不配齐，也不悄悄回退同步账号', () async {
    final db = await openMemoryDatabase(databaseFactoryFfi);
    await writeSetting(db, 'device.id', 'phone-a', fixedNow, 'phone-a');
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await repo.saveEmail('sync@example.com', fixedNow);
    await repo.saveMedicineEmail('med@example.com', fixedNow);
    final service = MedicineService(
      repo: repo,
      vault: MemoryPasswordVault(),
      fallbackVault: MemoryPasswordVault('sync-pass'),
      nowMs: () => fixedNow,
      base: 'http://127.0.0.1:1/dav/medicine-sync/',
    );

    expect(await service.configured, isFalse);
    expect(await service.refresh(), isNull);
  });

  test('留空邮箱保存 = 清掉独立账号，回退同步那组', () async {
    final db = await openMemoryDatabase(databaseFactoryFfi);
    await writeSetting(db, 'device.id', 'phone-a', fixedNow, 'phone-a');
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await repo.saveEmail('sync@example.com', fixedNow);
    await repo.saveMedicineEmail('med@example.com', fixedNow);
    final medVault = MemoryPasswordVault('med-pass');
    final service = MedicineService(
      repo: repo,
      vault: medVault,
      fallbackVault: MemoryPasswordVault('sync-pass'),
      nowMs: () => fixedNow,
    );

    await service.saveAccount('', '');
    expect(await repo.medicineEmail(), isNull);
    expect(await medVault.read(), isNull);
    expect(await service.configured, isTrue, reason: '回退到工作台那组');
  });
}
