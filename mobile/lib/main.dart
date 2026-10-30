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
  final db = await openAppDatabase();
  final repo = await PhoneRepo.open(
    db,
    now: DateTime.now().millisecondsSinceEpoch,
  );
  final service = SyncService(
    db: db,
    repo: repo,
    vault: const SecurePasswordVault(),
    nowMs: () => DateTime.now().millisecondsSinceEpoch,
  );
  final medicine = MedicineService(
    repo: repo,
    vault: const SecurePasswordVault(),
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
