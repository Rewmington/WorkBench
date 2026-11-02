/// 应用根：皮肤（暗/亮，令牌来自 palette）、两屏导航、齿轮进设置。
///
/// 主题**跟着系统走**，app 内不放这个开关 —— 一个重启就忘的开关比没有更坏。
/// 截图要的两套主题由 `--dart-define=ANVIL_THEME=light|dark` 强制，只有取证那条路用它。
library;

import 'package:flutter/material.dart';

import 'demo.dart';
import 'palette.dart';
import 'store.dart';
import 'ui/settings_screen.dart';
import 'ui/timetable_screen.dart';
import 'ui/today_screen.dart';

enum _Page { today, timetable, settings }

class AnvilApp extends StatefulWidget {
  final Store store;
  final DemoCase demo;

  /// 强制主题只给截图用；平时是 null = 跟随系统
  final Brightness? forcedBrightness;

  /// 取证用：从哪一屏开始拍（`today` / `tt` / `settings`）。平时不传 = 今天屏。
  final String initialPage;

  const AnvilApp({
    required this.store,
    this.demo = DemoCase.none,
    this.forcedBrightness,
    this.initialPage = 'today',
    super.key,
  });

  @override
  State<AnvilApp> createState() => _AnvilAppState();
}

class _AnvilAppState extends State<AnvilApp> {
  late _Page _page = switch (widget.initialPage) {
    'tt' => _Page.timetable,
    'settings' => _Page.settings,
    _ => _Page.today,
  };

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_refresh);
  }

  @override
  void dispose() {
    widget.store.removeListener(_refresh);
    super.dispose();
  }

  void _refresh() => setState(() {});

  ThemeData _theme(Brightness brightness) {
    final s = Skin.of(brightness);
    return ThemeData(
      useMaterial3: true,
      brightness: brightness,
      scaffoldBackgroundColor: s.bg,
      // 点击水波纹压到几乎看不见：原生软件不靠涟漪证明自己可点（桌面 §8.6 同一条）
      splashColor: s.acc.withValues(alpha: .06),
      highlightColor: s.acc.withValues(alpha: .04),
      canvasColor: s.bg,
      textSelectionTheme: TextSelectionThemeData(cursorColor: s.acc, selectionColor: s.accSoft),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Anvil',
      debugShowCheckedModeBanner: false,
      // 桌面那条「17px 灰色网页滚动条」在手机上同样露馅：不要可见滚动条
      scrollBehavior: const _NoScrollbar(),
      themeMode: widget.forcedBrightness == null
          ? ThemeMode.system
          : (widget.forcedBrightness == Brightness.light ? ThemeMode.light : ThemeMode.dark),
      theme: _theme(Brightness.light),
      darkTheme: _theme(Brightness.dark),
      home: Builder(
        builder: (context) {
          final onTimetable = _page == _Page.timetable;
          // 设置页是切换 home 而不是 Navigator 推入的：不给它拦返回键，
          // 安卓返回会落到「只有一条路由」上，直接把 app 退掉。
          final child = switch (_page) {
            _Page.settings => SettingsScreen(
                store: widget.store,
                onBack: () => setState(() => _page = _Page.today),
              ),
            _Page.today || _Page.timetable => onTimetable
                ? TimetableScreen(
                    store: widget.store,
                    onOpenSettings: () => setState(() => _page = _Page.settings),
                    onTab: (v) => setState(() => _page = v ? _Page.timetable : _Page.today),
                  )
                : TodayScreen(
                    store: widget.store,
                    initialUndoId: widget.demo == DemoCase.undo ? undoDemoId : null,
                    onOpenSettings: () => setState(() => _page = _Page.settings),
                    onTab: (v) => setState(() => _page = v ? _Page.timetable : _Page.today),
                  ),
          };
          return PopScope(
            canPop: _page != _Page.settings,
            onPopInvokedWithResult: (didPop, _) {
              if (didPop) return;
              setState(() => _page = _Page.today);
            },
            child: child,
          );
        },
      ),
    );
  }
}

/// 滚动条一律不画。手机上它本来就该隐形，桌面调试窗口里露出网页滚动条最掉价。
class _NoScrollbar extends ScrollBehavior {
  const _NoScrollbar();

  @override
  Widget buildScrollbar(BuildContext context, Widget child, ScrollableDetails details) => child;
}

/// 起哪一份数据，全看 `--dart-define=ANVIL_DEMO=`。不带这个参数就是真数据（现在是空库）。
AnvilApp buildApp({String page = 'today'}) {
  const raw = String.fromEnvironment('ANVIL_DEMO');
  const theme = String.fromEnvironment('ANVIL_THEME');
  final c = DemoCase.from(raw);
  return AnvilApp(
    store: demoStore(c),
    demo: c,
    initialPage: page,
    forcedBrightness: switch (theme) {
      'light' => Brightness.light,
      'dark' => Brightness.dark,
      _ => null,
    },
  );
}
