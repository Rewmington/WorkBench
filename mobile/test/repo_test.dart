/// 手机本机库的写路径：每一条都得把行标脏（§3.4），否则那行永远上不了云。
/// 与 Rust `every_local_write_path_marks_the_row_pending` 同一组钉子，剩下两条写路径
/// （勾选、新建）就是手机端全部。
library;

import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/sync/merge.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

const fixedNow = 1_700_000_000_000; // 2023-11-14 周二

Future<Database> memoryDb() => openMemoryDatabase(databaseFactoryFfi);

void main() {
  setUpAll(sqfliteFfiInit);

  test('新建与勾选各标一行脏；清脏后再写又重新标脏', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final store = await Store.load(repo, clock: Clock(() => fixedNow));

    final t = await store.create('买猫粮');
    expect(t.id.startsWith('${repo.deviceId}-'), isTrue, reason: 'id 带上设备号，远端一眼看出来源');
    expect(await pending(db), 1, reason: '新建要传出去');
    expect(t.dirty, isTrue);

    await store.setDone(t.id, true);
    expect(await pending(db), 1, reason: '勾选也是一次本地写入');
    var row = (await db.query('todos', where: 'id = ?', whereArgs: [t.id])).single;
    expect(row['completed'], 1);
    expect(row['done_at'], fixedNow);
    expect(row['updated_by'], repo.deviceId);
    expect(row['rev'], 2);

    // 传上去了
    await clearDirty(db, fixedNow);
    expect(await pending(db), 0);

    // 取消勾选：0002 之后它是一次真写入，不是「只推 rev」；完成时刻保留
    await store.setDone(t.id, false);
    expect(await pending(db), 1);
    row = (await db.query('todos', where: 'id = ?', whereArgs: [t.id])).single;
    expect(row['completed'], 0);
    expect(row['done_at'], fixedNow, reason: '取消勾选不能清空最后完成时间');
    expect(row['rev'], 3);
  });

  test('勾选不会抹掉模型里没有的列（提醒、完成日、创建日/时）', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await db.insert('todos', {
      'id': 'pc-1',
      'rev': 1,
      'created_at': fixedNow - 1000,
      'created_by': 'pc-a',
      'updated_at': fixedNow - 1000,
      'updated_by': 'pc-a',
      'title': '电脑建的',
      'note': '',
      'remind_at': fixedNow + 5000,
      'priority': 1,
      'sort_key': 1024.0,
      'zone': 'inbox',
      'created_day': 20231114,
      'created_hour': 10,
      'completed': 0,
      'dirty': 0,
    });
    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    await store.setDone('pc-1', true);
    final row = (await db.query('todos', where: "id = 'pc-1'")).single;
    expect(row['remind_at'], fixedNow + 5000);
    expect(row['created_day'], 20231114);
    expect(row['created_hour'], 10);
    expect(row['priority'], 1);
  });

  test('课表按学期过滤，总周数从周次集合的最大端推', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await writeSetting(db, keyTerm, '2026-2027-1', fixedNow, repo.deviceId);
    for (final (term, weeks) in [('2026-2027-1', '1-10,12-16'), ('2025-2026-2', '1-20')]) {
      await db.insert('classes', {
        'id': 'cls-$term',
        'rev': 1,
        'created_at': fixedNow,
        'created_by': 'pc-a',
        'updated_at': fixedNow,
        'updated_by': 'pc-a',
        'term': term,
        'name': '数据结构',
        'teacher': '',
        'room': '',
        'day': 5,
        'start_min': 480,
        'end_min': 580,
        'period': '第一大节',
        'weeks': weeks,
      });
    }
    await db.insert('classes', {
      'id': 'gone',
      'rev': 1,
      'created_at': fixedNow,
      'created_by': 'pc-a',
      'updated_at': fixedNow,
      'updated_by': 'pc-a',
      'deleted_at': fixedNow,
      'term': '2026-2027-1',
      'name': '删掉的课',
      'teacher': '',
      'room': '',
      'day': 1,
      'start_min': 600,
      'end_min': 700,
      'period': '',
      'weeks': '1-30',
    });

    expect(await repo.term(), '2026-2027-1');
    final rows = await repo.classesFor(await repo.term());
    expect(rows.length, 2, reason: '只带当前学期（墓碑行也带，同步要看得见）');
    expect(await repo.totalWeeks('2026-2027-1'), 16, reason: '不被别的学期与墓碑的 30 周带跑');
  });

  test('同步状态与 meta 落库后，重新打开 store 能原样读回', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await repo.saveMeta(
      {'timetable.term': '2026-2027-1', 'timetable.week1Monday': '2023-11-13'},
      fixedNow,
    );
    // 总周数从课表推，所以要有至少一行课（学期跟着 meta 走）
    await db.insert('classes', {
      'id': 'cls-1',
      'rev': 1,
      'created_at': fixedNow,
      'created_by': 'pc-a',
      'updated_at': fixedNow,
      'updated_by': 'pc-a',
      'term': '2026-2027-1',
      'name': '数据结构',
      'teacher': '',
      'room': '',
      'day': 5,
      'start_min': 480,
      'end_min': 580,
      'period': '第一大节',
      'weeks': '1-16',
    });
    await repo.saveSyncState({
      'lastSyncAt': fixedNow,
      'lastError': null,
      'lastErrorAt': null,
      'peers': ['pc-a'],
    }, fixedNow);

    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    expect(store.week1Monday, '2023-11-13');
    expect(store.currentWeek, isNotNull);
    expect(store.sync, isA<SyncedAt>());
    expect(store.peers, ['pc-a']);
    expect(store.pending, 0);
  });

  test('失败时间比成功新时是离线态；从没成功过就是「还没同步过」', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);

    await repo.saveSyncState({
      'lastSyncAt': fixedNow,
      'lastError': '连不上',
      'lastErrorAt': fixedNow + 100,
    }, fixedNow);
    var store = await Store.load(repo, clock: Clock(() => fixedNow));
    expect(store.sync, isA<OfflineAt>());
    expect(store.syncError, '连不上');

    await repo.saveSyncState({
      'lastSyncAt': null,
      'lastError': '连不上',
      'lastErrorAt': fixedNow,
    }, fixedNow);
    store = await Store.load(repo, clock: Clock(() => fixedNow));
    expect(store.sync, isA<NeverSynced>());
  });
}
