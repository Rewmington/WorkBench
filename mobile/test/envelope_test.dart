// M11a 验收：Dart 侧这份信封与 PC 的 `backup/crypto.rs` 逐字节互通。
//
// `rustSealed` 是 Rust 用当前实现一次性产出的 blob（重新产出：
// `cargo test --test envelope emit_xlang_rust -- --ignored`）。它冻住的是**字节**，
// 所以哪天动了头部布局或 KDF 参数顺序，这条会立刻红 —— 而真正的代价是
// 坚果云上那份 device-*.enc 再也解不开。
import 'dart:convert';
import 'dart:typed_data';

import 'package:anvil_mobile/sync/envelope.dart';
import 'package:flutter_test/flutter_test.dart';

const secret = 'xlang@wmddd.online:xlang-pass';
const plaintext = 'anvil cross-language envelope vector';

/// `crypto::seal_with(SECRET, PLAINTEXT, 16 MiB/3/1)` 的产物，base64。
const rustSealed =
    'QU5WSUxCS1ABAQAAQAAAAAADAZ7my1c4JrpORblePtRumlTnlaQ++vmVNcziioSvsWSIH78luX8aINcowHnGU1ttgwQ2Aa7oVqNXbf5VF6Gm88p48oLDMmCudGVut+v5uuV7';

Uint8List bytes(String s) => Uint8List.fromList(utf8.encode(s));

void main() {
  test('Rust 封的那份，Dart 打得出明文', () async {
    final blob = base64Decode(rustSealed);
    final open = await Envelope.open(secret, blob);
    expect(utf8.decode(open), plaintext);
  });

  test('Dart 封的那份自己解得开，且头部写的是同步那档成本', () async {
    final blob = await Envelope.seal(secret, bytes(plaintext));
    expect(blob.sublist(0, 8), utf8.encode('ANVILBKP'));
    expect(blob[8], 1, reason: '版本号');
    expect(blob[9], 1, reason: 'kdf=Argon2id');
    expect((blob[10] << 24) | (blob[11] << 16) | (blob[12] << 8) | blob[13], 16384,
        reason: '同步走 16 MiB 那档，不是日备的 64 MiB');
    expect(utf8.decode(await Envelope.open(secret, blob)), plaintext);
  });

  test('同一份明文两次 seal 的字节不同', () async {
    final a = await Envelope.seal(secret, bytes(plaintext));
    final b = await Envelope.seal(secret, bytes(plaintext));
    expect(a, isNot(b), reason: 'salt 与 nonce 每次都换，否则能看出两份内容相同');
    expect(await Envelope.open(secret, a), await Envelope.open(secret, b));
  });

  test('密码敲错是标签拒绝，不是解出乱码', () async {
    final blob = await Envelope.seal(secret, bytes(plaintext));
    expect(
      () => Envelope.open('xlang@wmddd.online:typo', blob),
      throwsA(isA<EnvelopeException>().having(
          (e) => e.message, 'message', contains('解密失败'))),
    );
  });

  test('改 kdf 参数或 salt 都被 AAD 拦住', () async {
    final sealed = await Envelope.seal(secret, bytes(plaintext));
    // m_cost 最低位翻一格：仍在可接受范围内，但 AAD 覆盖它，所以标签不通过
    final tampered = Uint8List.fromList(sealed);
    tampered[13] ^= 0x01;
    expect(() => Envelope.open(secret, tampered), throwsA(isA<EnvelopeException>()));

    final otherSalt = Uint8List.fromList(sealed);
    otherSalt[19] ^= 0xff;
    expect(() => Envelope.open(secret, otherSalt), throwsA(isA<EnvelopeException>()));
  });

  test('头部吹出来的离谱成本在派生之前就被拒绝', () async {
    // 头部是文件带来的：一个被改过的 m_cost 高位不能让手机去申请十几 GB 内存
    final sealed = await Envelope.seal(secret, bytes(plaintext));
    final absurd = Uint8List.fromList(sealed);
    absurd[10] = 0x01;
    expect(
      () => Envelope.open(secret, absurd),
      throwsA(isA<EnvelopeException>()
          .having((e) => e.message, 'message', contains('超出可接受范围'))),
    );
  });

  test('截断与外来文件各说一句人话', () async {
    expect(
      () => Envelope.open(secret, bytes('short')),
      throwsA(isA<EnvelopeException>()
          .having((e) => e.message, 'message', contains('太短'))),
    );
    final foreign = Uint8List.fromList(await Envelope.seal(secret, bytes(plaintext)));
    foreign[0] = 0x4e; // N：魔数不匹配
    expect(
      () => Envelope.open(secret, foreign),
      throwsA(isA<EnvelopeException>()
          .having((e) => e.message, 'message', contains('不是 Anvil'))),
    );
  });

  test('日备那档成本也解得开（同一把算法，只是参数不同）', () async {
    final backupCost = await Envelope.seal(
      secret,
      bytes(plaintext),
      params: const KdfParams.backup(),
    );
    expect(utf8.decode(await Envelope.open(secret, backupCost)), plaintext);
  }, timeout: const Timeout(Duration(minutes: 3)));
}
