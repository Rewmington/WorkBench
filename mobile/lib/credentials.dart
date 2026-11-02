/// 坚果云应用密码的存放处：Android Keystore（`flutter_secure_storage`）。
///
/// 邮箱不是秘密，进 settings 表；**应用密码不进库、不进日志**（PLAN-MOBILE §4）。
/// 与桌面共用同一把凭据：同一个邮箱 + 同一个应用密码，KDF secret 是 `邮箱:密码`。
///
/// 界面上的约定（与桌面一致）：密码框留空 = 沿用已存那条，不是「清空密码」。
library;

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

abstract interface class PasswordVault {
  Future<String?> read();
  Future<void> write(String password);
  Future<void> clear();
}

/// 真机实现。v11 默认就是 Keystore 支撑的 AES-GCM（API 23+），不需要再开旧版开关。
///
/// 两个条目分开存：工作台同步用 [syncKey]，药箱用 [medicineKey]。
/// 药箱可能是另一个坚果云账号，Anvil 不能拿同步那把去猜。
class SecurePasswordVault implements PasswordVault {
  /// 与桌面 Windows 凭据管理器里的目标名同一个名字，出问题时好对照。
  static const syncKey = 'Anvil/webdav';
  static const medicineKey = 'Anvil/medicine-webdav';

  final String key;
  final FlutterSecureStorage storage;

  const SecurePasswordVault({
    this.key = syncKey,
    this.storage = const FlutterSecureStorage(),
  });

  @override
  Future<String?> read() async {
    try {
      final v = await storage.read(key: key);
      return (v != null && v.trim().isNotEmpty) ? v : null;
    } catch (e) {
      // Keystore 里的密钥不随系统备份走：从备份恢复回来的那台机器上，这条读不出来是**预期的**。
      // 读不出来就当没配 —— 界面会说「还没填」，用户重填一次就好。
      // 原来这里不接异常，而启动链 `main.dart → Store.load → configured` 一路裸奔到 `runApp` 之前，
      // 于是一台恢复过的手机会直接起不来（issue #1 的 P0-4c）。
      debugPrint('读凭据失败，按未配置处理：$e');
      return null;
    }
  }

  @override
  Future<void> write(String password) => storage.write(key: key, value: password);

  @override
  Future<void> clear() => storage.delete(key: key);
}

/// 测试用：不碰平台通道。
class MemoryPasswordVault implements PasswordVault {
  String? _value;
  MemoryPasswordVault([this._value]);

  @override
  Future<String?> read() async => _value;

  @override
  Future<void> write(String password) async => _value = password;

  @override
  Future<void> clear() async => _value = null;
}
