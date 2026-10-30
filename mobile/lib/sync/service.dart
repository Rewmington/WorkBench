/// 把凭据、远端地址和一次同步串起来。界面只见 `configured` / `run` / `saveAccount`，
/// 所以设置页的测试不需要真网络（远端地址可注入）。
library;

import 'package:sqflite/sqflite.dart';

import '../credentials.dart';
import '../repo.dart';
import 'engine.dart';
import 'webdav.dart';

class SyncService {
  final Database db;
  final PhoneRepo repo;
  final PasswordVault vault;
  final int Function() nowMs;

  /// 生产为 null（坚果云固定地址）；测试指向本地桩。
  final String? base;

  SyncService({
    required this.db,
    required this.repo,
    required this.vault,
    required this.nowMs,
    this.base,
  });

  Future<bool> get configured async =>
      (await repo.email()) != null && (await vault.read()) != null;

  Future<String?> get email => repo.email();

  /// 密码留空 = 沿用已存那条（与桌面同一句约定），不是「清空」。
  Future<void> saveAccount(String email, String password) async {
    await repo.saveEmail(email, nowMs());
    final p = password.trim();
    if (p.isNotEmpty) await vault.write(password);
  }

  /// 没配齐凭据时返回 null —— 那不是失败记录，是「先去填」。
  Future<SyncOutcome?> run() async {
    final mail = await repo.email();
    final pass = await vault.read();
    if (mail == null || pass == null) return null;
    final remote = WebDavRemote(base ?? defaultBase, mail, pass);
    try {
      return await runSync(
        db: db,
        repo: repo,
        secret: '$mail:$pass',
        remote: remote,
        nowMs: nowMs,
      );
    } finally {
      remote.close();
    }
  }
}
