/// 解压上限（P2-6 在手机侧的那半）：越界要在**收字节的过程中**拒绝，而不是解完再量长度 ——
/// 先收满再量，炸弹在量之前就已经把内存吃掉了。
/// 与 Rust `sync::snapshot::a_decompression_bomb_is_refused_instead_of_allocated` 同一条断言，
/// 默认上限两端同值（64 MiB），测试里另给一个小上限才好构造。
library;

import 'dart:convert';
import 'dart:io' show gzip;
import 'dart:typed_data';

import 'package:anvil_mobile/sync/snapshot.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('超过上限的 gzip 一个字节都不收，合法的小文件照旧放行', () {
    // 4 MiB 零字节压完只有几 KB：输入很小、输出越界 —— 正是「先收满再量」会失手的那一类
    final bomb = gzip.encode(Uint8List(4 << 20));
    expect(bomb.length, lessThan(8192), reason: '压不小就说明这条没意义：${bomb.length} 字节');

    expect(
        () => gunzipBounded(bomb, max: 1 << 20),
        throwsA(isA<SnapshotException>()
            .having((e) => e.message, 'message', contains('上限'))));

    // 同一份实现得放行合法的小文件，别把上限写成「全都拒」
    expect(gunzipBounded(gzip.encode(utf8.encode('ok')), max: 1 << 20), utf8.encode('ok'));

    // 根本不是 gzip 的输入报的是「解不开」，不是上限 —— 两句话的下一步动作不同
    expect(
        () => Snapshot.fromBytes(utf8.encode('just text')),
        throwsA(isA<SnapshotException>()
            .having((e) => e.message, 'message', contains('gzip'))));
  });

  /// 上限那句必须活着走到界面：`Snapshot.fromBytes` 外面套着一个 catch-all，稍不留神就把
  /// 「不接」改写成「解不开」，而这两种在用户那边是完全不同的两件事（一个是拒收，一个是重传）。
  /// 所以这条真按生产的默认上限（64 MiB）走一遍 `fromBytes`，不是只调内部函数。
  test('越界从 fromBytes 出来仍是「不接」，没被改写成「解不开」', () {
    final snap = Snapshot(
      device: 'pc-a',
      sealedAt: 1700000000000,
      tables: <String, List<Map<String, Object?>>>{
        'todos': [
          {'id': 'pc-a-1', 'title': '留着', 'rev': 1}
        ]
      },
    );
    expect(Snapshot.fromBytes(snap.toBytes()).rows('todos').single['title'], '留着',
        reason: '先把合法链路钉住，才谈得上失败路径的措辞');

    final beyond = maxSnapshotBytes + (1 << 20);
    final bomb = gzip.encode(Uint8List(beyond));
    expect(
        () => Snapshot.fromBytes(bomb),
        throwsA(isA<SnapshotException>()
            .having((e) => e.message, 'message', contains('上限'))));
  });
}
