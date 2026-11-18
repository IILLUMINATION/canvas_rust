// Renders an animation of the canvas for the README (frames -> GIF/WebP).
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/services.dart';
import 'dart:ui' as ui;

import 'package:canvas_app/src/content.dart';
import 'package:canvas_app/src/icons.dart';
import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

const int _w = 800;
const int _h = 500;
const String _out = '/tmp/anim';

Future<void> _loadFont() async {
  final file = File('/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf');
  if (!file.existsSync()) return;
  final bytes = file.readAsBytesSync();
  await (FontLoader('Roboto')
        ..addFont(Future<ByteData>.value(ByteData.view(Uint8List.fromList(bytes).buffer))))
      .load();
}

/// Docs generator, not a test: no-ops unless CANVAS_CAPTURE is set.
///
///   flutter test --dart-define=CANVAS_CAPTURE=true --dart-define=CANVAS_CLEAN_SHOTS=true test/capture_anim_test.dart
///   ffmpeg -framerate 13 -i /tmp/anim/%03d.png -vf "split[a][b];[a]palettegen=..." -loop 0 docs/demo.gif
void main() {
  testWidgets('capture canvas animation', (tester) async {
    if (!const bool.fromEnvironment('CANVAS_CAPTURE')) return;
    await _loadFont();
    final core = GraphCore();
    core.setViewport(_w.toDouble(), _h.toDouble());
    core.demo(700);
    core.pendingFit = true;

    final icons = IconCache(sources: demoIconSources, onReady: () {});
    final tick = ValueNotifier<int>(0);
    final painter = GraphPainter(
      core: core,
      tick: tick,
      perf: Perf(),
      content: DemoContentProvider().call,
      icons: icons,
    );
    final dir = Directory(_out)..createSync(recursive: true);
    var frame = 0;

    Future<void> shot() async {
      final recorder = ui.PictureRecorder();
      final canvas = Canvas(recorder, Rect.fromLTWH(0, 0, _w.toDouble(), _h.toDouble()));
      painter.paint(canvas, Size(_w.toDouble(), _h.toDouble()));
      final picture = recorder.endRecording();
      final image = await picture.toImage(_w, _h);
      final png = await image.toByteData(format: ui.ImageByteFormat.png);
      picture.dispose();
      image.dispose();
      File('${dir.path}/${frame.toString().padLeft(3, '0')}.png')
          .writeAsBytesSync(png!.buffer.asUint8List());
      frame++;
    }

    await tester.runAsync(() async {
      // let icons rasterise
      await Future<void>.delayed(const Duration(milliseconds: 300));

      for (var i = 0; i < 4; i++) {
        await shot(); // settled view
      }

      // zoom into a cluster
      for (var i = 0; i < 18; i++) {
        core.zoom(_w * 0.5, _h * 0.42, 1.2);
        await shot();
      }

      // find and drag a node
      // centre the node closest to the camera, then drag it
      var id = 0;
      var best = double.infinity;
      for (var i = 0; i < 700; i++) {
        final dx = core.nodeX(i) - core.camX;
        final dy = core.nodeY(i) - core.camY;
        final d = dx * dx + dy * dy;
        if (d < best) {
          best = d;
          id = i;
        }
      }
      core.setCamera(core.nodeX(id), core.nodeY(id), core.scale);
      final dbg = core.frame();
      // ignore: avoid_print
      print('DBG scale=${core.scale} vis=${dbg.visNodes} lod=${dbg.lod} labels=${dbg.labelsCount} '
          'rect0=${dbg.rects.isEmpty ? '-' : '${dbg.rects[2]}x${dbg.rects[3]}'}');
      await shot();
      await shot();
      if (id != GraphCore.noNode) {
        core.select(id);
        painter.startDrag(id);
        final x0 = core.nodeX(id);
        final y0 = core.nodeY(id);
        final sc = core.scale;
        for (var i = 1; i <= 14; i++) {
          final t = i / 14.0;
          core.moveNode(id, x0 + t * 180 / sc, y0 - t * 110 / sc);
          await shot();
        }
        painter.endDrag();
        core.select(GraphCore.noNode);
        for (var i = 0; i < 3; i++) {
          await shot();
        }
      }

      // zoom back out and pan
      core.pendingFit = true;
      for (var i = 0; i < 3; i++) {
        await shot();
      }
      for (var i = 0; i < 6; i++) {
        core.pan(-16, -7);
        await shot();
      }
    });

    icons.dispose();
    painter.close();
    core.dispose();
    // ignore: avoid_print
    print('FRAMES ${dir.path} $frame');
  });
}
