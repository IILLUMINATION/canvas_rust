import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:canvas_app/src/content.dart';
import 'package:canvas_app/src/native.dart';
import 'package:canvas_app/src/painter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

const double _size = 260;

Future<Uint8List> _render(GraphCore core, NodeContentProvider? content) async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder, const Rect.fromLTWH(0, 0, _size, _size));
  GraphPainter(
    core: core,
    tick: ValueNotifier<int>(0),
    perf: Perf(),
    content: content,
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
  testWidgets('node content is drawn inside the card', (tester) async {
    final core = GraphCore();
    core.setViewport(_size, _size);
    core.addNode(0, 0, 400, 160, 0xFF204060);
    core.pendingFit = true;
    late Uint8List plain;
    late Uint8List rich;
    await tester.runAsync(() async {
      plain = await _render(core, null);
      rich = await _render(core, (id) => const NodeContent(
            title: 'Alpha',
            body: 'beta gamma',
            badge: '7',
          ));
    });

    // the card is the same, but with content more distinct colours appear
    int distinct(Uint8List px) {
      final set = <int>{};
      for (var y = 0; y < _size.toInt(); y += 2) {
        for (var x = 0; x < _size.toInt(); x += 2) {
          set.add(_argb(px, x, y));
        }
      }
      return set.length;
    }

    expect(distinct(rich), greaterThan(distinct(plain)),
        reason: 'title, body and badge must add pixels');

    // the fallback path still labels a node without a content provider
    final centre = _argb(plain, _size ~/ 2, _size ~/ 2);
    expect(centre, isNot(0xFF204060), reason: 'fallback label is drawn');
    core.dispose();
  });
}
