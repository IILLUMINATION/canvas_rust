import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:canvas_app/src/content.dart';
import 'package:canvas_app/src/icons.dart';
import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

const double _size = 300;

Future<Uint8List> _render(GraphCore core, NodeContentProvider content, IconCache icons) async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder, const Rect.fromLTWH(0, 0, _size, _size));
  GraphPainter(
    core: core,
    tick: ValueNotifier<int>(0),
    perf: Perf(),
    content: content,
    icons: icons,
  ).paint(canvas, const Size(_size, _size));
  final picture = recorder.endRecording();
  final image = await picture.toImage(_size.toInt(), _size.toInt());
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  picture.dispose();
  image.dispose();
  return data!.buffer.asUint8List();
}

int _argb(Uint8List px, int x, int y) {
  final o = (y * _size.toInt() + x) * 4;
  return (px[o + 3] << 24) | (px[o] << 16) | (px[o + 1] << 8) | px[o + 2];
}

void main() {
  testWidgets('SVG icons are rasterised once and then blitted', (tester) async {
    var rasters = 0;
    final icons = IconCache(
      sources: demoIconSources,
      onReady: () => rasters++,
    );
    final core = GraphCore();
    core.setViewport(_size, _size);
    core.addNode(0, 0, 400, 160, 0xFF204060);
    core.pendingFit = true;

    NodeContent? contentFor(int id) => const NodeContent(
          title: 'Alpha',
          body: 'beta gamma',
          badge: '7',
          icon: 'star',
        );

    late Uint8List firstFrame;
    late Uint8List afterIcon;
    await tester.runAsync(() async {
      // frame 1: icon not rasterised yet -> slot reserved, no icon pixels
      firstFrame = await _render(core, contentFor, icons);
      expect(icons.rasterCount, 1, reason: 'one raster per (key, bucket)');

      // let the raster land, then the same frame must contain it
      await Future<void>.delayed(const Duration(milliseconds: 300));
      expect(rasters, greaterThan(0), reason: 'raster must complete');
      afterIcon = await _render(core, contentFor, icons);
      expect(icons.rasterCount, 1, reason: 'second frame is a cache hit, no raster');
    });

    int nonBackground(Uint8List px, int fromY, int toY) {
      var n = 0;
      for (var y = fromY; y < toY; y++) {
        for (var x = 0; x < _size.toInt(); x++) {
          final c = _argb(px, x, y);
          if (c != 0 && c != 0xFF204060) n++;
        }
      }
      return n;
    }

    expect(nonBackground(afterIcon, 60, 240), greaterThan(nonBackground(firstFrame, 60, 240)),
        reason: 'the icon adds pixels inside the card');

    icons.dispose();
    core.dispose();
  });
}
