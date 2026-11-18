import 'package:canvas_app/src/board.dart';
import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// The baked-overview path is what keeps 100k+ node graphs interactive: without
/// it every pan frame costs millions of tiny triangles.
void main() {
  testWidgets('dense graphs switch to a baked overview', (tester) async {
    final core = GraphCore();
    core.demo(40000);
    final tick = ValueNotifier<int>(0);
    final perf = Perf();
    await tester.pumpWidget(MaterialApp(
      home: Center(
        child: SizedBox(
          width: 800,
          height: 600,
          child: GraphBoard(core: core, tick: tick, perf: perf),
        ),
      ),
    ));
    await tester.pump(const Duration(milliseconds: 50));
    expect(perf.visNodes, greaterThan(kOverviewNodes));
    expect(perf.draws, greaterThan(2), reason: 'first frame is drawn live');

    // let the quiet window elapse, the bake run and the async image land
    for (var i = 0; i < 6; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 350)));
      tick.value++; // a frame is produced only when something repaints
      await tester.pump(const Duration(milliseconds: 16));
    }

    expect(perf.draws, lessThanOrEqualTo(2), reason: 'overview is one blit');

    // and it must keep working while the camera moves
    for (var i = 0; i < 5; i++) {
      core.pan(11, 5);
      tick.value++;
      await tester.pump(const Duration(milliseconds: 16));
    }
    expect(perf.draws, lessThanOrEqualTo(2), reason: 'panning stays on the blit path');

    await tester.pumpWidget(const SizedBox.shrink());
    core.dispose();
  });
}
