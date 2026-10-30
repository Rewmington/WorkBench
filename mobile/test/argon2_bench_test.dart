// PLAN-MOBILE §7 第 1 条的实测台：纯 Dart 的 Argon2id 在这几个成本参数下要跑多久。
//
// 一次同步 = 自己 seal 一次 + 开对方那份一次 = **两次派生**，所以这里的数字要 ×2 才是
// 「打开 app 到能用」的等待。默认测试套件不跑它（太慢，而且它量的是本机 CPU），
// 单独起：`flutter test test/argon2_bench_test.dart -r compact`
@Tags(['bench'])
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:cryptography/cryptography.dart';
import 'package:cryptography/dart.dart';
import 'package:flutter_test/flutter_test.dart';

Future<int> _timeMs(Future<void> Function() body) async {
  final sw = Stopwatch()..start();
  await body();
  sw.stop();
  return sw.elapsedMilliseconds;
}

Future<Duration> _derive(int mKib, int t, int p) async {
  final sw = Stopwatch()..start();
  final argon2 = DartArgon2id(
    memory: mKib,
    iterations: t,
    parallelism: p,
    hashLength: 32,
  );
  await argon2.deriveKey(
    secretKey: SecretKey(utf8.encode('bench@wmddd.online:bench-pass')),
    nonce: List<int>.generate(16, (i) => i),
  );
  sw.stop();
  return sw.elapsed;
}

void main() {
  test('Argon2id 纯 Dart 耗时（PLAN-MOBILE §7 第 1 条）', () async {
    final report = StringBuffer('\n=== 本机（Dart VM / ${const bool.fromEnvironment('dart.vm.product') ? 'release' : 'debug-or-jit'}）===\n');
    for (final case_ in [
      (65536, 3, 1, '日备那把 64 MiB/3/1'),
      (16384, 3, 1, '下限 16 MiB/3/1'),
      (8192, 3, 1, '参考 8 MiB/3/1'),
      (1024, 3, 1, '参考 1 MiB/3/1'),
    ]) {
      final ms = await _timeMs(() => _derive(case_.$1, case_.$2, case_.$3));
      final once = await _derive(case_.$1, case_.$2, case_.$3);
      report.writeln(
        '${case_.$4.padRight(22)} 首次 $ms ms（含分配）· 第二次 ${once.inMilliseconds} ms · 一次同步两次派生 ≈ ${once.inMilliseconds * 2} ms',
      );
    }

    // AES-256-GCM 不是内存硬的，但 30 KB 走纯 Dart 也要有个数
    final key = List<int>.generate(32, (i) => (i * 7 + 11) & 0xff);
    final plain = Uint8List(30 * 1024);
    final ms = await _timeMs(() async {
      await DartAesGcm().encrypt(plain,
          nonce: List<int>.filled(12, 7), secretKey: SecretKey(key));
    });
    report.writeln('AES-256-GCM 加密 30 KB：$ms ms');
    // ignore: avoid_print
    print(report);
    expect(true, isTrue, reason: '这条只报数，不设门禁');
  });
}
