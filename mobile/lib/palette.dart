/// 手机端皮肤：**桌面 §8.5 那套令牌原样搬过来**（PLAN-MOBILE §5.1 选定的「桌面直系」）。
///
/// 这一页不许出现新颜色。每一个值都能在 `design/workbench-mobile-ui.html` 的
/// 「皮肤 1 · 桌面直系」那段找到，暗亮两档一起 —— 换的是**基线**（正文 15px、行高 54dp、
/// 触控 ≥48dp），不是色板。之所以抠得这么死：一旦手机上多出一套色阶，
/// 从此改一个颜色要改两个产品（§5.1 里拒掉「④ 安卓原生味」的唯一理由）。
library;

import 'package:flutter/material.dart';

/// 一组颜色。尺寸不跟着主题变，所以是静态常量。
class Skin {
  final Color bg;
  final Color surf;
  final Color surf2;
  final Color line;
  final Color line2;
  final Color t1;
  final Color t2;
  final Color t3;
  final Color acc;
  final Color accInk;
  final Color accSoft;
  final Color red;
  final Color redSoft;
  final Color amber;
  final Color blue;

  /// 撤销条：暗模式是亮底压深墨，亮模式反过来 —— 与桌面「选中卡翻亮」同一手法
  final Color sel;
  final Color selInk;
  final Color accOnSel;

  const Skin({
    required this.bg,
    required this.surf,
    required this.surf2,
    required this.line,
    required this.line2,
    required this.t1,
    required this.t2,
    required this.t3,
    required this.acc,
    required this.accInk,
    required this.accSoft,
    required this.red,
    required this.redSoft,
    required this.amber,
    required this.blue,
    required this.sel,
    required this.selInk,
    required this.accOnSel,
  });

  static const dark = Skin(
    bg: Color(0xFF0D1110),
    surf: Color(0xFF131816),
    surf2: Color(0xFF171D1A),
    line: Color(0x12FFFFFF), // rgba(255,255,255,.07)
    line2: Color(0x29FFFFFF), // rgba(255,255,255,.16)
    t1: Color(0xFFF1F5F3),
    t2: Color(0xFFA3ADA8),
    t3: Color(0xFF6B7571),
    acc: Color(0xFF2EE27A),
    accInk: Color(0xFF04240F),
    accSoft: Color(0x242EE27A), // rgba(46,226,122,.14)
    red: Color(0xFFF87171),
    redSoft: Color(0x1EF87171), // rgba(248,113,113,.12)
    amber: Color(0xFFFBBF24),
    blue: Color(0xFF60A5FA),
    sel: Color(0xFFF6FAF7),
    selInk: Color(0xFF4B5450),
    accOnSel: Color(0xFF047857),
  );

  static const light = Skin(
    bg: Color(0xFFF7F9F8),
    surf: Color(0xFFFFFFFF),
    surf2: Color(0xFFEEF2F0),
    line: Color(0x17080B0A), // rgba(8,11,10,.09)
    line2: Color(0x33080B0A), // rgba(8,11,10,.2)
    t1: Color(0xFF0B100E),
    t2: Color(0xFF4B5450),
    t3: Color(0xFF7B8480),
    acc: Color(0xFF059669),
    accInk: Color(0xFFFFFFFF),
    accSoft: Color(0x1A059669), // rgba(5,150,105,.10)
    red: Color(0xFFDC2626),
    redSoft: Color(0x14DC2626), // rgba(220,38,38,.08)
    amber: Color(0xFFB45309),
    blue: Color(0xFF2563EB),
    sel: Color(0xFF0B100E),
    selInk: Color(0xFFF1F5F3),
    accOnSel: Color(0xFF2EE27A),
  );

  static Skin of(Brightness? b) => b == Brightness.light ? light : dark;
}

/// 控件里从 context 取皮肤。
Skin skinOf(BuildContext context) => Skin.of(Theme.of(context).brightness);

/// 基线（§5.2）：设计框 360×800dp、正文 15px、触控 ≥48dp、行高 54px、底部导航 54px。
/// 数字不是审美选择，是验收项 —— 改任何一个都要回看 §6 M11c 那三条实测截图条件。
abstract final class Metrics {
  static const body = 15.0; // --p-fs
  static const h2 = 24.0; // --p-h2
  static const rowH = 54.0; // --p-rowh，一条一行实测就是它
  static const padX = 14.0; // --p-padx
  static const radius = 14.0; // --p-r
  static const radiusSm = 10.0; // --p-rs
  static const navH = 54.0; // --p-navh
  static const touch = 48.0; // Android 最小触控，一处都不许破
  static const checkbox = 21.0; // --p-cb
  static const checkRadius = 6.0; // --p-cbr
  static const rowGap = 7.0; // --p-rowgap

  /// 数字一律等宽（§8.6 那条「17px 灰色网页感」的解法之一）
  static const numStyle = TextStyle(fontFeatures: [FontFeature.tabularFigures()]);
}

/// 字号只从这几个档位取，取值全部对得上对比稿里那一条 CSS。
abstract final class Type {
  static const kicker = 10.5; // 分区小标题、KPI 标签
  static const meta = 11.0; // 行末小字、课表第二行
  static const metaWide = 11.5; // 同步那行、导航标签
  static const chip = 9.5; // 优先级角标、「下一节」
  static const input = 14.0; // 输入条正文（正文 -1）
  static const undo = 13.0; // 撤销条
  static const settingKey = 13.5; // 设置行主文字
  static const kpi = 22.0; // KPI 数字
}
