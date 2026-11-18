import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';

/// What a node shows inside its card. The Rust core only decides *which* nodes
/// are large enough to be worth content; the text itself is a client concern
/// (fonts, shaping, caching all live in Flutter).
class NodeContent {
  const NodeContent({required this.title, this.body = '', this.badge = '', this.icon = ''});

  final String title;
  final String body;
  final String badge;

  /// Key into `IconCache.sources`; empty means "no icon".
  final String icon;
}

typedef NodeContentProvider = NodeContent? Function(int id);

const List<String> _words = <String>[
  'alpha', 'beta', 'gamma', 'delta', 'river', 'stone', 'harbor', 'signal',
  'tunnel', 'garden', 'silent', 'cipher', 'ladder', 'meadow', 'anchor',
];

/// Deterministic content for the demo graph: a title, a wrapping body line for
/// every third node and a badge for every fifth.
class DemoContentProvider {
  final Map<int, NodeContent> _cache = <int, NodeContent>{};

  NodeContent? call(int id) {
    final hit = _cache[id];
    if (hit != null) return hit;
    final body = id % 3 == 0
        ? '${_words[id % _words.length]} ${_words[(id ~/ 7) % _words.length]} '
            '${_words[(id ~/ 13) % _words.length]}'
        : '';
    const icons = <String>['star', 'check', 'warn'];
    final content = NodeContent(
      title: 'Node $id',
      body: body,
      badge: id % 5 == 0 ? '${id % 97}' : '',
      icon: id % 4 == 0 ? icons[(id ~/ 4) % icons.length] : '',
    );
    if (_cache.length > 4096) _cache.clear();
    _cache[id] = content;
    return content;
  }
}

/// Inline SVG sources for the demo. Any string map works: the engine only needs
/// `key -> svg markup`.
const Map<String, String> demoIconSources = <String, String>{
  'star': '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">'
      '<path fill="#FFD166" d="M12 2l2.9 6.3 6.9.8-5.1 4.7 1.4 6.8L12 17.8l-6.1 2.8 1.4-6.8L2.2 9.1l6.9-.8z"/></svg>',
  'check': '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">'
      '<path fill="none" stroke="#33C6A3" stroke-width="3" stroke-linecap="round" d="M4 13l5 5L20 6"/></svg>',
  'warn': '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">'
      '<path fill="#E0524E" d="M12 3l10 18H2L12 3z"/></svg>',
};

/// Laid-out card content, cached per node and per text-size bucket so that
/// panning never triggers text layout.
class CardLayout {
  CardLayout({
    required this.size,
    required this.title,
    required this.body,
    required this.badge,
    required this.pad,
  });

  final double size;
  final TextPainter title;
  final TextPainter? body;
  final TextPainter? badge;
  final double pad;

  double get badgeW => badge == null ? 0 : badge!.width + pad * 1.2;
  double get badgeH => badge == null ? 0 : badge!.height + pad * 0.4;

  /// Text size follows the card so it stays proportionate while zooming, but is
  /// quantised so that panning reuses one layout.
  static double quantise(double cardHeight) {
    final raw = (cardHeight * 0.17).clamp(9.0, 20.0);
    return (raw * 2).roundToDouble() / 2;
  }

  /// Icon slot width for a given text size; the layout reserves it even before
  /// the raster is ready, so text never shifts when the icon appears.
  static double iconSlot(double size) => size * 1.45;

  static CardLayout build(NodeContent content, double size, double maxWidth) {
    final pad = (size * 0.42).clamp(3.0, 9.0);
    TextPainter lay(String text, double fontSize, FontWeight w, int maxLines) {
      final tp = TextPainter(
        text: TextSpan(
          text: text,
          style: TextStyle(
            color: const Color(0xFFEFF5FF),
            fontSize: fontSize,
            fontWeight: w,
            height: 1.15,
          ),
        ),
        textDirection: TextDirection.ltr,
        maxLines: maxLines,
        ellipsis: '…',
      )..layout(maxWidth: (maxWidth - pad * 2).clamp(8.0, 4000.0));
      return tp;
    }

    final title = lay(content.title, size, FontWeight.w600, 1);
    final body = content.body.isEmpty
        ? null
        : lay(content.body, size * 0.88, FontWeight.w400, 2);
    final badge = content.badge.isEmpty ? null : lay(content.badge, size * 0.8, FontWeight.w600, 1);
    return CardLayout(size: size, title: title, body: body, badge: badge, pad: pad);
  }

  void paint(Canvas canvas, Rect rect, {ui.Image? icon, String iconKey = ''}) {
    var left = rect.left + pad;
    final top = rect.top + pad;
    final slot = iconKey.isEmpty ? 0.0 : iconSlot(size);
    if (icon != null && slot > 0) {
      final side = slot * 0.86;
      final dst = Rect.fromLTWH(left, top + (title.height - side) * 0.5, side, side);
      canvas.drawImageRect(
        icon,
        Rect.fromLTWH(0, 0, icon.width.toDouble(), icon.height.toDouble()),
        dst,
        Paint()..filterQuality = FilterQuality.low,
      );
    }
    left += slot;
    final titleMaxW = rect.width - pad * 2 - badgeW - slot;
    final titleRect = Rect.fromLTWH(left, top, titleMaxW.clamp(1.0, 4000.0), title.height);
    title.paint(canvas, Offset(titleRect.left, titleRect.top));

    final bodyTp = body;
    if (bodyTp != null) {
      final y = top + title.height + pad * 0.35;
      if (y + bodyTp.height <= rect.bottom - pad * 0.6) {
        bodyTp.paint(canvas, Offset(left, y));
      }
    }

    final badgeTp = badge;
    if (badgeTp != null) {
      final w = badgeW;
      final h = badgeH;
      final br = Rect.fromLTWH(rect.right - pad - w, rect.top + pad * 0.8, w, h);
      canvas.drawRRect(
        RRect.fromRectAndRadius(br, Radius.circular(h * 0.5)),
        Paint()..color = const Color(0x336FE3FF),
      );
      badgeTp.paint(canvas, Offset(br.center.dx - badgeTp.width * 0.5, br.center.dy - badgeTp.height * 0.5));
    }
  }
}
