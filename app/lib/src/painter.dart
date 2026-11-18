import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';

import 'content.dart';
import 'icons.dart';
import 'native.dart';

/// Max quads per `drawVertices` batch: 16384 * 4 = 65536 vertices, the largest
/// count addressable by a Uint16 index buffer.
const int kQuadChunk = 16384;

/// Above this many visible nodes the scene is rendered once into an image and
/// then blitted: on software/weak GL, millions of tiny triangles per frame are
/// two orders of magnitude more expensive than one textured quad.
const int kOverviewNodes = 20000;

final Uint16List _quadIndices = _buildQuadIndices(kQuadChunk);

Uint16List _buildQuadIndices(int quads) {
  final out = Uint16List(quads * 6);
  for (var q = 0; q < quads; q++) {
    final b = q * 4;
    final o = q * 6;
    out[o] = b;
    out[o + 1] = b + 1;
    out[o + 2] = b + 2;
    out[o + 3] = b;
    out[o + 4] = b + 2;
    out[o + 5] = b + 3;
  }
  return out;
}

/// Framebuffer counters for the HUD. Written by the painter, read by a timer,
/// so the HUD never schedules work from inside `paint`.
class Perf {
  int buildUs = 0;
  int visNodes = 0;
  int visEdges = 0;
  int totalNodes = 0;
  int totalEdges = 0;
  int lod = 0;
  int draws = 0;
  double frameMs = 0;
  double fps = 0;

  final List<int> _stamps = <int>[];
  int _lastStampUs = 0;
  double _fps = 0;

  /// Frames per second over the last second of activity, 0 when idle: the
  /// counter must not keep a stale value while nothing is being drawn.
  double get liveFps {
    if (_lastStampUs == 0) return 0;
    final idleUs = DateTime.now().microsecondsSinceEpoch - _lastStampUs;
    return idleUs > 600000 ? 0 : _fps;
  }

  bool get idle => liveFps == 0;

  void record(GFrame fr) {
    buildUs = fr.buildUs;
    visNodes = fr.visNodes;
    visEdges = fr.visEdges;
    totalNodes = fr.totalNodes;
    totalEdges = fr.totalEdges;
    lod = fr.lod;
    tickFrame();
  }

  /// Frame accounting for the cached-overview path, where no frame is built.
  void tickFrame() {
    final now = DateTime.now().microsecondsSinceEpoch;
    _stamps.add(now);
    while (_stamps.length > 2 && now - _stamps.first > 1000000) {
      _stamps.removeAt(0);
    }
    _lastStampUs = now;
    if (_stamps.length > 1) {
      frameMs = (now - _stamps.first) / (_stamps.length - 1) / 1000.0;
      fps = frameMs > 0 ? 1000.0 / frameMs : 0;
      _fps = fps;
    }
  }
}

/// Baked overview: one `ui.Image` covering [world] at [bakeScale] px/unit.
class _Overview {
  _Overview(this.image, this.version, this.bakeScale, this.world, this.coversAll);
  final ui.Image image;
  final int version;
  final double bakeScale;

  /// World rect the image covers.
  final Rect world;

  /// True when [world] contains the whole graph, so the blit is complete for
  /// any camera position inside the graph.
  final bool coversAll;
}

class GraphPainter extends CustomPainter {
  GraphPainter({
    required this.core,
    required this.tick,
    required this.perf,
    this.content,
    this.icons,
  }) : super(repaint: tick);

  final GraphCore core;
  final ValueNotifier<int> tick;
  final Perf perf;

  /// Optional per-node content (title, body, badge). When null the painter
  /// falls back to a plain "Node <id>" label.
  final NodeContentProvider? content;

  /// Rasterised SVG icons; lookups are cache hits, misses schedule a raster.
  final IconCache? icons;

  static const Color _edgeColor = Color(0xFF35506B);
  static const Color _labelColor = Color(0xFFE8F0FA);
  static const Color _ringColor = Color(0xFF6FE3FF);
  static const BlendMode quadBlend = BlendMode.src;

  static final Paint _edgePaint = Paint()
    ..color = _edgeColor
    ..strokeWidth = 1.0
    ..style = PaintingStyle.stroke
    ..isAntiAlias = true;

  static final Paint _nodePaint = Paint()..isAntiAlias = true;
  static final Paint _vertexPaint = Paint();
  static final Paint _edgeFillPaint = Paint()..color = _edgeColor;
  static final Paint _blitPaint = Paint()
    ..filterQuality = FilterQuality.none
    ..isAntiAlias = false;
  static final Paint _ringPaint = Paint()
    ..color = _ringColor
    ..style = PaintingStyle.stroke
    ..strokeWidth = 2.0
    ..isAntiAlias = true;

  static const TextStyle _labelStyle = TextStyle(
    color: _labelColor,
    fontSize: 11,
    fontWeight: FontWeight.w600,
    height: 1.0,
  );

  final Map<int, TextPainter> _labelCache = <int, TextPainter>{};
  final Map<(int, int, int), CardLayout> _cardCache = <(int, int, int), CardLayout>{};

  _Overview? _overview;
  int _lastSelected = GraphCore.noNode;
  bool _baking = false;
  bool _closed = false;
  int _seenVersion = 0;
  int _versionChangedMs = 0;
  double _seenScale = 0;
  int _scaleChangedMs = 0;

  void close() {
    _closed = true;
    _overview?.image.dispose();
    _overview = null;
  }

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width < 1 || size.height < 1) return;
    // the baked overview can extend past the viewport; keep it inside the board
    canvas.clipRect(Offset.zero & size);
    core.setViewport(size.width, size.height);
    if (core.pendingFit) core.fit();

    // Fast path: valid baked overview -> one textured quad, no core work at all.
    final version = core.version;
    final sc = core.scale;
    final ov = _overview;
    if (ov != null && ov.version == version && _ratioOk(sc / ov.bakeScale)) {
      if (_blit(canvas, size, ov, sc)) {
        perf.draws = 1;
        if (_lastSelected != GraphCore.noNode) {
          _drawSelection(canvas, size, _lastSelected);
          perf.draws++;
        }
        perf.tickFrame();
        return;
      }
    }

    var fr = core.frame();
    if (fr.visNodes >= kOverviewNodes) {
      _maybeBake(size, fr);
    }
    _lastSelected = fr.selected;
    perf.draws = _paintScene(canvas, fr, size);
    if (fr.selected != GraphCore.noNode) {
      _drawSelection(canvas, size, fr.selected);
      perf.draws++;
    }
    perf.record(fr);
  }

  // ---------------------------------------------------------------- overview

  static bool _ratioOk(double ratio) => ratio >= 0.75 && ratio <= 1.4;

  bool _blit(Canvas canvas, Size size, _Overview ov, double sc) {
    final camX = core.camX;
    final camY = core.camY;
    final left = (ov.world.left - camX) * sc + size.width * 0.5;
    final top = (ov.world.top - camY) * sc + size.height * 0.5;
    final w = ov.world.width * sc;
    final h = ov.world.height * sc;
    if (!ov.coversAll) {
      // partial bake: only usable while the viewport stays inside it
      if (left > 0 || top > 0 || left + w < size.width || top + h < size.height) return false;
    }
    canvas.drawImageRect(
      ov.image,
      Rect.fromLTWH(0, 0, ov.image.width.toDouble(), ov.image.height.toDouble()),
      Rect.fromLTWH(left, top, w, h),
      _blitPaint,
    );
    return true;
  }

  void _maybeBake(Size size, GFrame fr) {
    if (_baking || _closed || fr.visNodes < kOverviewNodes) return;
    final now = DateTime.now().millisecondsSinceEpoch;
    if (fr.version != _seenVersion) {
      _seenVersion = fr.version;
      _versionChangedMs = now;
      return;
    }
    final sc = core.scale;
    if ((sc / (_seenScale == 0 ? sc : _seenScale) - 1).abs() > 0.01) {
      _seenScale = sc;
      _scaleChangedMs = now;
      return;
    }
    // let the gesture settle, then pay for one bake
    if (now - _versionChangedMs < 300 || now - _scaleChangedMs < 300) return;
    _bake(size);
  }

  void _bake(Size size) {
    final camX = core.camX;
    final camY = core.camY;
    final sc = core.scale;
    final graphBounds = Rect.fromLTRB(core.boundsX0, core.boundsY0, core.boundsX1, core.boundsY1);
    var world = graphBounds;
    if (world.isEmpty || world.width <= 0 || world.height <= 0) return;

    const double maxDim = 4096;
    var bakeScale = sc;
    if (world.width * bakeScale > maxDim || world.height * bakeScale > maxDim) {
      // too big for one texture: cover the viewport plus a wide margin
      final vw = size.width / sc;
      final vh = size.height / sc;
      world = Rect.fromCenter(center: Offset(camX, camY), width: vw * 2.5, height: vh * 2.5)
          .intersect(world);
      if (world.isEmpty) return;
      final k = math.min(maxDim / (world.width * bakeScale), maxDim / (world.height * bakeScale));
      if (k < 1) {
        bakeScale *= math.max(k, 0.25);
        world = Rect.fromCenter(
          center: world.center,
          width: math.min(world.width, maxDim / bakeScale),
          height: math.min(world.height, maxDim / bakeScale),
        );
      }
    }
    final imgW = (world.width * bakeScale).clamp(1.0, maxDim);
    final imgH = (world.height * bakeScale).clamp(1.0, maxDim);

    final recorder = ui.PictureRecorder();
    final bakeCanvas = Canvas(recorder, Rect.fromLTWH(0, 0, imgW, imgH));
    core.setViewport(imgW, imgH);
    core.setCamera(world.center.dx, world.center.dy, bakeScale);
    final bakeFrame = core.frame();
    _paintScene(bakeCanvas, bakeFrame, Size(imgW, imgH));
    final picture = recorder.endRecording();
    core.setViewport(size.width, size.height);
    core.setCamera(camX, camY, sc);

    _baking = true;
    final version = bakeFrame.version;
    final scale = bakeScale;
    final covered = world;
    final coversAll = covered.left <= graphBounds.left &&
        covered.top <= graphBounds.top &&
        covered.right >= graphBounds.right &&
        covered.bottom >= graphBounds.bottom;
    picture.toImage(imgW.round(), imgH.round()).then((image) {
      picture.dispose();
      if (_closed) {
        image.dispose();
        return;
      }
      _overview?.image.dispose();
      _overview = _Overview(image, version, scale, covered, coversAll);
      _baking = false;
      tick.value++;
    });
  }

  // ----------------------------------------------------------------- drawing

  int _paintScene(Canvas canvas, GFrame fr, Size size) {
    var draws = 0;

    // ---- edges: one batched call for the whole graph ----
    if (fr.edgesCount > 0) {
      if (fr.edgeQuads == 1) {
        draws += _drawEdgeQuads(canvas, fr);
      } else {
        canvas.drawRawPoints(ui.PointMode.lines, fr.edges, _edgePaint);
        draws++;
      }
    }

    // ---- nodes ----
    if (fr.quadsCount > 0) {
      draws += _drawQuads(canvas, fr);
    } else if (fr.rectsCount > 0) {
      draws += _drawRects(canvas, fr);
    }

    // ---- labels ----
    if (fr.labelsCount > 0) {
      draws += _drawLabels(canvas, fr);
    }
    return draws;
  }

  /// Dense edge sets: two triangles per edge with shared indices, one call per
  /// 16k edges. The engine tessellates stroked lines per segment (~2 us each),
  /// which is far more expensive than these batched triangles.
  int _drawEdgeQuads(Canvas canvas, GFrame fr) {
    final pos = fr.edges;
    final n = fr.edgesCount;
    var draws = 0;
    for (var start = 0; start < n; start += kQuadChunk) {
      final count = math.min(kQuadChunk, n - start);
      final positions = Float32List.sublistView(pos, start * 8, (start + count) * 8);
      final indices =
          count == kQuadChunk ? _quadIndices : Uint16List.sublistView(_quadIndices, 0, count * 6);
      final vertices = ui.Vertices.raw(ui.VertexMode.triangles, positions, indices: indices);
      canvas.drawVertices(vertices, BlendMode.srcOver, _edgeFillPaint);
      draws++;
    }
    return draws;
  }

  int _drawQuads(Canvas canvas, GFrame fr) {
    final pos = fr.quads;
    final colors = fr.quadColors;
    final n = fr.quadsCount;
    var draws = 0;
    for (var start = 0; start < n; start += kQuadChunk) {
      final count = math.min(kQuadChunk, n - start);
      final positions = Float32List.sublistView(pos, start * 8, (start + count) * 8);
      final cols = Int32List.sublistView(colors, start * 4, (start + count) * 4);
      final indices =
          count == kQuadChunk ? _quadIndices : Uint16List.sublistView(_quadIndices, 0, count * 6);
      final vertices = ui.Vertices.raw(
        ui.VertexMode.triangles,
        positions,
        colors: cols,
        indices: indices,
      );
      canvas.drawVertices(vertices, quadBlend, _vertexPaint);
      draws++;
    }
    return draws;
  }

  int _drawRects(Canvas canvas, GFrame fr) {
    final rects = fr.rects;
    final colors = fr.rectColors;
    var draws = 0;
    for (var i = 0, j = 0; i < fr.rectsCount; i++, j += 4) {
      final h = rects[j + 3];
      _nodePaint.color = Color(colors[i]);
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromLTWH(rects[j], rects[j + 1], rects[j + 2], h),
          Radius.circular(math.min(6.0, h * 0.18)),
        ),
        _nodePaint,
      );
      draws++;
    }
    return draws;
  }

  TextPainter _label(int id) {
    final cached = _labelCache[id];
    if (cached != null) return cached;
    if (_labelCache.length > 4096) _labelCache.clear();
    final tp = TextPainter(
      text: TextSpan(text: 'Node $id', style: _labelStyle),
      textDirection: TextDirection.ltr,
      maxLines: 1,
    )..layout();
    _labelCache[id] = tp;
    return tp;
  }

  int _drawLabels(Canvas canvas, GFrame fr) {
    final labels = fr.labels;
    final ids = fr.labelIds;
    final provider = content;
    var draws = 0;
    for (var i = 0, j = 0; i < fr.labelsCount; i++, j += 4) {
      final id = ids[i];
      final rect = Rect.fromLTWH(labels[j], labels[j + 1], labels[j + 2], labels[j + 3]);
      final c = provider?.call(id);
      if (c == null) {
        final tp = _label(id);
        if (tp.width > rect.width - 8 || tp.height > rect.height) continue;
        tp.paint(canvas, rect.topLeft + Offset((rect.width - tp.width) * 0.5, (rect.height - tp.height) * 0.5));
        draws++;
        continue;
      }
      final icon = (icons != null && c.icon.isNotEmpty)
          ? icons!.lookup(c.icon, CardLayout.quantise(rect.height))
          : null;
      _card(id, c, rect).paint(canvas, rect, icon: icon, iconKey: c.icon);
      draws += icon == null ? 1 : 2;
    }
    return draws;
  }

  /// Card content is laid out once per (node, text size, card width) bucket:
  /// panning reuses the layout, only zooming produces a new bucket.
  CardLayout _card(int id, NodeContent c, Rect rect) {
    final sizeQ = CardLayout.quantise(rect.height);
    final wb = (rect.width / 8).round();
    final key = (id, (sizeQ * 4).round(), wb);
    final hit = _cardCache[key];
    if (hit != null) return hit;
    if (_cardCache.length > 2048) _cardCache.clear();
    return _cardCache[key] = CardLayout.build(c, sizeQ, rect.width);
  }

  void _drawSelection(Canvas canvas, Size size, int id) {
    final sc = core.scale;
    final sx = (core.nodeX(id) - core.camX) * sc + size.width * 0.5;
    final sy = (core.nodeY(id) - core.camY) * sc + size.height * 0.5;
    final w = 148 * sc;
    final h = 50 * sc;
    final rect = Rect.fromCenter(center: Offset(sx, sy), width: w + 4, height: h + 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(rect, Radius.circular(math.min(8.0, h * 0.2))),
      _ringPaint,
    );
  }

  @override
  bool shouldRepaint(covariant GraphPainter oldDelegate) => oldDelegate != this;
}