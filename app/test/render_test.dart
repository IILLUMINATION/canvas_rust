import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

const double _w = 240;
const double _h = 240;

Future<Uint8List> _render(GraphCore core, Perf perf) async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder, const Rect.fromLTWH(0, 0, _w, _h));
  GraphPainter(core: core, tick: ValueNotifier<int>(0), perf: perf).paint(
    canvas,
    const Size(_w, _h),
  );
  final picture = recorder.endRecording();
  final image = await picture.toImage(_w.toInt(), _h.toInt());
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  picture.dispose();
  image.dispose();
  return data!.buffer.asUint8List();
}

/// Premultiplied RGBA at (x, y).
int _argbAt(Uint8List px, int x, int y) {
  final o = (y * _w.toInt() + x) * 4;
  return (px[o + 3] << 24) | (px[o] << 16) | (px[o + 1] << 8) | px[o + 2];
}

void main() {
  testWidgets('rect LOD paints every node with its own colour', (tester) async {
    final core = GraphCore();
    core.setViewport(_w, _h);
    final colors = <int>[0xFFE0524E, 0xFF33C6A3, 0xFF4F8CFF];
    for (var i = 0; i < 3; i++) {
      core.addNode(i * 400.0, 0, 160, 90, colors[i]);
    }
    core.pendingFit = true;
    late Uint8List px;
    late GFrame frame;
    await tester.runAsync(() async {
      px = await _render(core, Perf());
      frame = core.frame();
    });
    expect(frame.lod, 2, reason: 'three nodes must be the labelled rect LOD');
    expect(frame.rectsCount, 3);
    for (var i = 0; i < 3; i++) {
      final r = frame.rects;
      final cx = (r[i * 4] + r[i * 4 + 2] / 2).round();
      final cy = (r[i * 4 + 1] + r[i * 4 + 3] / 2).round();
      expect(_argbAt(px, cx, cy), colors[i], reason: 'node $i pixel');
      expect(frame.rectColors[i] & 0xFFFFFF, colors[i] & 0xFFFFFF);
    }
    core.dispose();
  });

  testWidgets('quads LOD paints batched vertex colours', (tester) async {
    final core = GraphCore();
    core.setViewport(_w, _h);
    const color = 0xFF22CC88;
    // > RECT_LIMIT visible nodes forces the batched triangle path
    for (var i = 0; i < 4200; i++) {
      core.addNode(i * 1.5, 0, 300, 120, color);
    }
    core.pendingFit = true;
    late Uint8List px;
    late GFrame frame;
    await tester.runAsync(() async {
      px = await _render(core, Perf());
      frame = core.frame();
    });
    expect(frame.lod, 0);
    expect(frame.quadsCount, frame.visNodes);
    final q = frame.quads;
    final cx = ((q[0] + q[2]) / 2).round().clamp(0, _w.toInt() - 1);
    final cy = ((q[1] + q[5]) / 2).round().clamp(0, _h.toInt() - 1);
    expect(_argbAt(px, cx, cy), color, reason: 'vertex colour must reach the raster');
    core.dispose();
  });

  testWidgets('edges are stroked between nodes', (tester) async {
    final core = GraphCore();
    core.setViewport(_w, _h);
    final a = core.addNode(-200, 0, 80, 40, 0xFF808080);
    final b = core.addNode(200, 0, 80, 40, 0xFF808080);
    core.addEdge(a, b);
    core.pendingFit = true;
    late Uint8List px;
    late GFrame frame;
    await tester.runAsync(() async {
      px = await _render(core, Perf());
      frame = core.frame();
    });
    expect(frame.edgesCount, 1);
    // scan the middle band for a non-transparent pixel between the nodes
    var hits = 0;
    for (var y = _h ~/ 2 - 2; y <= _h ~/ 2 + 2; y++) {
      for (var x = _w ~/ 2 - 10; x <= _w ~/ 2 + 10; x++) {
        if ((_argbAt(px, x, y) >> 24) != 0) hits++;
      }
    }
    expect(hits, greaterThan(5), reason: 'edge must be visible between nodes');
    expect(frame.edgeQuads, 0);
    core.dispose();
  });

  testWidgets('labels are painted over nodes at close zoom', (tester) async {
    final core = GraphCore();
    core.setViewport(_w, _h);
    core.addNode(0, 0, 400, 160, 0xFF204060);
    core.pendingFit = true;
    late Uint8List px;
    late GFrame frame;
    await tester.runAsync(() async {
      px = await _render(core, Perf());
      frame = core.frame();
    });
    expect(frame.lod, 2);
    expect(frame.labelsCount, 1, reason: 'single node must emit a label rect');
    expect(frame.labelIds[0], 0);
    // the test font renders solid glyph boxes, so the label centre pixel is
    // label coloured rather than node coloured
    final centre = _argbAt(px, _w ~/ 2, _h ~/ 2);
    expect(centre, isNot(0xFF204060), reason: 'label must be drawn on top');
    expect(centre & 0xFF000000, 0xFF000000, reason: 'label must be opaque');
    core.dispose();
  });

  testWidgets('dense edge sets switch to batched triangles', (tester) async {
    final core = GraphCore();
    core.setViewport(_w, _h);
    final a = core.addNode(-200, 0, 80, 40, 0xFF808080);
    final b = core.addNode(200, 0, 80, 40, 0xFF808080);
    for (var i = 0; i < 1300; i++) {
      core.addEdge(a, b);
    }
    core.pendingFit = true;
    late Uint8List px;
    late GFrame frame;
    await tester.runAsync(() async {
      px = await _render(core, Perf());
      frame = core.frame();
    });
    expect(frame.edgesCount, 1300);
    expect(frame.edgeQuads, 1);
    expect(frame.edges.length, frame.edgesCount * 8);
    var hits = 0;
    for (var y = _h ~/ 2 - 2; y <= _h ~/ 2 + 2; y++) {
      for (var x = _w ~/ 2 - 10; x <= _w ~/ 2 + 10; x++) {
        if ((_argbAt(px, x, y) >> 24) != 0) hits++;
      }
    }
    expect(hits, greaterThan(5), reason: 'batched edges must be rasterised');
    core.dispose();
  });
}