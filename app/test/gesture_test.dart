import 'package:canvas_app/src/board.dart';
import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Future<GraphCore> _pumpBoard(WidgetTester tester, {int nodes = 200}) async {
  final core = GraphCore();
  core.demo(nodes);
  final tick = ValueNotifier<int>(0);
  await tester.pumpWidget(
    MaterialApp(
      home: Center(
        child: SizedBox(
          width: 600,
          height: 400,
          child: GraphBoard(core: core, tick: tick, perf: Perf()),
        ),
      ),
    ),
  );
  await tester.pump();
  return core;
}

Offset _board(WidgetTester tester) => tester.getTopLeft(find.byType(GraphBoard));

void main() {
  testWidgets('drag on empty canvas pans the camera', (tester) async {
    final core = await _pumpBoard(tester);
    final camX = core.camX;
    // empty corner of the board, away from nodes
    final o = _board(tester) + const Offset(6, 6);
    await tester.dragFrom(o, const Offset(-120, 60), kind: PointerDeviceKind.mouse);
    await tester.pump();
    expect(core.camX, greaterThan(camX), reason: 'camera must move opposite to the drag');
  });

  testWidgets('mouse wheel zooms at the cursor', (tester) async {
    final core = await _pumpBoard(tester);
    final scale = core.scale;
    tester.binding.handlePointerEvent(
      const PointerScrollEvent(position: Offset(300, 200), scrollDelta: Offset(0, -300)),
    );
    await tester.pump();
    expect(core.scale, greaterThan(scale));
  });

  testWidgets('trackpad pan/zoom gestures are handled', (tester) async {
    final core = await _pumpBoard(tester);
    final scale = core.scale;
    final camX = core.camX;
    const pointer = 7;
    final o = tester.getCenter(find.byType(GraphBoard));
    tester.binding.handlePointerEvent(PointerPanZoomStartEvent(pointer: pointer, position: o));
    tester.binding.handlePointerEvent(PointerPanZoomUpdateEvent(
      pointer: pointer,
      position: o,
      pan: const Offset(-40, 10),
      scale: 1.4,
      rotation: 0,
    ));
    await tester.pump();
    expect(core.scale, greaterThan(scale), reason: 'pinch must zoom');
    expect(core.camX, isNot(camX), reason: 'trackpad pan must move the camera');
    tester.binding.handlePointerEvent(const PointerPanZoomEndEvent(pointer: pointer));
  });

  testWidgets('dragging a node moves that node, not the camera', (tester) async {
    final core = GraphCore();
    core.setViewport(400, 400);
    final id = core.addNode(0, 0, 120, 60, 0xFF22CC88);
    final tick = ValueNotifier<int>(0);
    await tester.pumpWidget(MaterialApp(
      home: Center(
        child: SizedBox(width: 400, height: 400, child: GraphBoard(core: core, tick: tick, perf: Perf())),
      ),
    ));
    await tester.pump();
    final centre = tester.getCenter(find.byType(GraphBoard));
    expect(core.hit(200, 200), id);
    final camX = core.camX;
    final before = core.nodeX(id);
    await tester.dragFrom(centre, const Offset(60, 0), kind: PointerDeviceKind.mouse);
    await tester.pump();
    expect(core.nodeX(id), greaterThan(before), reason: 'node must follow the cursor');
    expect(core.camX, camX, reason: 'camera must stay put');
  });
}
