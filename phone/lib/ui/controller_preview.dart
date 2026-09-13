/// A still picture of the controller, drawn by the same painter that draws the
/// real one. Anywhere the product needs a picture of itself, this is it: not
/// an illustration of a controller, the controller.
library;

import 'package:flutter/material.dart';

import '../core/protocol/packets.dart';
import '../model/layout.dart';
import '../model/presets.dart';
import 'controller_painter.dart';
import 'theme.dart';
import 'touch_router.dart';

class ControllerPreview extends StatefulWidget {
  const ControllerPreview({super.key, this.layout, this.radius = kRadiusL});

  final ControllerLayout? layout;
  final double radius;

  @override
  State<ControllerPreview> createState() => _ControllerPreviewState();
}

class _ControllerPreviewState extends State<ControllerPreview> {
  final _router = TouchRouter(state: ControllerState());
  Size _laidOut = Size.zero;

  @override
  void dispose() {
    _router.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // A phone in landscape. The preview is the phone's own screen, so it takes
    // the phone's own proportions.
    return AspectRatio(
      aspectRatio: 19.5 / 9,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(widget.radius),
        child: ColoredBox(
          color: kNight,
          child: LayoutBuilder(
            builder: (context, constraints) {
              final size = Size(constraints.maxWidth, constraints.maxHeight);
              if (size != _laidOut) {
                _laidOut = size;
                final layout = widget.layout ?? standardLayout();
                _router.geometry = resolveGeometry(
                  layout.forOrientation(isLandscape: true),
                  size.width,
                  size.height,
                );
              }
              return IgnorePointer(
                child: CustomPaint(
                  painter: ControllerPainter(
                    router: _router,
                    // A touch brighter than in play: this is a picture on
                    // paper, not an overlay on a game.
                    theme: const ControllerTheme(
                      outline: Color(0x80FFFFFF),
                      idle: Color(0x1AFFFFFF),
                    ),
                  ),
                  size: size,
                ),
              );
            },
          ),
        ),
      ),
    );
  }
}
