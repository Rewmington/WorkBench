// 产出「Dart seal → Rust open」那条跨语言向量（PLAN-MOBILE §6 M11a 验收）：
//
//   cd mobile && dart run tool/emit_xlang_dart.dart
//
// 打印的 base64 粘到 src-tauri/tests/envelope.rs 的 XLANG_DART_SEALED。
// 反向那条（Rust seal → Dart open）由 `cargo test --test envelope emit_xlang_rust -- --ignored`
// 产出，粘到 test/envelope_test.dart 里的 rustSealed。
//
// 每次信封布局或同步那档 KDF 成本变了，两边都要重新产一次 —— 向量冻住的是字节，
// 而线上那份 device-*.enc 要能一直解得开，所以轻易别动。
// ignore_for_file: avoid_print —— 这个工具的全部产出就是那一行 base64
import 'dart:convert';
import 'dart:typed_data';

import 'package:anvil_mobile/sync/envelope.dart';

const secret = 'xlang@wmddd.online:xlang-pass';
const plaintext = 'anvil cross-language envelope vector';

void main() async {
  final blob = await Envelope.seal(secret, Uint8List.fromList(utf8.encode(plaintext)));
  print('XLANG_DART=${base64Encode(blob)}');
}
