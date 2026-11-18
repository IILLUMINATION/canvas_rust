import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/scheduler.dart';

import 'content.dart';
import 'icons.dart';
import 'native.dart';
import 'painter.dart';

/// Owns all pointer handling. Every gesture is applied by the Rust core
/// (camera math and picking live there), this widget only forwards deltas.
class GraphBoard extends StatefulWidget {
  const GraphBoard({
    super.key,
    required this.core,
    required this.tick,
    required this.perf,
    this.stress = false,
    this.content,
    this.icons,
  });

  final GraphCore core;
  final ValueNotifier<int> tick;
  final Perf perf;
  final bool stress;
  final NodeContentProvider? content;
  final IconCache? icons;

  @override
  State<GraphBoard> createState() => _GraphBoardState();
}

class _GraphBoardState extends State<GraphBoard> with SingleTickerProviderStateMixin {
  final Map<int, Offset> _pointers = <int, Offset>{};
  int _dragNode = GraphCore.noNode;
  double _dragWorldX = 0;
  double _dragWorldY = 0;
  double _dragScale = 1;
  double _pinchDist = 0;
  Offset _pinchMid = Offset.zero;
  // trackpad pan/zoom stream (PointerPanZoom*): deltas since the last update
  Offset _panZoomPan = Offset.zero;
  double _panZoomScale = 1;
  late final Ticker _stressTicker = createTicker(_onStressTick);
  // One painter instance for the lifetime of the board: it owns the baked
  // overview bitmap, which must survive widget rebuilds (the HUD rebuilds the
  // page several times per second).
  late final GraphPainter _painter = GraphPainter(
    core: widget.core,
    tick: widget.tick,
    perf: widget.perf,
    content: widget.content,
    icons: widget.icons,
  );

  @override
  void initState() {
    super.initState();
    if (widget.stress) _stressTicker.start();
  }

  @override
  void didUpdateWidget(covariant GraphBoard old) {
    super.didUpdateWidget(old);
    if (widget.stress != old.stress) {
      if (widget.stress) {
        _stressTicker.start();
      } else {
        _stressTicker.stop();
      }
    }
  }

  @override
  void dispose() {
    _stressTicker.dispose();
    _painter.close();
    super.dispose();
  }

  int _stressPhase = 0;

  /// Stress mode: a bounded loop, so the camera keeps sweeping the same graph
  /// instead of drifting away from it.
  void _onStressTick(Duration _) {
    final t = _stressPhase++ * 0.02;
    widget.core.pan(5.0 * math.cos(t), 3.0 * math.sin(t));
    widget.tick.value++;
  }

  double _dist() {
    final values = _pointers.values;
    if (values.length < 2) return 0;
    return (values.elementAt(0) - values.elementAt(1)).distance;
  }

  Offset _mid() {
    final values = _pointers.values;
    if (values.length < 2) return Offset.zero;
    return (values.elementAt(0) + values.elementAt(1)) / 2;
  }

  void _onDown(PointerDownEvent e) {
    _pointers[e.pointer] = e.localPosition;
    if (_pointers.length == 1) {
      final id = widget.core.hit(e.localPosition.dx, e.localPosition.dy);
      widget.core.select(id);
      if (id != GraphCore.noNode) {
        _dragNode = id;
        _dragWorldX = widget.core.nodeX(id);
        _dragWorldY = widget.core.nodeY(id);
        _dragScale = widget.core.scale;
        // tell the painter: keep blitting the baked overview, draw this node live
        _painter.startDrag(id);
      } else {
        _dragNode = GraphCore.noNode;
      }
    } else if (_pointers.length == 2) {
      _dragNode = GraphCore.noNode;
      _pinchDist = _dist();
      _pinchMid = _mid();
    }
    widget.tick.value++;
  }

  void _onMove(PointerMoveEvent e) {
    final prev = _pointers[e.pointer];
    _pointers[e.pointer] = e.localPosition;
    if (_pointers.length >= 2) {
      final d = _dist();
      final mid = _mid();
      if (_pinchDist > 0 && d > 0) {
        widget.core.zoom(mid.dx, mid.dy, d / _pinchDist);
        widget.core.pan(mid.dx - _pinchMid.dx, mid.dy - _pinchMid.dy);
      }
      _pinchDist = d;
      _pinchMid = mid;
      widget.tick.value++;
      return;
    }
    if (prev == null) return;
    final dx = e.localPosition.dx - prev.dx;
    final dy = e.localPosition.dy - prev.dy;
    if (_dragNode != GraphCore.noNode) {
      final s = _dragScale.abs() < 1e-6 ? 1.0 : _dragScale;
      _dragWorldX += dx / s;
      _dragWorldY += dy / s;
      widget.core.moveNode(_dragNode, _dragWorldX, _dragWorldY);
      _dragScale = widget.core.scale;
    } else {
      widget.core.pan(dx, dy);
    }
    widget.tick.value++;
  }

  void _onUp(PointerEvent e) {
    _pointers.remove(e.pointer);
    if (_pointers.isEmpty && _dragNode != GraphCore.noNode) {
      _dragNode = GraphCore.noNode;
      _painter.endDrag();
    }
    widget.tick.value++;
  }

  /// Trackpad pinch / two-finger gesture. Linux and macOS deliver this as a
  /// dedicated event stream; without it a trackpad cannot zoom at all.
  void _onPanZoomStart(PointerPanZoomStartEvent e) {
    _panZoomPan = Offset.zero;
    _panZoomScale = 1;
  }

  void _onPanZoomUpdate(PointerPanZoomUpdateEvent e) {
    final dScale = _panZoomScale == 0 ? 1.0 : e.scale / _panZoomScale;
    if ((dScale - 1.0).abs() > 1e-4) {
      widget.core.zoom(e.localPosition.dx, e.localPosition.dy, dScale);
    }
    _panZoomScale = e.scale;
    final dPan = e.pan - _panZoomPan;
    if (dPan != Offset.zero) widget.core.pan(dPan.dx, dPan.dy);
    _panZoomPan = e.pan;
    widget.tick.value++;
  }

  void _onPanZoomEnd(PointerPanZoomEndEvent e) {
    _panZoomPan = Offset.zero;
    _panZoomScale = 1;
  }

  /// Wheel and trackpad scroll share one event type, so pick the intent from
  /// the payload: fine-grained deltas are a trackpad swipe (pan), whole notches
  /// or a held Ctrl/Meta are a zoom request.
  void _onSignal(PointerSignalEvent e) {
    if (e is! PointerScrollEvent) return;
    final zoomModified = HardwareKeyboard.instance.isControlPressed ||
        HardwareKeyboard.instance.isMetaPressed;
    final fine = e.scrollDelta.dx.abs() < 45 && e.scrollDelta.dy.abs() < 45;
    if (zoomModified || !fine) {
      final factor = math.exp(-e.scrollDelta.dy / 260.0);
      widget.core.zoom(e.localPosition.dx, e.localPosition.dy, factor);
    } else {
      widget.core.pan(e.scrollDelta.dx, e.scrollDelta.dy);
    }
    widget.tick.value++;
  }

  @override
  Widget build(BuildContext context) {
    return Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: _onDown,
      onPointerMove: _onMove,
      onPointerUp: _onUp,
      onPointerCancel: _onUp,
      onPointerSignal: _onSignal,
      onPointerPanZoomStart: _onPanZoomStart,
      onPointerPanZoomUpdate: _onPanZoomUpdate,
      onPointerPanZoomEnd: _onPanZoomEnd,
      child: RepaintBoundary(
        child: SizedBox.expand(
          child: CustomPaint(
            isComplex: true,
            willChange: true,
            painter: _painter,
          ),
        ),
      ),
    );
  }
}