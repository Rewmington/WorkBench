/// 设置屏。导航里没有它的位置（§5.2：只有两项，设置从今天屏右上角齿轮进）。
///
/// 这一页承担 §8.9 那三类话里的**机制说明**：手机能改什么、冲突谁赢、什么不上云。
/// 主屏上那一句 ≤20 字的操作提示不在这页 —— 那在两屏各自的位置上。
///
/// 同步那组只写可验证的真事实：时间来自最后一次成功，条数来自 `dirty=1`，
/// 远端设备来自目录 listing。没有「已同步」这种无法验证的词（§5.5）。
library;

import 'package:flutter/material.dart';

import '../palette.dart';
import '../reltime.dart';
import '../store.dart';

class SettingsScreen extends StatefulWidget {
  final Store store;
  final VoidCallback onBack;
  const SettingsScreen({required this.store, required this.onBack, super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  bool _busy = false;
  final _email = TextEditingController();
  final _password = TextEditingController();

  Store get store => widget.store;

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = skinOf(context);
    final syncer = store.syncer;
    return Scaffold(
      backgroundColor: s.bg,
      body: SafeArea(
        bottom: false,
        child: ListView(
          padding: const EdgeInsets.fromLTRB(Metrics.padX, 2, Metrics.padX, 20),
          children: [
            Align(
              alignment: Alignment.centerLeft,
              child: InkWell(
                onTap: widget.onBack,
                borderRadius: BorderRadius.circular(Metrics.radiusSm),
                child: Container(
                  constraints: const BoxConstraints(minHeight: Metrics.touch),
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  alignment: Alignment.center,
                  child: Text('‹ 今天',
                      style: TextStyle(fontSize: 12, fontWeight: FontWeight.w700, color: s.t2)),
                ),
              ),
            ),
            const SizedBox(height: 2),
            Text('设置',
                style: TextStyle(
                  fontSize: Metrics.h2,
                  fontWeight: FontWeight.w800,
                  letterSpacing: -.02 * Metrics.h2,
                  color: s.t1,
                )),
            const SizedBox(height: 14),
            _group(context, '同步', [
              // 这一行只有可验证的真事实：时间来自最后一次成功同步，条数来自 dirty=1
              _row(
                context,
                key: '上次同步',
                sub: switch (store.sync) {
                  NeverSynced() => '这台手机还没成功同步过一次',
                  SyncedAt(:final atMs) => '${clockTime(atMs)} · ${store.pending} 条待上传',
                  OfflineAt(:final atMs) => '没连上 · 显示的是 ${clockTime(atMs)} 那份',
                },
                trailing: store.pending > 0 ? _unsentDot(s) : null,
              ),
              _row(
                context,
                key: '坚果云账号',
                sub: '应用密码存在本机 Keystore，不进数据库',
                value: store.accountSaved ? (store.accountEmail ?? '已填') : '未填',
                onTap: syncer == null ? null : _editAccount,
              ),
              // 传输层接上之前不画这两行；现在接了，它们说的每一句都有地方兑现。
              if (syncer != null)
                _row(
                  context,
                  key: '立即同步',
                  sub: !store.accountSaved
                      ? '还没填坚果云账号，先去上面那行填完'
                      : (store.syncError ??
                          (store.syncing ? '正在读远端目录并合并' : '打开 app 时也会自动同步一次')),
                  value: !store.accountSaved
                      ? '去填'
                      : (store.syncing ? '同步中' : (store.syncError != null ? '重试' : '同步')),
                  // 没配齐时这颗不是「按下去什么都不做」，而是把用户送到要做的那一步
                  onTap: _busy || store.syncing
                      ? null
                      : (store.accountSaved ? _syncNow : _editAccount),
                ),
              if (syncer != null)
                _row(
                  context,
                  key: '远端设备',
                  sub: '只认 device-*.enc；自己那份不读回来合并',
                  value: store.sync is NeverSynced
                      ? '还没列过'
                      : (store.peers.isEmpty ? '没发现其他设备' : store.peers.join('、')),
                ),
            ]),
            if (store.medicineService != null)
              _group(context, '药箱（只读）', [
                _row(
                  context,
                  key: '读取手机药箱',
                  sub: '默认用上面那组；药箱是另一个账号时在下面单独填。打卡仍回药箱',
                  value: store.medicineEnabled ? '开' : '关',
                  onTap: _toggleMedicine,
                ),
                _row(
                  context,
                  key: '药箱账号',
                  sub: '留空 = 用上面那组；独立账号的应用密码也进本机 Keystore',
                  value: store.medicineAccountEmail ?? '同上方',
                  onTap: _editMedicineAccount,
                ),
                _row(
                  context,
                  key: '上次读取',
                  sub: _medicineSub(),
                  value: store.medicineFetching
                      ? '读取中'
                      : (store.medicine?.fetchedAt == null
                          ? '—'
                          : clockTime(store.medicine!.fetchedAt!)),
                  onTap: store.medicineEnabled && !store.medicineFetching
                      ? _readMedicine
                      : null,
                ),
              ]),
            _group(context, '关于与隐私', [
              _row(context,
                  key: '手机能改什么',
                  sub: '新建待办、勾选完成。课表只读，改它要回电脑导入'),
              _row(context,
                  key: '冲突了谁赢',
                  sub: '谁那条更新谁赢；一样新就比设备号，两端算出同一个结果'),
              _row(context,
                  key: '什么不上云',
                  sub: '笔记正文、速达、便签、倒计时。上传前用应用密码派生密钥加密，坚果云只看到密文'),
            ]),
            _group(context, '外观', [
              _row(context,
                  key: '字号',
                  sub: '正文 15px · 触控目标不小于 48dp',
                  // 主题跟着系统走，这一版没有 app 内的开关：
                  // 一个不记忆的开关比没有开关坏得多（截图的两套主题由 --dart-define 强制）
                  value: '标准'),
              _row(context, key: '主题', sub: '跟随系统的明暗', value: '跟随系统'),
            ]),
          ],
        ),
      ),
    );
  }

  Future<void> _syncNow() async {
    if (!store.accountSaved) {
      await _editAccount();
      return;
    }
    setState(() => _busy = true);
    try {
      await store.syncNow();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 药箱那两行：开的时候先把账号补齐，关就是真的停（不再请求、卡片也收起来）。
  Future<void> _toggleMedicine() async {
    if (store.medicineEnabled) {
      await store.setMedicineEnabled(false);
      return;
    }
    if (!store.medicineConfigured) {
      await _editMedicineAccount();
      if (!store.medicineConfigured) return;
    }
    await store.setMedicineEnabled(true);
  }

  Future<void> _readMedicine() async {
    if (!store.medicineConfigured) {
      await _editMedicineAccount();
      if (!store.medicineConfigured) return;
    }
    await store.refreshMedicine();
  }

  String _medicineSub() {
    if (!store.medicineEnabled) return '已关；打开后才会再读';
    if (!store.medicineConfigured) return '先填药箱账号（或用上面那组）';
    final err = store.medicineError;
    if (err != null) return err;
    final published = store.medicine?.publishedAt;
    if (published != null) return '药箱发布于 ${clockTime(published)}';
    if (store.medicine?.fetchedAt != null) return '药箱还没同步过';
    return '还没读过；打开 app 时会自动读一次';
  }

  /// 密码框留空 = 沿用已存那条（与桌面同一句约定）。
  Future<void> _editAccount() async {
    final syncer = store.syncer;
    if (syncer == null) return;
    final saved = await _showAccountDialog(
      title: '坚果云账号',
      subtitle: '与电脑备份用的是同一组凭据',
      initialEmail: store.accountEmail ?? '',
    );
    if (saved != true) return;

    if (_email.text.trim().isEmpty) {
      _snack('邮箱不能为空');
      return;
    }
    await syncer.saveAccount(_email.text, _password.text);
    _password.clear();
    await store.reload();
    if (!mounted) return;
    if (!store.accountSaved) {
      _snack('还差应用密码，同步暂时不会跑');
      return;
    }
    await _syncNow();
  }

  /// 药箱独立账号：邮箱留空并保存 = 清掉独立账号，回到工作台那组。
  Future<void> _editMedicineAccount() async {
    final med = store.medicineService;
    if (med == null) return;
    final saved = await _showAccountDialog(
      title: '药箱账号',
      subtitle: '药箱在另一个坚果云账号时填这里；邮箱留空 = 用上面那组',
      initialEmail: store.medicineAccountEmail ?? '',
    );
    if (saved != true) return;
    final email = _email.text.trim();
    await med.saveAccount(email, _password.text);
    _password.clear();
    await store.reload();
    if (!mounted) return;
    _snack(email.isEmpty ? '已改用上面那组账号' : '药箱账号已保存');
    if (store.medicineEnabled) await store.refreshMedicine();
  }

  Future<bool?> _showAccountDialog({
    required String title,
    required String subtitle,
    required String initialEmail,
  }) {
    _email.text = initialEmail;
    _password.clear();
    return showDialog<bool>(
      context: context,
      builder: (ctx) {
        final s = skinOf(ctx);
        return Dialog(
          backgroundColor: s.surf,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Metrics.radius),
            side: BorderSide(color: s.line),
          ),
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 12),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title,
                    style: TextStyle(
                        fontSize: 17, fontWeight: FontWeight.w700, color: s.t1)),
                const SizedBox(height: 4),
                Text(subtitle, style: TextStyle(fontSize: 12, color: s.t3)),
                const SizedBox(height: 14),
                _field(ctx,
                    controller: _email,
                    hint: '注册邮箱',
                    keyboardType: TextInputType.emailAddress),
                const SizedBox(height: 10),
                _field(
                  ctx,
                  controller: _password,
                  hint: '应用密码（留空 = 沿用已存那条）',
                  obscure: true,
                ),
                const SizedBox(height: 8),
                Text('应用密码不是登录密码：坚果云「账户信息 → 安全选项 → 第三方应用管理」里生成。',
                    style: TextStyle(fontSize: 11, height: 1.5, color: s.t3)),
                const SizedBox(height: 16),
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    TextButton(
                      onPressed: () => Navigator.pop(ctx, false),
                      child: Text('取消', style: TextStyle(color: s.t2)),
                    ),
                    const SizedBox(width: 6),
                    FilledButton(
                      onPressed: () => Navigator.pop(ctx, true),
                      style: FilledButton.styleFrom(
                        backgroundColor: s.acc,
                        foregroundColor: s.accInk,
                        minimumSize: const Size(72, Metrics.touch),
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(Metrics.radiusSm)),
                      ),
                      child: const Text('保存'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _field(
    BuildContext context, {
    required TextEditingController controller,
    required String hint,
    bool obscure = false,
    TextInputType? keyboardType,
  }) {
    final s = skinOf(context);
    return TextField(
      controller: controller,
      obscureText: obscure,
      keyboardType: keyboardType,
      style: TextStyle(fontSize: 15, color: s.t1),
      cursorColor: s.acc,
      decoration: InputDecoration(
        hintText: hint,
        hintStyle: TextStyle(fontSize: 14, color: s.t3),
        isDense: true,
        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 14),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(Metrics.radiusSm),
          borderSide: BorderSide(color: s.line),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(Metrics.radiusSm),
          borderSide: BorderSide(color: s.acc),
        ),
      ),
    );
  }

  void _snack(String text) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(text, style: const TextStyle(fontSize: 13)),
      behavior: SnackBarBehavior.floating,
      duration: const Duration(seconds: 3),
    ));
  }

  Widget _group(BuildContext context, String title, List<Widget> rows) {
    final s = skinOf(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: 2, bottom: 7),
          child: Text(title.toUpperCase(),
              style: TextStyle(
                fontSize: Type.kicker,
                letterSpacing: .12,
                fontWeight: FontWeight.w700,
                color: s.t3,
              )),
        ),
        ...rows,
        const SizedBox(height: 14),
      ],
    );
  }

  Widget _row(BuildContext context,
      {required String key,
      String? sub,
      String? value,
      Widget? trailing,
      VoidCallback? onTap}) {
    final s = skinOf(context);
    final body = Container(
      margin: const EdgeInsets.only(bottom: Metrics.rowGap),
      constraints: const BoxConstraints(minHeight: 52),
      padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 9),
      decoration: BoxDecoration(
        color: s.surf,
        border: Border.all(color: s.line),
        borderRadius: BorderRadius.circular(Metrics.radiusSm),
      ),
      child: Row(
        crossAxisAlignment: sub == null ? CrossAxisAlignment.center : CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(key, style: TextStyle(fontSize: Type.settingKey, color: s.t1)),
                if (sub != null) ...[
                  const SizedBox(height: 2),
                  Text(sub,
                      style: TextStyle(fontSize: Type.meta, height: 1.55, color: s.t3)),
                ],
              ],
            ),
          ),
          if (trailing != null) ...[
            const SizedBox(width: 10),
            trailing,
          ],
          if (value != null) ...[
            const SizedBox(width: 10),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 170),
              child: Text(value,
                  textAlign: TextAlign.right,
                  style: TextStyle(fontSize: Type.metaWide, color: s.t3)),
            ),
          ],
        ],
      ),
    );
    if (onTap == null) return body;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(Metrics.radiusSm),
      child: body,
    );
  }

  Widget _unsentDot(Skin s) => Container(
        width: 7,
        height: 7,
        decoration: BoxDecoration(color: s.amber, shape: BoxShape.circle),
      );
}
