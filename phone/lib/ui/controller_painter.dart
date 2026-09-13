/// Draws the whole controller in a single [CustomPainter].
///
/// This is the reason touch input does not cause widget rebuilds: the painter
/// listens directly to the [TouchRouter], so a press repaints one layer and
/// runs no build, no layout and no diff. Flat shapes and pre-built [Paint]
/// objects keep each frame cheap.
///
/// The controller is drawn over somebody else's game, so it is monochrome and
/// outline-first. Weight carries state rather than colour: a pressed control
/// gets a brighter edge and a faint fill. Anything more assertive competes with
/// the thing the player is actually looking at.
library;

import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../core/protocol/packets.dart';
import '../model/layout.dart';
import 'theme.dart';
import 'touch_router.dart';

@immutable
class ControllerTheme {
  final Color idle;
  final Color pressed;
  final Color outline;
  final Color outlineActive;
  final Color label;
  final Color stickBase;
  final Color stickKnob;
  final Color accent;

  const ControllerTheme({
    this.idle = const Color(0x14FFFFFF),
    this.pressed = const Color(0x3DFFFFFF),
    this.outline = const Color(0x59FFFFFF),
    this.outlineActive = const Color(0xF0FFFFFF),
    this.label = const Color(0xCCFFFFFF),
    this.stickBase = const Color(0x0FFFFFFF),
    this.stickKnob = const Color(0x8AFFFFFF),
    this.accent = const Color(0xFF8178FF),
  });

  @override
  bool operator ==(Object other) =>
      other is ControllerTheme &&
      other.idle == idle &&
      other.pressed == pressed &&
      other.outline == outline &&
      other.outlineActive == outlineActive &&
      other.label == label &&
      other.stickBase == stickBase &&
      other.stickKnob == stickKnob &&
      other.accent == accent;

  @override
  int get hashCode => Object.hash(
    idle,
    pressed,
    outline,
    outlineActive,
    label,
    stickBase,
    stickKnob,
    accent,
  );
}

class ControllerPainter extends CustomPainter {
  ControllerPainter({
    required this.router,
    required this.theme,
    this.editorSelection,
    this.showGrid = false,
  }) : super(repaint: router);

  final TouchRouter router;
  final ControllerTheme theme;

  /// Set in the editor to highlight the control being manipulated.
  final String? editorSelection;
  final bool showGrid;

  final Paint _fill = Paint()..style = PaintingStyle.fill;
  final Paint _stroke = Paint()
    ..style = PaintingStyle.stroke
    ..strokeWidth = 2.0
    ..strokeCap = StrokeCap.round;

  // Text layout is the expensive part of painting, so lay each label out once
  // and reuse the paragraph until its text or size changes.
  final Map<String, ui.Paragraph> _labelCache = {};

  @override
  void paint(Canvas canvas, Size size) {
    if (showGrid) _paintGrid(canvas, size);

    for (final g in router.geometry) {
      final spec = g.spec;
      final selected = spec.id == editorSelection;
      canvas.save();
      if (spec.rotation != 0) {
        canvas.translate(g.cx, g.cy);
        canvas.rotate(spec.rotation);
        canvas.translate(-g.cx, -g.cy);
      }

      switch (spec.type) {
        case ControlType.button:
          _paintButton(canvas, g, selected);
        case ControlType.trigger:
          _paintTrigger(canvas, g, selected);
        case ControlType.dpad:
          _paintDpad(canvas, g, selected);
        case ControlType.stick:
          _paintStick(canvas, g, selected);
      }

      canvas.restore();
    }
  }

  // --- helpers ---------------------------------------------------------------

  double _alpha(ControlSpec s) => s.opacity.clamp(0.05, 1.0);

  Color _withOpacity(Color c, double factor) =>
      c.withValues(alpha: (c.a * factor).clamp(0.0, 1.0));

  /// Set up the stroke for a control edge. One place, so the pressed and
  /// selected treatments cannot drift apart between control types.
  void _edge(bool down, bool selected, double alpha) {
    _stroke.color = selected
        ? _withOpacity(theme.accent, 1.0)
        : _withOpacity(down ? theme.outlineActive : theme.outline, alpha);
    _stroke.strokeWidth = selected ? 3.0 : (down ? 2.5 : 2.0);
  }

  void _shape(Canvas canvas, ControlGeometry g, Paint paint) {
    final rect = Rect.fromCenter(
      center: Offset(g.cx, g.cy),
      width: g.width,
      height: g.height,
    );
    switch (g.spec.shape) {
      case ControlShape.circle:
        canvas.drawCircle(Offset(g.cx, g.cy), g.radius, paint);
      case ControlShape.pill:
        canvas.drawRRect(
          RRect.fromRectAndRadius(rect, Radius.circular(g.height / 2)),
          paint,
        );
      case ControlShape.roundedRect:
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            rect,
            Radius.circular(math.min(g.width, g.height) * 0.34),
          ),
          paint,
        );
    }
  }

  void _paintButton(Canvas canvas, ControlGeometry g, bool selected) {
    final down = router.visuals.pressed.contains(g.spec.id);
    final a = _alpha(g.spec);

    _fill.color = _withOpacity(down ? theme.pressed : theme.idle, a);
    _shape(canvas, g, _fill);

    _edge(down, selected, a);
    _shape(canvas, g, _stroke);

    _paintLabel(canvas, g, a, down);
  }

  void _paintTrigger(Canvas canvas, ControlGeometry g, bool selected) {
    final a = _alpha(g.spec);
    final down = router.visuals.pressed.contains(g.spec.id);
    final travel = router.visuals.triggerTravel[g.spec.id] ?? 0.0;
    final rect = Rect.fromCenter(
      center: Offset(g.cx, g.cy),
      width: g.width,
      height: g.height,
    );
    final radius = Radius.circular(math.min(g.width, g.height) * 0.34);

    _fill.color = _withOpacity(theme.idle, a);
    _shape(canvas, g, _fill);

    // Fill from the bottom in proportion to the value actually being sent, so
    // a feathered trigger shows how far in it is rather than just on or off.
    if (travel > 0) {
      canvas.save();
      canvas.clipRRect(RRect.fromRectAndRadius(rect, radius));
      _fill.color = _withOpacity(theme.pressed, a);
      canvas.drawRect(
        Rect.fromLTRB(
          rect.left,
          rect.bottom - rect.height * travel,
          rect.right,
          rect.bottom,
        ),
        _fill,
      );
      canvas.restore();
    }

    _edge(down, selected, a);
    _shape(canvas, g, _stroke);
    _paintLabel(canvas, g, a, down);
  }

  void _paintDpad(Canvas canvas, ControlGeometry g, bool selected) {
    final a = _alpha(g.spec);
    final centre = Offset(g.cx, g.cy);
    final r = g.radius;
    final bits = router.visuals.dpadBits[g.spec.id] ?? 0;

    // Two crosses stacked, the cardinal one over a diagonal one, which is the
    // construction of the mark and also what an eight-way pad is. Each cross
    // is one outline, so there are no seams where its arms meet, and each arm
    // has its own fill so the direction being pushed lights on its own. A
    // diagonal press lights the diagonal arm: the pad shows what it sends.
    const dirs = <(int, double, double)>[
      (Btn.dpadUp, 0.0, -1.0),
      (Btn.dpadRight, 1.0, 0.0),
      (Btn.dpadDown, 0.0, 1.0),
      (Btn.dpadLeft, -1.0, 0.0),
    ];
    const diagonals = <(int, double, double)>[
      (Btn.dpadUp | Btn.dpadRight, 1.0, -1.0),
      (Btn.dpadDown | Btn.dpadRight, 1.0, 1.0),
      (Btn.dpadDown | Btn.dpadLeft, -1.0, 1.0),
      (Btn.dpadUp | Btn.dpadLeft, -1.0, -1.0),
    ];

    final arm = r * 0.46;
    final cross = _cross(centre, r, arm, Radius.circular(arm * 0.32));
    final diagArm = arm * 0.66;
    final diag =
        _cross(
          centre,
          r * 0.86,
          diagArm,
          Radius.circular(diagArm * 0.32),
        ).transform(
          (Matrix4.identity()
                ..translateByDouble(centre.dx, centre.dy, 0, 1)
                ..rotateZ(math.pi / 4)
                ..translateByDouble(-centre.dx, -centre.dy, 0, 1))
              .storage,
        );

    // The diagonal cross sits under the cardinal one, so it is drawn first
    // and a little quieter.
    _fill.color = _withOpacity(theme.idle, a * 0.8);
    canvas.drawPath(diag, _fill);
    for (final (mask, ux, uy) in diagonals) {
      if (bits & mask != mask) continue;
      canvas.save();
      canvas.clipPath(diag);
      _fill.color = _withOpacity(theme.pressed, a);
      canvas.drawRect(
        Rect.fromPoints(centre, centre + Offset(ux * r, uy * r)),
        _fill,
      );
      canvas.restore();
    }
    _stroke
      ..color = _withOpacity(theme.outline, a * 0.7)
      ..strokeWidth = 1.5;
    canvas.drawPath(diag, _stroke);

    _fill.color = _withOpacity(theme.idle, a);
    canvas.drawPath(cross, _fill);
    for (final (bit, ux, uy) in dirs) {
      if (bits & bit == 0) continue;
      canvas.save();
      canvas.clipPath(cross);
      _fill.color = _withOpacity(theme.pressed, a);
      canvas.drawRect(
        Rect.fromCenter(
          center: centre + Offset(ux * r * 0.55, uy * r * 0.55),
          width: ux == 0 ? arm : r * 0.9,
          height: ux == 0 ? r * 0.9 : arm,
        ),
        _fill,
      );
      canvas.restore();
    }
    _edge(bits != 0, selected, a);
    canvas.drawPath(cross, _stroke);

    // The hollow square at the centre of the mark, filled here.
    final c = arm * 0.16;
    _fill.color = _withOpacity(theme.outline, a * 0.75);
    canvas.drawRect(
      Rect.fromCenter(center: centre, width: c, height: c),
      _fill,
    );
  }

  /// A plus of two rounded bars, as one outline.
  Path _cross(Offset centre, double half, double arm, Radius corner) {
    final vertical = Rect.fromCenter(
      center: centre,
      width: arm,
      height: half * 2,
    );
    final horizontal = Rect.fromCenter(
      center: centre,
      width: half * 2,
      height: arm,
    );
    return Path.combine(
      PathOperation.union,
      Path()..addRRect(RRect.fromRectAndRadius(vertical, corner)),
      Path()..addRRect(RRect.fromRectAndRadius(horizontal, corner)),
    );
  }

  void _paintStick(Canvas canvas, ControlGeometry g, bool selected) {
    final a = _alpha(g.spec);
    final anchored = Offset(g.cx, g.cy);
    final ring = router.visuals.floatingCentre[g.spec.id] ?? anchored;
    final r = g.radius;
    final held = router.visuals.knob.containsKey(g.spec.id);

    // A floating stick keeps a faint home ring, so its resting place stays
    // discoverable while the thumb has taken the stick somewhere else.
    if (g.spec.floating && ring != anchored) {
      _stroke
        ..color = _withOpacity(theme.outline, a * 0.2)
        ..strokeWidth = 1.5;
      canvas.drawCircle(anchored, r, _stroke);
    }

    _fill.color = _withOpacity(theme.stickBase, a);
    canvas.drawCircle(ring, r, _fill);

    _edge(held, selected, a * 0.9);
    canvas.drawCircle(ring, r, _stroke);

    // Where full deflection is actually reached. Drawn only while the stick is
    // held: it teaches the limit at the one moment that is useful, and stays
    // out of the way the rest of the time.
    if (held && g.spec.saturation < 0.99) {
      _stroke
        ..color = _withOpacity(theme.outline, a * 0.28)
        ..strokeWidth = 1.0;
      canvas.drawCircle(ring, r * g.spec.saturation, _stroke);
    }

    final knob = router.visuals.knob[g.spec.id] ?? Offset.zero;
    final knobCentre = ring + knob;
    final knobRadius = r * 0.40;

    _fill.color = _withOpacity(theme.stickKnob, a);
    canvas.drawCircle(knobCentre, knobRadius, _fill);

    // A concentric lip, so the knob reads as a domed thing to push rather than
    // a flat disc. Deliberately not a set of grip lines: three stacked strokes
    // on a circle read as a menu icon, not as a stick.
    _stroke
      ..color = _withOpacity(const Color(0x26000000), a)
      ..strokeWidth = 1.2;
    canvas.drawCircle(knobCentre, knobRadius * 0.62, _stroke);

    if (g.spec.label.isNotEmpty) {
      _paintTextAt(canvas, g.spec.label, knobCentre, knobRadius * 0.8, a * 0.7);
    }
  }

  void _paintLabel(Canvas canvas, ControlGeometry g, double alpha, bool down) {
    if (g.spec.label.isEmpty) return;
    final maxSize = math.min(g.width, g.height);
    _paintTextAt(
      canvas,
      g.spec.label,
      Offset(g.cx, g.cy),
      maxSize * (g.spec.label.length > 2 ? 0.30 : 0.46),
      down ? alpha : alpha * 0.85,
    );
  }

  void _paintTextAt(
    Canvas canvas,
    String text,
    Offset centre,
    double fontSize,
    double alpha,
  ) {
    final key =
        '$text|${fontSize.toStringAsFixed(1)}|${alpha.toStringAsFixed(2)}';
    var paragraph = _labelCache[key];
    if (paragraph == null) {
      final builder =
          ui.ParagraphBuilder(
              ui.ParagraphStyle(
                textAlign: TextAlign.center,
                fontFamily: kFont,
                fontSize: fontSize,
                fontWeight: FontWeight.w600,
              ),
            )
            ..pushStyle(
              ui.TextStyle(
                color: _withOpacity(theme.label, alpha),
                letterSpacing: fontSize * 0.06,
              ),
            )
            ..addText(text);
      paragraph = builder.build()
        ..layout(const ui.ParagraphConstraints(width: 400));
      // Bound the cache: labels are few, but the editor can churn font sizes
      // while a control is being resized.
      if (_labelCache.length > 128) _labelCache.clear();
      _labelCache[key] = paragraph;
    }
    canvas.drawParagraph(
      paragraph,
      Offset(centre.dx - 200, centre.dy - paragraph.height / 2),
    );
  }

  void _paintGrid(Canvas canvas, Size size) {
    _stroke
      ..color = const Color(0x0FFFFFFF)
      ..strokeWidth = 1.0;
    for (var i = 1; i < 20; i++) {
      final x = size.width * i / 20;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), _stroke);
    }
    for (var i = 1; i < 12; i++) {
      final y = size.height * i / 12;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), _stroke);
    }
  }

  @override
  bool shouldRepaint(ControllerPainter old) =>
      old.editorSelection != editorSelection ||
      old.showGrid != showGrid ||
      old.theme != theme ||
      !identical(old.router, router);
}
