import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

/// Node icons.
///
/// INVARIANT: a vector icon is rasterised exactly once per (key, pixel bucket)
/// into a `ui.Image`, and from then on it is only blitted. Never parse SVG,
/// never build or tessellate a `Path` inside `paint()` — that turns a ~1 us
/// blit into tens of microseconds and blows the frame budget at a few hundred
/// visible cards.
///
/// Misses are handled by drawing nothing for one or two frames while the raster
/// completes asynchronously; the card layout reserves the slot, so text does not
/// jump when the icon appears.
class IconCache {
  IconCache({
    required this.sources,
    required this.onReady,
    this.maxEntries = 128,
  });

  /// key -> SVG markup.
  final Map<String, String> sources;

  /// Called after a raster completes, so the canvas can repaint once.
  final VoidCallback onReady;

  /// One image per (key, bucket) pair; old entries are disposed.
  final int maxEntries;

  static const List<int> buckets = <int>[16, 24, 32, 48, 64];

  final Map<String, _Entry> _images = <String, _Entry>{};
  final Set<String> _pending = <String>{};
  int _rasters = 0;

  /// Number of SVG rasterisations performed so far (test hook).
  int get rasterCount => _rasters;

  static int bucketFor(double px) {
    for (final b in buckets) {
      if (px <= b) return b;
    }
    return buckets.last;
  }

  /// Returns the icon if it is already rasterised, otherwise schedules the
  /// raster and returns null for this frame.
  ui.Image? lookup(String key, double pixelSize) {
    final px = bucketFor(pixelSize);
    final cacheKey = '$key@$px';
    final hit = _images[cacheKey];
    if (hit != null) {
      hit.used = ++_tick;
      return hit.image;
    }
    final svg = sources[key];
    if (svg == null) return null;
    if (_pending.add(cacheKey)) _schedule(key, svg, px, cacheKey);
    return null;
  }

  int _tick = 0;

  void _schedule(String key, String svg, int px, String cacheKey) {
    _rasters++;
    unawaited(() async {
      try {
        final info = await vg.loadPicture(SvgStringLoader(svg), null);
        final image = await info.picture.toImage(px, px);
        info.picture.dispose();
        _evictIfNeeded();
        _images[cacheKey] = _Entry(image, ++_tick);
        _pending.remove(cacheKey);
        onReady();
      } catch (_) {
        _pending.remove(cacheKey);
      }
    }());
  }

  void _evictIfNeeded() {
    while (_images.length >= maxEntries) {
      String? oldest;
      var stamp = 1 << 62;
      _images.forEach((k, v) {
        if (v.used < stamp) {
          stamp = v.used;
          oldest = k;
        }
      });
      if (oldest == null) return;
      _images.remove(oldest)?.image.dispose();
    }
  }

  void dispose() {
    for (final v in _images.values) {
      v.image.dispose();
    }
    _images.clear();
    _pending.clear();
  }
}

class _Entry {
  _Entry(this.image, this.used);
  final ui.Image image;
  int used;
}