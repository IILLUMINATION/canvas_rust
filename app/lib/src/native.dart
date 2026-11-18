// Hand-written `dart:ffi` bindings for the Rust core (no codegen, no allocator
// package: the native side hands out buffers and scalars only).

import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

/// Layout must match `CvFrame` in `core/src/ffi.rs` field for field.
final class CvFrame extends Struct {
  external Pointer<Float> f32Ptr;
  external Pointer<Uint32> u32Ptr;
  @Uint32()
  external int f32Len;
  @Uint32()
  external int u32Len;
  @Uint32()
  external int edgesOff;
  @Uint32()
  external int edgesCount;
  @Uint32()
  external int edgesQuads;
  @Uint32()
  external int rectsOff;
  @Uint32()
  external int rectsCount;
  @Uint32()
  external int quadsOff;
  @Uint32()
  external int quadsCount;
  @Uint32()
  external int labelsOff;
  @Uint32()
  external int labelsCount;
  @Uint32()
  external int rectColorsOff;
  @Uint32()
  external int quadColorsOff;
  @Uint32()
  external int labelIdsOff;
  @Uint32()
  external int visNodes;
  @Uint32()
  external int visEdges;
  @Uint32()
  external int totalNodes;
  @Uint32()
  external int totalEdges;
  @Uint32()
  external int selected;
  @Uint32()
  external int version;
  @Uint32()
  external int lod;
  @Uint32()
  external int buildUs;
}

/// One frame of screen-space geometry. The typed lists are views into native
/// memory owned by the canvas: valid until the next [GraphCore.frame] call.
class GFrame {
  GFrame._(this.f, this.u, CvFrame i)
      : edgesOff = i.edgesOff,
        edgesCount = i.edgesCount,
        edgeQuads = i.edgesQuads,
        rectsOff = i.rectsOff,
        rectsCount = i.rectsCount,
        quadsOff = i.quadsOff,
        quadsCount = i.quadsCount,
        labelsOff = i.labelsOff,
        labelsCount = i.labelsCount,
        rectColorsOff = i.rectColorsOff,
        quadColorsOff = i.quadColorsOff,
        labelIdsOff = i.labelIdsOff,
        visNodes = i.visNodes,
        visEdges = i.visEdges,
        totalNodes = i.totalNodes,
        totalEdges = i.totalEdges,
        selected = i.selected,
        version = i.version,
        lod = i.lod,
        buildUs = i.buildUs;

  final Float32List f;
  final Int32List u;

  final int edgesOff;
  final int edgesCount;
  final int edgeQuads;
  final int rectsOff;
  final int rectsCount;
  final int quadsOff;
  final int quadsCount;
  final int labelsOff;
  final int labelsCount;
  final int rectColorsOff;
  final int quadColorsOff;
  final int labelIdsOff;
  final int visNodes;
  final int visEdges;
  final int totalNodes;
  final int totalEdges;
  final int selected;
  final int version;
  final int lod;
  final int buildUs;

  /// Either 4 floats per edge (x1, y1, x2, y2) when [edgeQuads] is 0, or 8
  /// floats per edge (4 quad corners) when it is 1.
  Float32List get edges => Float32List.sublistView(
        f,
        edgesOff,
        edgesOff + edgesCount * (edgeQuads == 1 ? 8 : 4),
      );

  /// 4 floats per node: x, y, w, h (screen).
  Float32List get rects => Float32List.sublistView(f, rectsOff, rectsOff + rectsCount * 4);

  /// 8 floats per node: 4 corners (x, y) x4, index buffer lives on the Dart side.
  Float32List get quads => Float32List.sublistView(f, quadsOff, quadsOff + quadsCount * 8);

  /// 4 floats per label: x, y, w, h (screen).
  Float32List get labels => Float32List.sublistView(f, labelsOff, labelsOff + labelsCount * 4);

  Int32List get rectColors =>
      Int32List.sublistView(u, rectColorsOff, rectColorsOff + rectsCount);

  /// 4 colours per node (one per vertex).
  Int32List get quadColors =>
      Int32List.sublistView(u, quadColorsOff, quadColorsOff + quadsCount * 4);

  Int32List get labelIds => Int32List.sublistView(u, labelIdsOff, labelIdsOff + labelsCount);
}

DynamicLibrary _openCore() {
  final candidates = <String>[
    ?Platform.environment['CANVAS_CORE_LIB'],
    'libcanvas_core.so',
    '../core/target/release/libcanvas_core.so',
    '../core/target/debug/libcanvas_core.so',
    'core/target/release/libcanvas_core.so',
  ];
  Object? last;
  for (final path in candidates) {
    try {
      return DynamicLibrary.open(path);
    } catch (e) {
      last = e;
    }
  }
  throw StateError(
    'libcanvas_core.so not found. Build it first: '
    'cargo build --release --manifest-path core/Cargo.toml. Last error: $last',
  );
}

final DynamicLibrary _lib = _openCore();

final Pointer<Void> Function() _cvNew =
    _lib.lookupFunction<Pointer<Void> Function(), Pointer<Void> Function()>('cv_new');
final void Function(Pointer<Void>) _cvFree =
    _lib.lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>('cv_free');
final void Function(Pointer<Void>, int, int) _cvDemo =
    _lib.lookupFunction<Void Function(Pointer<Void>, Uint32, Uint32), void Function(Pointer<Void>, int, int)>(
        'cv_demo');
final int Function(Pointer<Void>, double, double, double, double, int) _cvAddNode = _lib.lookupFunction<
    Uint32 Function(Pointer<Void>, Float, Float, Float, Float, Uint32),
    int Function(Pointer<Void>, double, double, double, double, int)>('cv_add_node');
final int Function(Pointer<Void>, int, int) _cvAddEdge =
    _lib.lookupFunction<Uint32 Function(Pointer<Void>, Uint32, Uint32), int Function(Pointer<Void>, int, int)>(
        'cv_add_edge');
final void Function(Pointer<Void>, int, double, double) _cvMoveNode = _lib.lookupFunction<
    Void Function(Pointer<Void>, Uint32, Float, Float),
    void Function(Pointer<Void>, int, double, double)>('cv_move_node');
final void Function(Pointer<Void>, int) _cvSelect =
    _lib.lookupFunction<Void Function(Pointer<Void>, Uint32), void Function(Pointer<Void>, int)>('cv_select');
final void Function(Pointer<Void>, double, double) _cvViewport = _lib.lookupFunction<
    Void Function(Pointer<Void>, Float, Float), void Function(Pointer<Void>, double, double)>('cv_viewport');
final void Function(Pointer<Void>, double, double) _cvPan = _lib.lookupFunction<
    Void Function(Pointer<Void>, Float, Float), void Function(Pointer<Void>, double, double)>('cv_pan');
final void Function(Pointer<Void>, double, double, double) _cvZoom = _lib.lookupFunction<
    Void Function(Pointer<Void>, Float, Float, Float),
    void Function(Pointer<Void>, double, double, double)>('cv_zoom');
final void Function(Pointer<Void>, double, double, double) _cvSetCamera = _lib.lookupFunction<
    Void Function(Pointer<Void>, Float, Float, Float),
    void Function(Pointer<Void>, double, double, double)>('cv_set_camera');
final int Function(Pointer<Void>) _cvVersion =
    _lib.lookupFunction<Uint32 Function(Pointer<Void>), int Function(Pointer<Void>)>('cv_version');
final double Function(Pointer<Void>) _cvBoundsX0 =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_bounds_x0');
final double Function(Pointer<Void>) _cvBoundsY0 =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_bounds_y0');
final double Function(Pointer<Void>) _cvBoundsX1 =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_bounds_x1');
final double Function(Pointer<Void>) _cvBoundsY1 =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_bounds_y1');
final void Function(Pointer<Void>, double) _cvZoomCentre =
    _lib.lookupFunction<Void Function(Pointer<Void>, Float), void Function(Pointer<Void>, double)>(
        'cv_zoom_centre');
final void Function(Pointer<Void>) _cvFit =
    _lib.lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>('cv_fit');
final double Function(Pointer<Void>) _cvViewScale =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_view_scale');
final double Function(Pointer<Void>) _cvViewCx =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_view_cx');
final double Function(Pointer<Void>) _cvViewCy =
    _lib.lookupFunction<Float Function(Pointer<Void>), double Function(Pointer<Void>)>('cv_view_cy');
final int Function(Pointer<Void>) _cvStructureVersion = _lib
    .lookupFunction<Uint32 Function(Pointer<Void>), int Function(Pointer<Void>)>('cv_structure_version');
final double Function(Pointer<Void>, int) _cvNodeW = _lib
    .lookupFunction<Float Function(Pointer<Void>, Uint32), double Function(Pointer<Void>, int)>('cv_node_w');
final double Function(Pointer<Void>, int) _cvNodeH = _lib
    .lookupFunction<Float Function(Pointer<Void>, Uint32), double Function(Pointer<Void>, int)>('cv_node_h');
final int Function(Pointer<Void>, int) _cvNodeColor = _lib
    .lookupFunction<Uint32 Function(Pointer<Void>, Uint32), int Function(Pointer<Void>, int)>('cv_node_color');
final double Function(Pointer<Void>, int) _cvNodeX = _lib
    .lookupFunction<Float Function(Pointer<Void>, Uint32), double Function(Pointer<Void>, int)>('cv_node_x');
final double Function(Pointer<Void>, int) _cvNodeY = _lib
    .lookupFunction<Float Function(Pointer<Void>, Uint32), double Function(Pointer<Void>, int)>('cv_node_y');
final CvFrame Function(Pointer<Void>) _cvFrame =
    _lib.lookupFunction<CvFrame Function(Pointer<Void>), CvFrame Function(Pointer<Void>)>('cv_frame');
final int Function(Pointer<Void>, double, double) _cvHit = _lib.lookupFunction<
    Uint32 Function(Pointer<Void>, Float, Float),
    int Function(Pointer<Void>, double, double)>('cv_hit');
final double Function(Pointer<Void>, int) _cvLayoutStep =
    _lib.lookupFunction<Float Function(Pointer<Void>, Uint32), double Function(Pointer<Void>, int)>(
        'cv_layout_step');
final void Function(Pointer<Void>) _cvLayoutReset =
    _lib.lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>('cv_layout_reset');

/// Owning handle to the native canvas.
class GraphCore {
  GraphCore._(this._h);

  factory GraphCore() => GraphCore._(_cvNew());

  static const int noNode = 0xFFFFFFFF;

  final Pointer<Void> _h;
  bool _disposed = false;

  /// Set when the next paint should re-frame the graph (new data, layout end).
  bool pendingFit = true;

  void _check() {
    if (_disposed) throw StateError('GraphCore used after dispose');
  }

  void demo(int nodes, {int seed = 1}) {
    _check();
    _cvDemo(_h, nodes, seed);
    pendingFit = true;
  }

  int addNode(double x, double y, double w, double h, int argb) {
    _check();
    return _cvAddNode(_h, x, y, w, h, argb);
  }

  int addEdge(int a, int b) {
    _check();
    return _cvAddEdge(_h, a, b);
  }

  void moveNode(int id, double x, double y) {
    _check();
    _cvMoveNode(_h, id, x, y);
  }

  void select(int id) {
    _check();
    _cvSelect(_h, id);
  }

  void setViewport(double w, double h) {
    _check();
    _cvViewport(_h, w, h);
  }

  void pan(double dx, double dy) {
    _check();
    _cvPan(_h, dx, dy);
  }

  void zoom(double sx, double sy, double factor) {
    _check();
    _cvZoom(_h, sx, sy, factor);
  }

  /// Place the camera directly (used to bake an overview bitmap).
  void setCamera(double cx, double cy, double scale) {
    _check();
    _cvSetCamera(_h, cx, cy, scale);
  }

  /// Scene content version: bumped by every mutation.
  int get version => _cvVersion(_h);

  double get boundsX0 => _cvBoundsX0(_h);
  double get boundsY0 => _cvBoundsY0(_h);
  double get boundsX1 => _cvBoundsX1(_h);
  double get boundsY1 => _cvBoundsY1(_h);

  /// Zoom around the viewport centre, for buttons/keyboard.
  void zoomCentre(double factor) {
    _check();
    _cvZoomCentre(_h, factor);
  }

  void fit() {
    _check();
    _cvFit(_h);
    pendingFit = false;
  }

  double get scale => _cvViewScale(_h);
  double get camX => _cvViewCx(_h);
  double get camY => _cvViewCy(_h);

  /// Topology/size version: node moves do not change it, so a cached overview
  /// stays usable while a node is being dragged.
  int get structureVersion => _cvStructureVersion(_h);

  double nodeX(int id) => _cvNodeX(_h, id);
  double nodeY(int id) => _cvNodeY(_h, id);
  double nodeW(int id) => _cvNodeW(_h, id);
  double nodeH(int id) => _cvNodeH(_h, id);
  int nodeColor(int id) => _cvNodeColor(_h, id);

  int hit(double x, double y) => _cvHit(_h, x, y);
  /// Runs [iters] layout iterations, returns the remaining temperature.
  double layoutStep(int iters) {
    _check();
    return _cvLayoutStep(_h, iters);
  }

  void layoutReset() => _cvLayoutReset(_h);

  /// Builds the frame and returns zero-copy views over the native buffers.
  GFrame frame() {
    _check();
    final info = _cvFrame(_h);
    return GFrame._(
      info.f32Ptr.asTypedList(info.f32Len),
      info.u32Ptr.cast<Int32>().asTypedList(info.u32Len),
      info,
    );
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _cvFree(_h);
  }
}