// ignore_for_file: avoid_print

/// 产出 `src-tauri/src/sync/snapshot.rs` 里那条 `XLANG_DART_SNAPSHOT` 用的 base64。
///
/// 它不是产品代码，只在格式改动、需要重新冻向量时手动跑一次：
/// ```
/// cd mobile && dart run tool/emit_xlang_snapshot.dart
/// ```
library;

import 'dart:convert';

import 'package:anvil_mobile/sync/snapshot.dart';

void main() {
  final snap = Snapshot(
    device: 'phone-xl',
    sealedAt: 1700000000123,
    tables: {
      'todos': [
        {
          'id': 'xl-1',
          'rev': 2,
          'created_at': 1699999999000,
          'created_by': 'phone-xl',
          'updated_at': 1700000000123,
          'updated_by': 'phone-xl',
          'deleted_at': null,
          'title': '跨语言的一条',
          'note': '备注里带中文',
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
        },
      ],
    },
    meta: {'timetable.term': '2026-2027-1', 'timetable.week1Monday': '2026-08-31'},
  );
  print(base64.encode(snap.toBytes()));
}
