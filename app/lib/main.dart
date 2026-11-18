import 'dart:async';

import 'package:flutter/material.dart';

import 'src/board.dart';
import 'src/content.dart';
import 'src/icons.dart';
import 'src/native.dart';
import 'src/painter.dart';

/// Docs screenshots are taken with the HUD hidden (`--dart-define`), because the
/// HUD would show numbers measured by the headless test rasteriser.
const bool kCleanShots = bool.fromEnvironment('CANVAS_CLEAN_SHOTS');

void main() => runApp(const CanvasApp());

const Color _bg = Color(0xFF0B0F16);
const Color _panel = Color(0xFF141B26);
const Color _line = Color(0xFF223047);
const Color _accent = Color(0xFF6FE3FF);

class CanvasApp extends StatelessWidget {
  const CanvasApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Graph Canvas',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        brightness: Brightness.dark,
        scaffoldBackgroundColor: _bg,
        colorScheme: ColorScheme.fromSeed(seedColor: _accent, brightness: Brightness.dark),
      ),
      home: const CanvasPage(),
    );
  }
}

class CanvasPage extends StatefulWidget {
  const CanvasPage({super.key});

  @override
  State<CanvasPage> createState() => _CanvasPageState();
}

class _CanvasPageState extends State<CanvasPage> {
  final GraphCore _core = GraphCore();
  final ValueNotifier<int> _tick = ValueNotifier<int>(0);
  final Perf _perf = Perf();
  final DemoContentProvider _content = DemoContentProvider();
  late final IconCache _icons = IconCache(
    sources: demoIconSources,
    onReady: () {
      if (mounted) _tick.value++;
    },
  );

  int _nodes = 10000;
  bool _stress = false;
  bool _layoutRunning = false;
  Timer? _layoutTimer;
  Timer? _hudTimer;

  static const List<int> _sizes = <int>[1000, 10000, 100000, 500000];

  @override
  void initState() {
    super.initState();
    _core.demo(_nodes);
    _hudTimer = Timer.periodic(const Duration(milliseconds: 250), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _layoutTimer?.cancel();
    _hudTimer?.cancel();
    _icons.dispose();
    _tick.dispose();
    _core.dispose();
    super.dispose();
  }

  void _regenerate(int n) {
    _layoutTimer?.cancel();
    _core.layoutReset();
    setState(() {
      _nodes = n;
      _layoutRunning = false;
    });
    _core.demo(n);
    _tick.value++;
  }

  void _fit() {
    _core.pendingFit = true;
    _tick.value++;
  }

  void _zoom(double factor) {
    _core.zoomCentre(factor);
    _tick.value++;
  }

  void _runLayout() {
    if (_layoutRunning) return;
    _core.layoutReset();
    setState(() => _layoutRunning = true);
    _layoutTimer = Timer.periodic(const Duration(milliseconds: 8), (t) {
      if (!mounted) {
        t.cancel();
        return;
      }
      // ~0.5 ms of work at 100k nodes: the UI thread stays interactive.
      final temp = _core.layoutStep(4);
      _tick.value++;
      if (temp <= 0.5) {
        t.cancel();
        _core.pendingFit = true;
        _tick.value++;
        setState(() => _layoutRunning = false);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Column(
          children: <Widget>[
            _toolbar(),
            Expanded(
              child: Stack(
                children: <Widget>[
                  Positioned.fill(
                    child: ColoredBox(
                      color: _bg,
                      child: GraphBoard(
                        core: _core,
                        tick: _tick,
                        perf: _perf,
                        stress: _stress,
                        content: _content.call,
                        icons: _icons,
                      ),
                    ),
                  ),
                  if (!kCleanShots) Positioned(left: 12, bottom: 12, child: _hud()),
                  if (!kCleanShots)
                    const Positioned(
                    right: 12,
                    bottom: 14,
                    child: Text(
                      'drag — pan   ·   wheel / pinch — zoom   ·   drag node — move   ·   click — select',
                      style: TextStyle(color: Color(0xFF5A6B84), fontSize: 11),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _toolbar() {
    return Container(
      decoration: const BoxDecoration(
        color: _panel,
        border: Border(bottom: BorderSide(color: _line)),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      child: Row(
        children: <Widget>[
          const Text('nodes', style: TextStyle(color: Color(0xFF8FA3BF), fontSize: 12)),
          const SizedBox(width: 8),
          for (final n in _sizes) ...<Widget>[
            _chip(_label(n), selected: _nodes == n, onTap: () => _regenerate(n)),
            const SizedBox(width: 6),
          ],
          const SizedBox(width: 12),
          _chip(_layoutRunning ? 'layout…' : 'layout', onTap: _runLayout, accent: true),
          const SizedBox(width: 6),
          _chip('−', onTap: () => _zoom(1 / 1.35)),
          const SizedBox(width: 6),
          _chip('+', onTap: () => _zoom(1.35)),
          const SizedBox(width: 6),
          _chip('fit', onTap: _fit),
          const Spacer(),
          const Text('stress', style: TextStyle(color: Color(0xFF8FA3BF), fontSize: 12)),
          Switch(
            value: _stress,
            onChanged: (v) => setState(() => _stress = v),
          ),
        ],
      ),
    );
  }

  static String _label(int n) => n >= 1000 ? '${n ~/ 1000}k' : '$n';

  Widget _chip(String text, {required VoidCallback onTap, bool selected = false, bool accent = false}) {
    final Color bg = selected ? _accent : (accent ? const Color(0xFF1B2A3D) : const Color(0xFF10161F));
    final Color fg = selected ? const Color(0xFF06121A) : const Color(0xFFB9C9DD);
    return Material(
      color: bg,
      borderRadius: BorderRadius.circular(6),
      child: InkWell(
        borderRadius: BorderRadius.circular(6),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
          child: Text(text, style: TextStyle(color: fg, fontSize: 12, fontWeight: FontWeight.w600)),
        ),
      ),
    );
  }

  Widget _hud() {
    final Perf p = _perf;
    const mono = TextStyle(
      color: Color(0xFF8FA3BF),
      fontSize: 11,
      fontFeatures: <FontFeature>[FontFeature.tabularFigures()],
    );
    return Container(
      decoration: BoxDecoration(
        color: _panel.withValues(alpha: 0.92),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: _line),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            p.idle
                ? 'idle        (stress — измерить fps)'
                : '${p.liveFps.toStringAsFixed(0).padLeft(3)} fps   ${p.frameMs.toStringAsFixed(1).padLeft(5)} ms',
            style: mono.copyWith(color: _accent, fontWeight: FontWeight.w600),
          ),
          Text('core   ${p.buildUs} µs   draws ${p.draws}', style: mono),
          Text('nodes  ${p.visNodes} / ${p.totalNodes}', style: mono),
          Text('edges  ${p.visEdges} / ${p.totalEdges}', style: mono),
          Text('lod    ${_lodName(p.lod)}', style: mono),
        ],
      ),
    );
  }

  static String _lodName(int lod) => switch (lod) {
        0 => 'quads (batched)',
        1 => 'rects',
        _ => 'rects + labels',
      };
}