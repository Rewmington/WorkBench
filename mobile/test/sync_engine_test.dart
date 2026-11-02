/// 一次同步的整条链，对本地桩跑：MKCOL → PROPFIND → GET → 合并 → PUT → 清脏。
/// 与 Rust `tests/sync_relay_e2e.rs` 同一个角色，验的是手机端这一半。
library;

import 'dart:typed_data';

import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:anvil_mobile/sync/engine.dart';
import 'package:anvil_mobile/sync/envelope.dart';
import 'package:anvil_mobile/sync/merge.dart';
import 'package:anvil_mobile/sync/snapshot.dart';
import 'package:anvil_mobile/sync/webdav.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

import 'support/dav_stub.dart';

const fixedNow = 1_700_000_000_000;
const user = 'me@example.com';
const pass = 'app-pass';
const secret = '$user:$pass';

Map<String, Object?> todoRow(String id, String title, int at, {String by = 'pc-a'}) => {
      'id': id,
      'rev': 1,
      'created_at': at,
      'created_by': by,
      'updated_at': at,
      'updated_by': by,
      'deleted_at': null,
      'title': title,
      'note': '',
      'done_at': null,
      'done_day': null,
      'due_at': null,
      'remind_at': null,
      'priority': 2,
      'pinned': 0,
      'parent_id': null,
      'sort_key': 1024.0,
      'zone': 'inbox',
      'created_day': 20231114,
      'created_hour': 9,
      'completed': 0,
      'reminded_at': null,
    };

Map<String, Object?> classRow() => {
      'id': 'cls-1',
      'rev': 1,
      'created_at': fixedNow,
      'created_by': 'pc-a',
      'updated_at': fixedNow,
      'updated_by': 'pc-a',
      'deleted_at': null,
      'term': '2026-2027-1',
      'name': '数据结构',
      'teacher': '王老师',
      'room': '3教 201',
      'day': 5,
      'start_min': 480,
      'end_min': 580,
      'period': '第一大节',
      'weeks': '1-16',
    };

Future<Database> phoneDb() async {
  final db = await openMemoryDatabase(databaseFactoryFfi);
  await writeSetting(db, 'device.id', 'phone-a', fixedNow, 'phone-a');
  return db;
}

void main() {
  setUpAll(sqfliteFfiInit);

  late DavStub stub;
  setUp(() async => stub = await DavStub.start(user: user, pass: pass));
  tearDown(() => stub.close());

  test('目录钉子：只认独立 workbench-sync，文件名只认 device-*.enc', () async {
    for (final bad in [
      'https://dav.jianguoyun.com/dav/medicine-sync/',
      'https://dav.jianguoyun.com/dav/workbench-backup/',
      'https://dav.jianguoyun.com/dav/',
      'https://dav.jianguoyun.com/dav/workbench-sync',
    ]) {
      expect(() => WebDavRemote(bad, user, pass), throwsA(isA<WebDavException>()),
          reason: '$bad 不该被接受');
    }
    expect(() => WebDavRemote(defaultBase, '', pass), throwsA(isA<WebDavException>()));
    expect(() => WebDavRemote(defaultBase, user, ' '), throwsA(isA<WebDavException>()));

    final r = WebDavRemote('http://127.0.0.1:1/dav/workbench-sync/', user, pass);
    for (final name in ['anvil-20261030.abackup', 'notes.txt', '']) {
      await expectLater(r.get(name), throwsA(predicate((e) => e.toString().contains('device-'))));
      await expectLater(
          r.put(name, const [1]), throwsA(predicate((e) => e.toString().contains('device-'))));
    }
  });

  test('备份 base 折成同父的 workbench-sync', () {
    expect(
      syncBaseFromBackupBase('https://dav.jianguoyun.com/dav/workbench-backup/'),
      'https://dav.jianguoyun.com/dav/workbench-sync/',
    );
    expect(
      syncBaseFromBackupBase('http://127.0.0.1:8791/dav/workbench-backup'),
      'http://127.0.0.1:8791/dav/workbench-sync/',
    );
  });

  /// 与 PC 的 `one_unreadable_peer_does_not_stop_the_others_from_merging` 对称：
  /// 一个读不懂的对端文件过去会把整轮带走，于是所有设备一起停摆。
  test('一个读不懂的对端不该带走整轮，但它要被点名', () async {
    final db = await phoneDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final good = Snapshot(
      device: 'pc-a',
      sealedAt: fixedNow,
      tables: {
        'todos': [todoRow('pc-1', '能读进来的那台', fixedNow - 500)],
      },
      meta: const {},
    );
    stub.files['device-pc-a.enc'] = await Envelope.seal(secret, good.toBytes());
    // 合法文件名 + 完全不是信封的字节（旧版本写的、截断的、被改坏的）
    stub.files['device-pc-b.enc'] = [0, 1, 2, 3];

    final out = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: WebDavRemote(stub.base, user, pass),
      nowMs: () => fixedNow,
    );

    expect(out.ok, isTrue, reason: out.error ?? '');
    expect(out.applied, 1, reason: '能读的那台该并进来');
    expect(out.unreadablePeers, ['pc-b']);
    expect(out.peers, containsAll(['pc-a', 'pc-b']), reason: '文件在远端就算在线');
  });

  test('拉取：别人的 device 文件合并进本地，学期/开学日跟着 meta 到位', () async {
    final db = await phoneDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final pcSnap = Snapshot(
      device: 'pc-a',
      sealedAt: fixedNow,
      tables: {
        'todos': [todoRow('pc-1', '电脑建的', fixedNow - 500)],
        'classes': [classRow()],
      },
      meta: {'timetable.term': '2026-2027-1', 'timetable.week1Monday': '2023-11-13'},
    );
    stub.files['device-pc-a.enc'] = await Envelope.seal(secret, pcSnap.toBytes());
    // 目录里混进不该被看见的东西
    stub.files['notes.txt'] = [1, 2, 3];
    stub.files['anvil-20261030T000000Z.abackup'] = [4, 5];

    final remote = WebDavRemote(stub.base, user, pass);
    final out = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: remote,
      nowMs: () => fixedNow,
    );

    expect(out.ok, isTrue, reason: out.error ?? '');
    expect(out.applied, 2, reason: 'todos 一行、classes 一行');
    expect(out.peers, ['pc-a']);
    expect(out.uploaded, isFalse, reason: '本机没有写入，不该 PUT');
    expect(out.putSkipped, isTrue);
    expect(out.pendingAfter, 0, reason: '对端带来的行不算本机欠账');
    expect(stub.files.containsKey('device-phone-a.enc'), isFalse);

    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    expect(store.todos.single.title, '电脑建的');
    expect(store.week1Monday, '2023-11-13');
    expect(store.classesToday().isEmpty, isTrue, reason: '周五课，今天是周二');
    expect(store.currentWeek, 1);
  });

  test('上传：手机写过的才出门；文件里没有 dirty、也没有 classes', () async {
    final db = await phoneDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    await store.create('手机写的');

    final remote = WebDavRemote(stub.base, user, pass);
    final out = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: remote,
      nowMs: () => fixedNow,
    );
    expect(out.ok, isTrue, reason: out.error ?? '');
    expect(out.uploaded, isTrue);
    expect(out.sealedAt, fixedNow);
    expect(await pending(db), 0, reason: '传上去了才清脏');

    final blob = stub.files['device-phone-a.enc'];
    expect(blob, isNotNull);
    final snap = Snapshot.fromBytes(await Envelope.open(secret, Uint8List.fromList(blob!)));
    expect(snap.device, 'phone-a');
    expect(snap.tables.keys, ['todos'], reason: '手机只发 todos');
    expect(snap.rows('todos').single['title'], '手机写的');
    expect(snap.rows('todos').single.containsKey('dirty'), isFalse);

    // 第二次：pending 0 → 不 PUT（§3.4 的省配额判断）
    final puts = stub.puts;
    final again = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: remote,
      nowMs: () => fixedNow,
    );
    expect(again.ok, isTrue);
    expect(again.putSkipped, isTrue);
    expect(stub.puts, puts, reason: '没有本地写入就不该占用一次 PUT');

    final state = await repo.syncState();
    expect(state['lastSyncAt'], fixedNow);
    expect(state['peers'], isEmpty, reason: '只有自己那份时远端设备列表为空');
  });

  test('断网：一句人话、不 panic、待上传数不变、上次成功时间保留', () async {
    final db = await phoneDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    await store.create('断网写的');
    await repo.saveSyncState({'lastSyncAt': fixedNow - 5000, 'peers': ['pc-a']}, fixedNow);

    final dead = WebDavRemote('http://127.0.0.1:1/dav/workbench-sync/', user, pass);
    final out = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: dead,
      nowMs: () => fixedNow,
    );
    expect(out.ok, isFalse);
    expect(out.error, isNotEmpty);
    expect(out.error!.contains('\n'), isFalse, reason: '状态必须是一行话');
    expect(out.pendingAfter, 1, reason: '远端断了不改本机欠账');

    final state = await repo.syncState();
    expect(state['lastSyncAt'], fixedNow - 5000, reason: '上一次成功的时间不能被失败抹掉');
    expect(state['lastError'], out.error);
    expect(state['peers'], ['pc-a']);
  });

  test('凭据被拒：401 翻成能照做的句子', () async {
    final db = await phoneDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final wrong = WebDavRemote(stub.base, user, 'wrong-pass');
    final out = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: wrong,
      nowMs: () => fixedNow,
    );
    expect(out.ok, isFalse);
    expect(out.error, contains('应用密码'), reason: '错因要说人话：${out.error}');
  });

  test('两端同时改同一条：手机这侧按同一句 LWW 裁决', () async {
    final db = await phoneDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    await db.insert('todos', {
      'id': 'same',
      'rev': 1,
      'created_at': fixedNow,
      'created_by': 'pc-a',
      'updated_at': fixedNow,
      'updated_by': 'bbb',
      'title': '本机这份',
      'note': '',
      'sort_key': 1024.0,
      'zone': 'inbox',
      'completed': 0,
      'dirty': 1,
    });
    // 平票，对端设备号更大 → 对端赢
    final pcSnap = Snapshot(
      device: 'ddd',
      sealedAt: fixedNow,
      tables: {
        'todos': [todoRow('same', '对端那份', fixedNow, by: 'ddd')],
      },
    );
    stub.files['device-pc-a.enc'] = await Envelope.seal(secret, pcSnap.toBytes());

    final out = await runSync(
      db: db,
      repo: repo,
      secret: secret,
      remote: WebDavRemote(stub.base, user, pass),
      nowMs: () => fixedNow,
    );
    expect(out.ok, isTrue, reason: out.error ?? '');
    final row = (await db.query('todos', where: "id = 'same'")).single;
    expect(row['title'], '对端那份');
    expect(row['dirty'], 0);
  });
}
