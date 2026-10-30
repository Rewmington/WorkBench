/// 药箱只读视图的一次刷新。与 `SyncService` 分开：它读的是**药箱自己的目录**，
/// 不代表工作台同步成功或失败，也不写任何药箱数据。
///
/// 凭据：药箱可能是**另一个**坚果云账号。`medicine.email` 填了就用
/// `Anvil/medicine-webdav` 那把；没填才回退到工作台同步那组。Anvil 不读药箱
/// 自己存在 prefs 里的密码。
library;

import '../credentials.dart';
import '../medicine.dart';
import '../repo.dart';
import 'sync/medicine_sync.dart';

class MedicineService {
  final PhoneRepo repo;

  /// `Anvil/medicine-webdav`（药箱独立账号）
  final PasswordVault vault;

  /// `Anvil/webdav`（工作台同步账号，药箱没单独填时回退用）
  final PasswordVault? fallbackVault;

  final int Function() nowMs;
  final String? base;

  MedicineService({
    required this.repo,
    required this.vault,
    this.fallbackVault,
    required this.nowMs,
    this.base,
  });

  PasswordVault get _fallback => fallbackVault ?? vault;

  Future<({String email, String pass})?> _credentials() async {
    final medEmail = await repo.medicineEmail();
    if (medEmail != null) {
      final pass = await vault.read();
      return pass == null ? null : (email: medEmail, pass: pass);
    }
    final email = await repo.email();
    if (email == null) return null;
    final pass = await _fallback.read();
    return pass == null ? null : (email: email, pass: pass);
  }

  Future<bool> get configured async => await _credentials() != null;

  /// 药箱单独填的邮箱；null = 用同步那组。
  Future<String?> get email => repo.medicineEmail();

  /// 空邮箱 + 空密码 = 清掉独立账号，回到同步那组。
  Future<void> saveAccount(String email, String password) async {
    final e = email.trim();
    if (e.isEmpty) {
      await repo.saveMedicineEmail('', nowMs());
      await vault.clear();
      return;
    }
    await repo.saveMedicineEmail(e, nowMs());
    final p = password.trim();
    if (p.isNotEmpty) await vault.write(password);
  }

  /// 没配凭据返回 null（不是错误）；网络/解析失败抛 [MedicineCloudException]，
  /// 旧缓存保持不动，由界面说「这次没读到」。
  Future<MedicineView?> refresh() async {
    final creds = await _credentials();
    if (creds == null) return null;
    final cloud = MedicineCloud(
      base ?? medicineCloudBase,
      creds.email,
      creds.pass,
    );
    try {
      final got = await cloud.fetch(nowMs: nowMs());
      final view = got.dirMissing
          ? MedicineView(
              medicines: const [],
              logs: const [],
              publishedAt: null,
              devices: 0,
              fetchedAt: got.fetchedAt,
            )
          : MedicineView.merge(got.docs, fetchedAt: got.fetchedAt);
      await repo.saveMedicineView(view, got.fetchedAt);
      await repo.saveMedicineError(null, got.fetchedAt);
      return view;
    } catch (e) {
      await repo.saveMedicineError(e.toString().replaceAll('\n', ' '), nowMs());
      rethrow;
    } finally {
      cloud.close();
    }
  }
}
