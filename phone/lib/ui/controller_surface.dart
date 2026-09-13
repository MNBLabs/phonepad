/// The touch surface itself: raw pointers in, one painted layer out.
///
/// Separated from the controller screen so the part that must never break can
/// be driven directly by a test, with real Flutter pointer routing rather than
/// by calling the router's methods by hand.
///
/// A raw [Listener] is used rather than any gesture detector on purpose. The
/// gesture arena introduces disambiguation delay, lets controls steal each
/// other's pointers, and — the reason it matters most here — can cancel a
/// pointer that is already down when a second one arrives.
library;

import 'package:flutter/material.dart';

import 'controller_painter.dart';
import 'touch_router.dart';

class ControllerSurface extends StatelessWidget {
  const ControllerSurface({
    super.key,
    required this.router,
    this.theme = const ControllerTheme(),
    this.editorSelection,
    this.showGrid = false,
  });

  final TouchRouter router;
  final ControllerTheme theme;
  final String? editorSelection;
  final bool showGrid;

  @override
  Widget build(BuildContext context) {
    return Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: (e) => router.onPointerDown(e.pointer, e.localPosition),
      onPointerMove: (e) => router.onPointerMove(e.pointer, e.localPosition),
      onPointerUp: (e) => router.onPointerUp(e.pointer),
      onPointerCancel: (e) => router.onPointerUp(e.pointer),
      child: RepaintBoundary(
        child: CustomPaint(
          size: Size.infinite,
          painter: ControllerPainter(
            router: router,
            theme: theme,
            editorSelection: editorSelection,
            showGrid: showGrid,
          ),
        ),
      ),
    );
  }
}
