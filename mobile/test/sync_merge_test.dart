/// 与 Rust `src-tauri/src/sync/merge.rs`、`snapshot.rs` 同一批钉子。
/// 两边各有一份测试、规则只有一句 —— 哪天有人只改一边，这里或那里会先红。
library;

import 'dart:convert';
import 'dart:io';

import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/sync/merge.dart';
import 'package:anvil_mobile/sync/snapshot.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

late DatabaseFactory ffi;

Future<Database> memoryDb() => openMemoryDatabase(ffi);

Map<String, Object?> row(String id, int at, String by) => {
      'id': id,
      'rev': 1,
      'created_at': at,
      'created_by': by,
      'updated_at': at,
      'updated_by': by,
      'deleted_at': null,
      'title': '对端写的',
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
      'created_day': 20261030,
      'created_hour': 9,
      'completed': 0,
      'reminded_at': null,
    };

Snapshot snapOf(
  Map<String, List<Map<String, Object?>>> tables, {
  Map<String, String> meta = const {},
}) =>
    Snapshot(device: 'pc-a', sealedAt: 500, tables: tables, meta: meta);

/// 一行课表，列与 `_classesDdl` 对齐。
Map<String, Object?> classRow(String id, String term, String name) => {
      'id': id,
      'rev': 1,
      'created_at': 500,
      'created_by': 'pc-a',
      'updated_at': 500,
      'updated_by': 'pc-a',
      'deleted_at': null,
      'term': term,
      'name': name,
      'teacher': '',
      'room': '',
      'day': 1,
      'start_min': 480,
      'end_min': 580,
      'period': '',
      'weeks': '1-16',
    };

Future<void> seed(Database db, String id, int at, String by, String title) =>
    db.insert('todos', {
      'id': id,
      'rev': 1,
      'created_at': at,
      'created_by': by,
      'updated_at': at,
      'updated_by': by,
      'title': title,
      'note': '本机原来的备注',
      'sort_key': 1024.0,
      'zone': 'inbox',
      'completed': 0,
      'dirty': 1,
    });

Future<Map<String, Object?>> read(Database db, String id) async =>
    (await db.query('todos', where: 'id = ?', whereArgs: [id])).single;

void main() {
  setUpAll(() {
    sqfliteFfiInit();
    ffi = databaseFactoryFfi;
  });

  test('对端更新的行整行替换，不逐字段合并', () async {
    final db = await memoryDb();
    await seed(db, 't1', 100, 'pc-a', '本机写的');
    final report = await applySnapshot(
        db, snapOf({'todos': [row('t1', 200, 'phone-a')]}), phoneAccepts);
    expect(report.applied, 1);
    final got = await read(db, 't1');
    expect(got['title'], '对端写的');
    expect(got['note'], '', reason: '没带的值按对端那份走，不与本机旧值混');
    expect(got['dirty'], 0, reason: '对端传来的行已经在云上那份里了');
  });

  test('对端更旧的行使本地保留，且本机那条还没传出去', () async {
    final db = await memoryDb();
    await seed(db, 't1', 300, 'pc-a', '本机刚改的');
    final report = await applySnapshot(
        db, snapOf({'todos': [row('t1', 200, 'phone-a')]}), phoneAccepts);
    expect((report.applied, report.kept), (0, 1));
    final got = await read(db, 't1');
    expect(got['title'], '本机刚改的');
    expect(got['dirty'], 1);
  });

  test('同一毫秒平票按设备 id 裁决，两个方向同一结果', () async {
    final a = await memoryDb();
    await seed(a, 't1', 500, 'bbb', '本机写');
    await applySnapshot(a, snapOf({'todos': [row('t1', 500, 'ddd')]}), phoneAccepts);
    expect((await read(a, 't1'))['title'], '对端写的', reason: '平票时 id 大的一方赢');

    final b = await memoryDb();
    await seed(b, 't1', 500, 'bbb', '本机写');
    final report = await applySnapshot(
        b, snapOf({'todos': [row('t1', 500, 'aaa')]}), phoneAccepts);
    expect((report.applied, report.kept), (0, 1), reason: 'id 小的一方在哪台机器上都赢不了');
    expect((await read(b, 't1'))['title'], '本机写');
  });

  test('墓碑传播，且旧快照再合并不会复活', () async {
    final db = await memoryDb();
    await seed(db, 't1', 100, 'pc-a', '要删的');
    final gone = row('t1', 200, 'phone-a')..['deleted_at'] = 200;

    await applySnapshot(db, snapOf({'todos': [gone]}), phoneAccepts);
    expect((await read(db, 't1'))['deleted_at'], 200, reason: '墓碑传播过来还是墓碑');

    await applySnapshot(db, snapOf({'todos': [row('t1', 150, 'phone-a')]}), phoneAccepts);
    expect((await read(db, 't1'))['deleted_at'], 200, reason: '旧快照不得复活已删的行');

    final report = await applySnapshot(db, snapOf({'todos': [gone]}), phoneAccepts);
    expect((report.applied, report.kept), (0, 1), reason: '同一份墓碑再并一次是无事发生');
  });

  test('手机接课表（PC→手机单向），但接受列表外的表整张拒绝', () async {
    final db = await memoryDb();
    await seed(db, 't1', 100, 'pc-a', '唯一的一行');
    final cls = row('cls-1', 999, 'pc-a')
      ..['term'] = '2026-2027-1'
      ..['name'] = '数据结构'
      ..['teacher'] = ''
      ..['room'] = '3教 201'
      ..['day'] = 5
      ..['start_min'] = 480
      ..['end_min'] = 580
      ..['period'] = '第一大节'
      ..['weeks'] = '1-16';
    final report = await applySnapshot(
      db,
      snapOf({
        'todos': [row('t1', 200, 'phone-a')],
        'classes': [cls],
        'notes': [row('n1', 200, 'phone-a')],
      }),
      phoneAccepts,
    );
    expect(report.refused, ['notes']);
    expect(report.applied, 2, reason: 'todos 与 classes 都要落地');
    expect((await db.query('classes')).length, 1);
    expect(await db.rawQuery(
      "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'notes'",
    ), isEmpty, reason: '拒绝的表连建都不该建');
  });

  test('对端新造的行整行插入，且不算本机欠账', () async {
    final db = await memoryDb();
    final report =
        await applySnapshot(db, snapOf({'todos': [row('new-1', 700, 'pc-a')]}), phoneAccepts);
    expect(report.applied, 1);
    final got = await read(db, 'new-1');
    expect(got['title'], '对端写的');
    expect(got['dirty'], 0);
    expect(got['zone'], 'inbox', reason: '没发的列由 DEFAULT 兜，不是我们编值');
  });

  test('合并父项时子项的外键不炸（不用 INSERT OR REPLACE 的理由）', () async {
    final db = await memoryDb();
    await seed(db, 'p', 100, 'pc-a', '装修');
    await db.insert('todos', {
      'id': 'kid',
      'rev': 1,
      'created_at': 100,
      'created_by': 'pc-a',
      'updated_at': 100,
      'updated_by': 'pc-a',
      'title': '买漆',
      'parent_id': 'p',
      'sort_key': 2048.0,
      'zone': 'inbox',
      'completed': 0,
      'dirty': 0,
    });
    final parent = row('p', 200, 'phone-a')..['title'] = '装修（对端改的）';
    await applySnapshot(db, snapOf({'todos': [parent]}), phoneAccepts);
    expect((await read(db, 'p'))['title'], '装修（对端改的）');
    expect((await db.query('todos')).length, 2, reason: '子项还在，父项整行换掉');
  });

  test('缺版本戳整批回滚，不会落一半', () async {
    final db = await memoryDb();
    final half = row('t1', 100, 'phone-a')..remove('updated_at');
    final good = row('t2', 100, 'phone-a');
    await expectLater(
      applySnapshot(db, snapOf({'todos': [half, good]}), phoneAccepts),
      throwsA(predicate((e) => e.toString().contains('updated_at'))),
    );
    expect(await db.query('todos'), isEmpty, reason: '整批一个事务：失败就一行都不该落地');
  });

  test('数组/对象形状的值被点名拒绝', () async {
    final db = await memoryDb();
    final weird = row('t1', 100, 'phone-a')..['note'] = ['a', 'b'];
    await expectLater(
      applySnapshot(db, snapOf({'todos': [weird]}), phoneAccepts),
      throwsA(predicate((e) => e.toString().contains('note'))),
    );
    expect(await db.query('todos'), isEmpty);
  });

  test('清脏比的是快照自己的时间戳，不是本机 now', () async {
    final db = await memoryDb();
    await seed(db, 't1', 100, 'pc-a', '快照里带着的');
    await seed(db, 't2', 150, 'pc-a', '也带着');
    await clearDirty(db, 200);
    expect(await pending(db), 0);

    // PUT 还在路上，用户又改了 t2、并新建了 t3
    await db.update('todos', {'updated_at': 260, 'dirty': 1},
        where: 'id = ?', whereArgs: ['t2']);
    await seed(db, 't3', 300, 'pc-a', '路上新写的');

    await clearDirty(db, 200);
    expect(await pending(db), 2, reason: '路上改的 t2 与新写的 t3 都还得留在 outbox 里');

    await clearDirty(db, 300);
    expect(await pending(db), 0);
  });

  test('快照往返保留行与 meta，dirty 不上线', () async {
    final db = await memoryDb();
    await seed(db, 't1', 100, 'pc-a', '本机写的');
    final snap = await dumpSnapshot(
      db,
      ['todos'],
      device: 'phone-a',
      sealedAt: 1234,
      meta: {'timetable.term': '2026-2027-1', 'timetable.week1Monday': '2026-08-31'},
    );
    final bytes = snap.toBytes();
    final back = Snapshot.fromBytes(bytes);
    expect(back.device, 'phone-a');
    expect(back.sealedAt, 1234);
    expect(back.meta, snap.meta);
    expect(back.rows('todos').single['title'], '本机写的');
    expect(back.rows('todos').single.containsKey('dirty'), isFalse, reason: '本机状态不外传');
  });

  test('旧快照没有 meta 也能读；版本对不上要点名版本号', () async {
    final old = Snapshot.fromBytes(gzipOf('{"ver":1,"device":"old","sealed_at":1,"tables":{}}'));
    expect(old.meta, isEmpty);

    await expectLater(
      () => Snapshot.fromBytes(gzipOf('{"ver":9,"tables":{}}')),
      throwsA(predicate((e) => e.toString().contains('版本 9'))),
    );
    await expectLater(
      () => Snapshot.fromBytes([1, 2, 3]),
      throwsA(predicate((e) => e.toString().contains('gzip'))),
    );
  });

  test('快照里不认识的多余列不挡这一行落地', () async {
    final db = await memoryDb();
    final future = row('t1', 800, 'pc-a')..['recurrence'] = '每天';
    await applySnapshot(db, snapOf({'todos': [future]}), phoneAccepts);
    final got = await read(db, 't1');
    expect(got['title'], '对端写的');
    expect(got.containsKey('recurrence'), isFalse);
  });

  /// 桌面导入课表是「物理删 + 每轮新 id」，所以快照里没有的那行就是被删掉的课。
  /// 不跟着删，手机上就会攒一批永远删不掉的幽灵课（课表在手机只读，没处手动清）。
  test('课表跟着快照整学期替换，被删的那门不该留在手机上', () async {
    final db = await memoryDb();
    await db.insert('classes', classRow('old-1', '2026-2027-1', '这学期被删掉的'));
    await applySnapshot(
      db,
      snapOf({
        'classes': [classRow('new-1', '2026-2027-1', '这学期新导入的')],
      }),
      phoneAccepts,
    );
    final ids = await db.query('classes', columns: ['id']);
    expect(ids.map((r) => r['id']), ['new-1']);
  });

  test('只动这份快照里出现过的那一学期；没带课表时一行都不动', () async {
    final db = await memoryDb();
    await db.insert('classes', classRow('a-1', '2026-2027-1', '这学期'));
    await db.insert('classes', classRow('b-1', '2025-2026-2', '上一学期'));

    await applySnapshot(
      db,
      snapOf({
        'classes': [classRow('a-2', '2026-2027-1', '这学期新课')],
      }),
      phoneAccepts,
    );
    final after = (await db.query('classes', columns: ['id']))
        .map((r) => r['id'] as String)
        .toList()
      ..sort();
    expect(after, ['a-2', 'b-1'], reason: '上一学期不在这份快照里，不该被牵连');

    // 「这轮没发课表」和「PC 上真的没课」在字节上长得一样，猜错了就是清空用户的课表
    await applySnapshot(db, snapOf({'todos': []}), phoneAccepts);
    await applySnapshot(db, snapOf({'classes': []}), phoneAccepts);
    expect((await db.query('classes')).length, 2);
  });
}

/// 测试里现压一份 gzip，免得把 frozen base64 混进这组规则测试。
List<int> gzipOf(String s) => gzip.encode(utf8.encode(s));
