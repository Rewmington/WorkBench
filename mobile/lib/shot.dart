/// 取证用的自截图（只在带了 `--dart-define=ANVIL_SHOT=<路径>` 时存在）。
///
/// 为什么不用 `flutter run --screenshot`：这个 Flutter 版本没有这个选项；而用窗口截图
/// 量到的是"窗口客户区"，标题栏与边框会把 360×800 这条设计框挤掉几十像素 ——
/// 而 §6 M11c 的验收恰恰写着「360×800dp 逐屏截图核对」。
/// 所以这里用 `RepaintBoundary` 把界面**约束成正好 360×800 的逻辑像素**再拍照，
/// 出来的 PNG 尺寸与设备无关，量什么都对得上。
///
/// 用法（每条一次跑，跑完自己退出）：
/// ```
/// flutter run -d windows --dart-define=ANVIL_DEMO=full --dart-define=ANVIL_SHOT=../shots/m11c-today-dark.png
/// ```
library;

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

import 'app.dart';
import 'demo.dart';

/// 设计框，逐条对 §5.2 的基线（360×800dp）
const Size designFrame = Size(360, 800);

class ShotApp extends StatefulWidget {
  final String path;
  final String page;
  final DemoCase demo;

  /// 拍照用几倍像素密度（2 倍出 720×1600，够看细节）
  final double pixelRatio;

  const ShotApp({
    required this.path,
    required this.demo,
    this.page = 'today',
    this.pixelRatio = 2,
    super.key,
  });

  @override
  State<ShotApp> createState() => _ShotAppState();
}

class _ShotAppState extends State<ShotApp> {
  final GlobalKey _boundary = GlobalKey(debugLabel: 'shot');

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _shootWhenReady(0));
  }

  /// 排帧之后再等一小会儿：桌面首帧的字形解析是异步的，太早拍会得到空白文本
  void _shootWhenReady(int attempt) {
    if (!mounted) return;
    if (attempt < 4) {
      Future<void>.delayed(const Duration(milliseconds: 220), () => _shootWhenReady(attempt + 1));
      return;
    }
    _capture();
  }

  Future<void> _capture() async {
    try {
      final ro = _boundary.currentContext?.findRenderObject();
      if (ro is! RenderRepaintBoundary) throw StateError('没有可拍的边界');
      final image = await ro.toImage(pixelRatio: widget.pixelRatio);
      try {
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        if (bytes == null) throw StateError('PNG 编码失败');
        await File(widget.path).writeAsBytes(bytes.buffer.asUint8List());
        stderr.writeln('已存 ${widget.path}（${image.width}×${image.height}px）');
      } finally {
        image.dispose();
      }
    } catch (e) {
      stderr.writeln('截图失败：$e');
      exitCode = 1;
    }
    exit(exitCode);
  }

  @override
  Widget build(BuildContext context) {
    return MediaQuery(
      // 自己造一个 360×800 的媒体查询：桌面窗口没有刘海，SafeArea 不会 padding，
      // 手机上那一条状态栏由真机那一轮（M11d）负责，不在这里假装存在
      data: MediaQueryData(size: designFrame),
      child: Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: designFrame.width,
          height: designFrame.height,
          // 边界放在尺寸框**里面**：放外面的话拍到的是整个客户区，
          // 那张图就不是 §6 要的 360×800 了（第一次跑量出来是 361.5×801.5）
          child: RepaintBoundary(key: _boundary, child: buildApp(page: widget.page)),
        ),
      ),
    );
  }
}
