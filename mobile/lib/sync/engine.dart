/// 一次完整同步：远端与密钥由调用方给，所以测试能对着本地桩跑通整条链，
/// 不必碰真坚果云（与 Rust `relay::run_with` 同一个形状）。
///
/// 失败折成字段，不折成异常：界面只读 `ok=false` + 一句人话，不弹窗（§5.5）。
library;

import 'package:sqflite/sqflite.dart';

import '../repo.dart';
import 'envelope.dart';
import 'merge.dart';
import 'snapshot.dart';
import 'webdav.dart';

/// 手机发出去的表只有 `todos`（`classes` 单向 PC→手机，§3.1）。
///
/// 这也是**手机与桌面的发布条件长得不一样的原因**（issue #1 把它报成了 P1-17）：
/// 桌面那句 `pending > 0 || local_rows` 是为 `classes` 补的 —— 课表没有 dirty 列，
/// 任何"看有没有变化"的优化都会漏发（2026-10-31 真机复现，见 relay.rs 的同款注释）；
/// 而手机从不发课表，本地新增必然 `dirty=1` → `pending>0`，所以 `pendingBefore > 0` 就够。
/// 两端条件不同是数据形状不同，不是有一边漏修。
const phoneTables = ['todos'];

class SyncOutcome {
  /// 这次同步的开始时刻（毫秒）。成功时它就是界面那句「上次同步于」的来源。
  final int at;
  final bool ok;
  final String? error;

  /// 对端那份并进本地的行数（含墓碑）
  final int applied;

  /// 本地版本更新或打平、原样保留的行数
  final int kept;

  /// 这次有没有 PUT 自己那份
  final bool uploaded;

  /// `pending()==0` 的省配额跳过（§3.4）
  final bool putSkipped;

  final int pendingAfter;

  /// 远端别人的设备 id（不含本机）
  final List<String> peers;

  /// `peers` 里这次**读不懂**的那几个：文件在（所以算在线），但那份改动没并进来。
  final List<String> unreadablePeers;

  /// 发出去那份快照自己的时间戳；`clearDirty` 用的就是它，不是墙钟（§3.3 第 2 条）
  final int? sealedAt;

  const SyncOutcome({
    required this.at,
    required this.ok,
    required this.error,
    required this.applied,
    required this.kept,
    required this.uploaded,
    required this.putSkipped,
    required this.pendingAfter,
    required this.peers,
    required this.unreadablePeers,
    required this.sealedAt,
  });
}

/// `secret` 是 `邮箱:应用密码`，与 PC 备份那组凭据同一把（§4）。
Future<SyncOutcome> runSync({
  required Database db,
  required PhoneRepo repo,
  required String secret,
  required WebDavRemote remote,
  required int Function() nowMs,
}) async {
  final start = nowMs();
  try {
    await remote.ensureDir();

    // GET 的是**别人**那些 device-*.enc：自己那份不读回来合并（自己的行本来就在库里）。
    final entries = await remote.list();
    var applied = 0;
    var kept = 0;
    final peers = <String>[];
    final unreadable = <String>[];
    var read = 0;
    for (final entry in entries) {
      if (entry.name == WebDavRemote.ourFileName(repo.deviceId)) continue;
      if (read >= maxPeers) break;
      read++;
      final name = entry.name;
      final id = WebDavRemote.peerIdOf(name);
      // 文件在远端就算在线；读不读得懂是另一件事（与 PC 的 relay 同一口径）
      peers.add(id);
      try {
        final blob = await remote.get(name);
        final plain = await Envelope.open(secret, blob);
        final snap = Snapshot.fromBytes(plain);
        final report = await applySnapshot(db, snap, phoneAccepts);
        // 学期/开学日是解释课表的基准，跟着快照走；本机设置里没有的才写
        await repo.saveMeta(snap.meta, nowMs());
        applied += report.applied;
        kept += report.kept;
      } catch (e) {
        // 一个读不懂的对端不该带走整轮：跳过它、记下它，其余照常合并、自己这份照常上传
        unreadable.add(id);
      }
    }

    // 「本地没有写入就跳过 PUT」的省配额判断（§3.4）。计数只有 pending 一个来源。
    final pendingBefore = await pending(db);
    var uploaded = false;
    int? sealedAt;
    if (pendingBefore > 0) {
      final stamp = nowMs();
      final snap = await dumpSnapshot(
        db,
        phoneTables,
        device: repo.deviceId,
        sealedAt: stamp,
      );
      // 同步那档成本（16 MiB），不是日备那把 64 MiB —— §4 实测补记，手机开这份要用。
      final blob = await Envelope.seal(secret, snap.toBytes());
      await remote.put(WebDavRemote.ourFileName(repo.deviceId), blob);
      // 清的是**那份快照自己的** sealedAt：序列化之后、PUT 响应回来之前新写的行
      // 必须还留在 outbox 里（§3.3 第 2 条），所以这里绝不能用 now。
      await clearDirty(db, stamp);
      uploaded = true;
      sealedAt = stamp;
    }

    final outcome = SyncOutcome(
      at: start,
      ok: true,
      error: null,
      applied: applied,
      kept: kept,
      uploaded: uploaded,
      putSkipped: !uploaded,
      pendingAfter: await pending(db),
      peers: peers,
      unreadablePeers: unreadable,
      sealedAt: sealedAt,
    );
    await record(repo, outcome, nowMs());
    return outcome;
  } catch (e) {
    final outcome = SyncOutcome(
      at: start,
      ok: false,
      error: e.toString().replaceAll('\n', ' '),
      applied: 0,
      kept: 0,
      uploaded: false,
      putSkipped: false,
      // 远端断了不代表待上传数变了（§5.5 那半句仍可验证）
      pendingAfter: await pending(db).catchError((_) => 0),
      peers: const [],
      unreadablePeers: const [],
      sealedAt: null,
    );
    await record(repo, outcome, nowMs());
    return outcome;
  }
}

/// 把这次结果并进 settings 的那份状态。成功刷新 `lastSyncAt`（界面那句时间的来源）；
/// 失败刷新 `lastError/lastErrorAt`，**不抹掉上一次的成功时间**。
Future<void> record(PhoneRepo repo, SyncOutcome out, int now) async {
  final prev = await repo.syncState();
  final next = <String, Object?>{
    'lastSyncAt': out.ok ? out.at : prev['lastSyncAt'],
    'lastError': out.ok ? null : out.error,
    'lastErrorAt': out.ok ? prev['lastErrorAt'] : out.at,
    'peers': out.ok ? out.peers : (prev['peers'] ?? const <String>[]),
    'unreadablePeers': out.ok
        ? out.unreadablePeers
        : (prev['unreadablePeers'] ?? const <String>[]),
  };
  // 记不进 settings 不值得把成功的同步说成失败：那份数据已经传上去了。
  try {
    await repo.saveSyncState(next, now);
  } catch (_) {
    // 与 Rust 侧同一条：只影响下次打开时的那行状态
  }
}
