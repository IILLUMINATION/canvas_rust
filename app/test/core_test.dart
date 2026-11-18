import 'package:canvas_app/src/native.dart';
import 'package:flutter_test/flutter_test.dart';

double _worldX(GraphCore c, double sx, double vw) => c.camX + (sx - vw / 2) / c.scale;
double _worldY(GraphCore c, double sy, double vh) => c.camY + (sy - vh / 2) / c.scale;

void main() {
  test('demo graph produces a frame', () {
    final core = GraphCore();
    core.setViewport(800, 600);
    core.demo(1000);
    core.fit();
    final fr = core.frame();
    expect(fr.totalNodes, 1000);
    expect(fr.totalEdges, greaterThan(1000));
    expect(fr.visNodes, 1000, reason: 'fit must show the whole graph');
    expect(fr.visEdges, greaterThan(0));
    expect(fr.edges.length, fr.edgesCount * (fr.edgeQuads == 1 ? 8 : 4));
    expect(fr.f.isNotEmpty, isTrue);
    expect(fr.u.isNotEmpty, isTrue);
    for (final v in fr.edges) {
      expect(v.isFinite, isTrue);
    }
    core.dispose();
  });

  test('empty canvas is safe', () {
    final core = GraphCore();
    core.setViewport(320, 240);
    final fr = core.frame();
    expect(fr.visNodes, 0);
    expect(fr.edgesCount, 0);
    expect(core.hit(10, 10), GraphCore.noNode);
    core.dispose();
  });

  test('zoom in culls nodes, zoom out restores them', () {
    final core = GraphCore();
    core.setViewport(800, 600);
    core.demo(5000);
    core.fit();
    final all = core.frame().visNodes;
    core.zoom(400, 300, 6.0);
    final zoomed = core.frame().visNodes;
    expect(all, 5000);
    expect(zoomed, lessThan(all));
    core.zoom(400, 300, 1 / 6.0);
    expect(core.frame().visNodes, all);
    core.dispose();
  });

  test('zoom keeps the world point under the cursor', () {
    final core = GraphCore();
    core.setViewport(400, 300);
    core.addNode(0, 0, 100, 40, 0xFF3366FF);
    core.setViewport(400, 300);
    core.frame();
    final wx0 = _worldX(core, 120, 400);
    final wy0 = _worldY(core, 90, 300);
    core.zoom(120, 90, 3.0);
    expect(_worldX(core, 120, 400), closeTo(wx0, 0.01));
    expect(_worldY(core, 90, 300), closeTo(wy0, 0.01));
    core.dispose();
  });

  test('pan moves content with the pointer', () {
    final core = GraphCore();
    core.setViewport(400, 300);
    core.demo(100);
    core.fit();
    final before = core.frame().rects[0];
    core.pan(25, -10);
    final after = core.frame().rects[0];
    expect(after - before, closeTo(25.0, 0.5));
    final beforeY = core.frame().rects[1];
    core.pan(0, 40);
    expect(core.frame().rects[1] - beforeY, closeTo(40.0, 0.5));
    core.dispose();
  });

  test('hit test picks nodes and ignores empty space', () {
    final core = GraphCore();
    core.setViewport(400, 400);
    final a = core.addNode(0, 0, 100, 40, 0xFF112233);
    final b = core.addNode(1000, 0, 100, 40, 0xFF112234);
    core.fit();
    double sx(double wx) => (wx - core.camX) * core.scale + 200;
    double sy(double wy) => (wy - core.camY) * core.scale + 200;
    expect(core.hit(sx(0), sy(0)), a);
    expect(core.hit(sx(1000), sy(0)), b);
    expect(core.hit(sx(500), sy(0)), GraphCore.noNode, reason: 'empty space between nodes');
    expect(core.hit(sx(0), sy(0) + 60), GraphCore.noNode, reason: 'below the node');
    core.dispose();
  });

  test('hit test follows a dragged node without a rebuild', () {
    final core = GraphCore();
    core.setViewport(400, 400);
    final a = core.addNode(0, 0, 100, 40, 0xFF112233);
    core.fit();
    double sx(double wx) => (wx - core.camX) * core.scale + 200;
    expect(core.hit(sx(0), 200), a);
    core.moveNode(a, 200, 0);
    expect(core.hit(sx(200), 200), a);
    expect(core.hit(sx(0), 200), GraphCore.noNode);
    core.dispose();
  });

  test('layout converges and shortens edges', () {
    final core = GraphCore();
    core.setViewport(800, 600);
    core.demo(300);
    double total() {
      var sum = 0.0;
      for (var i = 0; i + 1 < 300; i++) {
        final dx = core.nodeX(i) - core.nodeX(i + 1);
        final dy = core.nodeY(i) - core.nodeY(i + 1);
        sum += dx * dx + dy * dy;
      }
      return sum;
    }

    final before = total();
    core.layoutReset();
    var temp = 1.0;
    var steps = 0;
    while (temp > 0.5 && steps < 600) {
      temp = core.layoutStep(4);
      steps++;
    }
    expect(steps, lessThan(600), reason: 'temperature must reach the floor');
    expect(total(), lessThan(before));
    for (var i = 0; i < 300; i++) {
      expect(core.nodeX(i).isFinite && core.nodeY(i).isFinite, isTrue);
    }
    core.dispose();
  });

  test('LOD switches from quads to rects when zooming in', () {
    final core = GraphCore();
    core.setViewport(1200, 800);
    core.demo(30000);
    core.fit();
    final dense = core.frame();
    expect(dense.lod, 0, reason: 'mass view must batch triangles');
    expect(dense.quadsCount, dense.visNodes);
    core.zoom(600, 400, 12.0);
    final close = core.frame();
    expect(close.lod, greaterThan(0));
    expect(close.quadsCount, 0);
    expect(close.rectsCount, close.visNodes);
    core.dispose();
  });
}