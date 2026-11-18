import 'dart:ui' as ui;

import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Regression: dragging a node on a big graph used to invalidate the baked
/// overview on every pointer move, dropping 100k-node scenes from 60 fps to
/// ~6 fps (live rendering of 80k quads per frame).
void main() {
  testWidgets('dragging a node keeps the baked overview', (tester) async {
    final core = GraphCore();
    core.demo(40000);
    final perf = Perf();
    final painter = GraphPainter(core: core, tick: ValueNotifier<int>(0), perf: perf);
    const size = Size(800, 600);

    Future<void> paintOnce() async {
      final recorder = ui.PictureRecorder();
      painter.paint(Canvas(recorder, Offset.zero & size), size);
      recorder.endRecording().dispose();
    }

    // 1. cold frame: everything is drawn live
    await tester.runAsync(paintOnce);
    expect(perf.draws, greaterThan(2), reason: 'cold frame is live geometry');

    // 2. let the overview bake and land
    for (var i = 0; i < 6; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 300)));
      await tester.runAsync(paintOnce);
    }
    expect(painter.bakes, greaterThan(0), reason: 'overview must bake');
    expect(perf.draws, lessThanOrEqualTo(2), reason: 'then it is one blit');

    // 3. drag: many moves, one paint each, must stay on the blit path
    painter.startDrag(0);
    for (var i = 0; i < 25; i++) {
      core.moveNode(0, 150.0 + i * 2, 100.0 - i);
      await tester.runAsync(paintOnce);
      expect(perf.draws, lessThanOrEqualTo(3),
          reason: 'blit + live node overlay, never full geometry (move $i)');
    }
    expect(painter.bakes, 1, reason: 'no re-bake while the drag is in progress');

    // 4. release: re-bake once, then back to a single blit
    painter.endDrag();
    for (var i = 0; i < 6; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 300)));
      await tester.runAsync(paintOnce);
    }
    expect(painter.bakes, greaterThan(1), reason: 'release triggers a fresh bake');
    expect(perf.draws, lessThanOrEqualTo(2));
    painter.close();
    core.dispose();
  });
}
