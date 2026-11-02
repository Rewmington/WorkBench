/// 真机 KDF 计时（PLAN-MOBILE §7 第 1 条）。
///
/// 桌面那组数（64 MiB 单次 632 ms / 16 MiB 129 ms）是**本机 Dart VM** 量的，
/// 而手机上跑的是 AOT 编译的 release 构建 —— 数值可能更好也可能更差，不能靠外推。
/// §4 那个"同步走 16 MiB"的决定就挂在这个数上，所以它必须在这台设备上量一次。
///
/// 触发方式：`--dart-define=ANVIL_BENCH=1`。量完不退出，继续进界面 ——
/// 这样一次安装既能读 logcat 里的数，又能截屏。
library;

import 'dart:convert';
import 'package:flutter/foundation.dart';

import 'sync/envelope.dart';

/// KDF 计时只需要一把**形状真实**的 secret（`邮箱:应用密码`），耗时与值的真假无关。
/// 这里原来硬编码了一个真实账号形状的字面量 —— 而这是个公开仓库，那串字符会一直留在
/// git 历史里。计时用不着一把真凭据，所以换成不可能存在的域名；
/// 真想用自己账号量一次，构建时传 `--dart-define=ANVIL_BENCH_SECRET=邮箱:应用密码`。
const _benchSecretFromDefine = String.fromEnvironment('ANVIL_BENCH_SECRET');
String get _secret =>
    _benchSecretFromDefine.isEmpty ? 'bench@invalid.invalid:bench-pass' : _benchSecretFromDefine;

Future<void> runKdfBench() async {
  // 30 KB 是 §3.5 估的那份 gzip 后全量快照的大小，不用假字节凑数
  final plain = Uint8List.fromList(utf8.encode('anvil 真机 KDF 计时 ' * 2000));

  debugPrint('BENCH 开始：payload=${plain.length}B（纯 Dart Argon2id，AOT release）');
  for (final p in [const KdfParams.sync(), const KdfParams.backup()]) {
    final sw = Stopwatch()..start();
    final sealed = await Envelope.seal(_secret, plain, params: p);
    final sealMs = sw.elapsedMilliseconds;
    sw
      ..reset()
      ..start();
    final back = await Envelope.open(_secret, sealed);
    final openMs = sw.elapsedMilliseconds;
    debugPrint(
      'BENCH m=${p.mKib}KiB t=${p.tCost} p=${p.pCost} · seal ${sealMs}ms · open ${openMs}ms '
      '· 一次同步（自己封 + 开对端那份）≈ ${sealMs + openMs}ms · 往返一致=${listEquals(back, plain)}',
    );
  }
  debugPrint('BENCH 结束');
}
