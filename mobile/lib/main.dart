import 'dart:async';

import 'package:flutter/widgets.dart';

import 'app.dart';
import 'bench.dart';
import 'credentials.dart';
import 'db.dart';
import 'demo.dart';
import 'medicine_service.dart';
import 'repo.dart';
import 'shot.dart';
import 'store.dart';
import 'sync/service.dart';

/// 两个取证开关，都只在带对应 `--dart-define` 时生效：
/// - `ANVIL_SHOT=<路径>`：拍一张 360×800 的 PNG 然后退出（桌面窗口截图量不准，见 shot.dart）
/// - `ANVIL_BENCH=true`：先量 Android 上的 Argon2 耗时（结果进 logcat），量完照常进界面
void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  const shot = String.fromEnvironment('ANVIL_SHOT');
  if (shot.isNotEmpty) {
    runApp(ShotApp(
      path: shot,
      demo: DemoCase.from(const String.fromEnvironment('ANVIL_DEMO')),
      page: const String.fromEnvironment('ANVIL_PAGE', defaultValue: 'today'),
    ));
    return;
  }

  if (const String.fromEnvironment('ANVIL_BENCH').isNotEmpty) {
    await runKdfBench();
  }

  // 真机这条路：SQLite 是真相，内存里的 Store 只是它的投影。
  //
  // 整段包起来，是为了**起不来的时候也要说一句话**：以前这里一抛，`runApp` 根本不执行，
  // 屏幕上只剩启动图标（真机拍过：库被设成只读时就是这一屏），而原因只在 logcat 里。
  // 那种"点开没反应"最难查，也容易让人去卸载重装 —— 那会连数据一起删掉。
  try {
    await _boot();
  } catch (e) {
    runApp(BootFailureApp(reason: '$e'));
  }
}

Future<void> _boot() async {
  final db = await openAppDatabase();
  final repo = await PhoneRepo.open(
    db,
    now: DateTime.now().millisecondsSinceEpoch,
  );
  const syncVault = SecurePasswordVault();
  final service = SyncService(
    db: db,
    repo: repo,
    vault: syncVault,
    nowMs: () => DateTime.now().millisecondsSinceEpoch,
  );
  final medicine = MedicineService(
    repo: repo,
    vault: const SecurePasswordVault(key: SecurePasswordVault.medicineKey),
    fallbackVault: syncVault,
    nowMs: () => DateTime.now().millisecondsSinceEpoch,
  );
  final store = await Store.load(
    repo,
    clock: Clock.live(),
    syncer: service,
    medicineService: medicine,
  );
  runApp(AnvilApp(store: store));

  // 打开 app 时同步一次（§3.2）；药箱只读视图也跟着读一次。
  // 失败不弹窗：状态行会写「没连上」并说是哪一份。
  if (store.accountSaved) {
    unawaited(store.syncNow());
    unawaited(store.refreshMedicine());
  }
}

/// 起不来时的那一屏。只用 widgets（不引 Material），因为此时什么都还没初始化。
class BootFailureApp extends StatelessWidget {
  final String reason;
  const BootFailureApp({required this.reason, super.key});

  /// 身份读不回来与"库打不开"是两件事，给的话不能是同一套：
  /// 前者是 `loadOrCreateDeviceId` 主动拒绝（宁可起不来也不换掉本机身份），
  /// 后者是环境把文件堵住了。用通用那套去解释前者，人会去删数据目录。
  bool get _isIdentity => reason.contains('device.id');

  @override
  Widget build(BuildContext context) => Directionality(
        textDirection: TextDirection.ltr,
        child: ColoredBox(
          color: const Color(0xFF101413),
          child: Center(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 28),
              child: DefaultTextStyle(
                style: const TextStyle(
                    color: Color(0xFFE7E4DC), fontSize: 15, height: 1.6),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text('Anvil 打不开本机的数据',
                        style: TextStyle(
                            fontSize: 22, fontWeight: FontWeight.w700)),
                    const SizedBox(height: 14),
                    const Text('先别卸载重装 —— 卸载会连数据一起删掉。'),
                    const SizedBox(height: 6),
                    Text(_isIdentity
                        ? '本机身份那一行读不回来了。软件不肯换一把新的：换了它，'
                            '你手机里每一条记录都会变成"别人写的"，'
                            '冲突时谁赢、几条待上传全部跟着乱。'
                            '要么用 adb / sqlite 把 settings 里那一行修回能读的样子，'
                            '要么在电脑上重新同步一份库过来。'
                        : '常见原因：这个目录被设成只读、被安全软件锁住、磁盘满了，'
                            '或者库文件坏了。'),
                    const SizedBox(height: 14),
                    Text('原始原因（排查时带上这句）：\n$reason'),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
}
