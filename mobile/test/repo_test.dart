/// 手机本机库的写路径：每一条都得把行标脏（§3.4），否则那行永远上不了云。
/// 与 Rust `every_local_write_path_marks_the_row_pending` 同一组钉子，剩下两条写路径
/// （勾选、新建）就是手机端全部。
library;

import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/sync/merge.dart';
import 'package:anvil_mobile/timetable.dart';
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

  /// 桌面 PC 的 `iso_to_day` 和这里必须拒同一批坏值。开学日一旦存坏，今天屏只该「算不出
  /// 这一周」，不该让一个 `int.parse` / 下标越界把 UI build 崩掉。
  test('坏开学日不进本机设置，weekOf 也只回 null', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await repo.saveMeta(const {
      'timetable.term': '2026-2027-1',
      'timetable.week1Monday': '开学再说',
    }, fixedNow);

    expect(await repo.term(), '2026-2027-1');
    expect(await repo.week1Monday(), isNull, reason: '格式不对的 meta 不该把本机库污染掉');
    expect(isoToDayOrNull('2026-09'), isNull);
    expect(isoToDayOrNull('2026-02-31'), isNull);
    expect(isoToDayOrNull('2026-09-07'), 20260907);
    expect(weekOf('开学再说', DateTime(2026, 11, 2), 16), isNull);
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

  /// 与 PC 的 `forward_of` 同一条规则，两端各钉一遍（规则只有一句，改一边这里会红）。
  test('墙钟回拨不会让本机写出更旧的时间戳', () {
    expect(Clock.forwardOf(200, 100), 200, reason: '正常前进就照墙钟');
    expect(Clock.forwardOf(100, 100), 100, reason: '同一毫秒里的多次写入不该 +1');
    expect(Clock.forwardOf(50, 100), 101, reason: '回拨时前进一格，而不是退回 50');
  });

  /// P1-14 在手机上的形状，而且比 PC 更危险：`device.id` 读不出来时**不许**换一把新的写回去。
  /// id 一变，本机每一行的 `updated_by` 就成了「别人写的」—— 平票裁决、待上传计数、
  /// 靠本机历史垫高的时间下限全押在这个串上。PC 那边写回的是一个默认值，这边写掉的是身份本身，
  /// 所以这里的处置是拒绝启动，而不是折成「没设过」再生成。
  test('device.id 读不出来时不换身份，坏的那行原样留着', () async {
    final db = await memoryDb();
    await db.insert('settings', {
      'key': 'device.id',
      'value_json': '"phone-abc', // 半截 JSON：解不开，但不是没设过
      'updated_at': fixedNow,
      'updated_by': 'phone-abc',
    });

    await expectLater(
        loadOrCreateDeviceId(db, fixedNow), throwsA(isA<StateError>()));

    final row = (await db.query('settings', where: "key = 'device.id'")).single;
    expect(row['value_json'], '"phone-abc', reason: '坏的那条是唯一线索，不能被一把新 id 盖掉');
  });

  test('device.id 是合法 JSON 但形状不对时也不能换身份', () async {
    final db = await memoryDb();
    await db.insert('settings', {
      'key': 'device.id',
      'value_json': '123',
      'updated_at': fixedNow,
      'updated_by': 'phone-abc',
    });

    await expectLater(
        loadOrCreateDeviceId(db, fixedNow), throwsA(isA<StateError>()));

    final row = (await db.query('settings', where: "key = 'device.id'")).single;
    expect(row['value_json'], '123', reason: '能解开不代表能用作身份；旧行仍是唯一线索');
  });

  /// 「上次同步的状态读不出来」与「还没同步过」在界面上必须是两句话：后者会让人以为手机从没
  /// 上过云（然后重做一遍动作），前者点一次同步就能自愈。占的是已有的 `syncError` 那一行。
  test('sync.state 坏了说「读不出来」，不冒充「还没同步过」', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    expect(await repo.syncStateUnreadable(), isFalse, reason: '真没同步过不算故障');

    Future<void> putState(String raw) => db.insert(
          'settings',
          {
            'key': keySyncState,
            'value_json': raw,
            'updated_at': fixedNow,
            'updated_by': repo.deviceId,
          },
          conflictAlgorithm: ConflictAlgorithm.replace,
        );

    await putState('{解不开');
    expect(await repo.syncStateUnreadable(), isTrue);
    expect(await repo.syncState(), isEmpty, reason: '内容侧仍是空，故障由那一句单独说');
    var store = await Store.load(repo, clock: Clock(() => fixedNow));
    expect(store.syncStateUnreadable, isTrue, reason: '故障由那一条状态说，不混进 syncError');

    // 解得开、但根本不是那个对象：同样没法用来讲「上次同步于几点」
    await putState('"就是一串字"');
    expect(await repo.syncStateUnreadable(), isTrue);

    // 一次真同步把那条重写回来，红行就该消失 —— 不是一条需要人去清的残留
    await repo.saveSyncState({'lastSyncAt': fixedNow}, fixedNow);
    expect(await repo.syncStateUnreadable(), isFalse);
    store = await Store.load(repo, clock: Clock(() => fixedNow));
    expect(store.syncStateUnreadable, isFalse, reason: '一次真同步重写那条之后，故障口径要自己消失');
  });

  /// 手机上完成的那一格也要进统计：桌面三处（今天完成数、热力图、连续天数）只读 `done_day`，
  /// 手机不写那一列 = 在手机上完成的事永远不存在。取消勾选时那一格留着不动（与 PC 同一句规则）。
  test('手机上勾选会落下 done_day，取消时不清空', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    final t = await store.create('打疫苗');

    await store.setDone(t.id, true);
    var row = (await db.query('todos', where: 'id = ?', whereArgs: [t.id])).single;
    expect(
        row['done_day'],
        dayInt(DateTime.fromMillisecondsSinceEpoch(row['done_at'] as int)),
        reason: 'done_day 是 done_at 的另一半，只写一半桌面那三处统计就读不到');

    await store.setDone(t.id, false);
    row = (await db.query('todos', where: 'id = ?', whereArgs: [t.id])).single;
    expect(row['completed'], 0);
    expect(row['done_day'], isNotNull, reason: '取消勾选不清空完成日 —— 与 PC set_done 同一句规则');
  });
}
