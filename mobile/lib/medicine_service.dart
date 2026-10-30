/// 药箱只读视图的一次刷新。与 `SyncService` 分开：它读的是**药箱自己的目录**，
/// 不代表工作台同步成功或失败，也不写任何药箱数据。
library;

import '../credentials.dart';
import '../medicine.dart';
import '../repo.dart';
import 'sync/medicine_sync.dart';

class MedicineService {
  final PhoneRepo repo;
  final PasswordVault vault;
  final int Function() nowMs;

  /// 生产为 null（坚果云固定地址）；测试指向本地桩。
  final String? base;

  MedicineService({
    required this.repo,
    required this.vault,
    required this.nowMs,
    this.base,
  });

  Future<bool> get configured async =>
      (await repo.email()) != null && (await vault.read()) != null;

  /// 没配凭据返回 null（不是错误）；网络/解析失败抛 [MedicineCloudException]，
  /// 旧缓存保持不动，由界面说「这次没读到」。
  Future<MedicineView?> refresh() async {
    final email = await repo.email();
    final pass = await vault.read();
    if (email == null || pass == null) return null;
    final cloud = MedicineCloud(base ?? medicineCloudBase, email, pass);
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
