/// 本机写失败不许被界面演成"已经改好了"（批次 8 的 N17，MuMu 上用 SQLite 触发器造出来的）。
///
/// 真机现象：勾一下 → 界面显示「已完成 2」并弹撤销条，而 `UPDATE` 抛了
/// `SQLITE_CONSTRAINT_TRIGGER`，库里那条还是 0；重开 app 那条就"凭空丢了"，全程没有一句话。
/// 这里用同一种注入（RAISE(ABORT) 触发器）钉住三件事：回滚内存、说出原因、下次写成功自动消掉。
library;

import 'package:anvil_mobile/db.dart';
import 'package:anvil_mobile/repo.dart';
import 'package:anvil_mobile/store.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

const fixedNow = 1_700_000_000_000;

const failUpdate =
    'CREATE TRIGGER inject_fail_upd BEFORE UPDATE ON todos '
    'BEGIN SELECT RAISE(ABORT,\'注入：写不进去\'); END;';
const failInsert =
    'CREATE TRIGGER inject_fail_ins BEFORE INSERT ON todos '
    'BEGIN SELECT RAISE(ABORT,\'注入：写不进去\'); END;';

void main() {
  setUpAll(sqfliteFfiInit);

  test('勾选写不进库：内存回滚、说出原因、库里确实没变', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final store = await Store.load(repo, clock: Clock(() => fixedNow));
    await store.create('喝水');
    final id = store.todos.single.id;
    expect(store.todos.single.completed, false);

    await db.execute(failUpdate);
    await store.setDone(id, true);

    expect(store.todos.single.completed, false,
        reason: '库里没改成，界面就不许显示成已完成');
    expect(store.localWriteError, contains('没写进本机数据'));
    expect(store.localWriteError, contains('注入：写不进去'),
        reason: '原因要原样带出来，否则用户只能猜');
    expect(store.localWriteError, isNot(contains("sql 'UPDATE")),
        reason: 'sqflite 会把整条 SQL 与参数拼在后面：用户读不完，还把行 id 摊在界面上');
    final row = (await db.query('todos', where: 'id = ?', whereArgs: [id])).single;
    expect(row['completed'], 0);

    // 故障过去之后，下一次成功的写要把那句话撤掉（不是一条要人去清的残留）
    await db.execute('DROP TRIGGER inject_fail_upd');
    await store.setDone(id, true);
    expect(store.localWriteError, isNull);
    expect(store.todos.single.completed, true);
    expect((await db.query('todos', where: 'id = ?', whereArgs: [id])).single['completed'], 1);
  });

  test('新建写不进库：那条不许留在列表里冒充已保存', () async {
    final db = await memoryDb();
    final repo = await PhoneRepo.open(db, now: fixedNow);
    final store = await Store.load(repo, clock: Clock(() => fixedNow));

    await db.execute(failInsert);
    await store.create('交实验报告');

    expect(store.todos, isEmpty, reason: '内存里留着它 = 用户以为记上了');
    expect(store.localWriteError, contains('没写进本机数据'));
    expect(await db.query('todos'), isEmpty);
  });
}

Future<Database> memoryDb() => openMemoryDatabase(databaseFactoryFfi);
