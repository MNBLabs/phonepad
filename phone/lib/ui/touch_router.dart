/// Routes raw pointer events to controls and produces controller state.
///
/// Two decisions here matter more than anything else for how the pad feels:
///
/// 1. **Pointer capture.** Once a finger is assigned to a control it keeps that
///    control until it lifts, even if it slides off. Without this, sticks let
///    go the moment your thumb strays past the ring.
///
/// 2. **Full recomputation on every event.** The whole state is rebuilt from
///    the set of live pointers rather than incrementally toggled. That makes a
///    stuck button structurally impossible: if no finger is on A, the A bit
///    cannot be set, regardless of what sequence of events got us here.
library;

import 'dart:async';
import 'dart:ui';

import 'package:flutter/foundation.dart';

import '../core/protocol/packets.dart';
import '../model/layout.dart';

class _Pointer {
  final ControlGeometry geom;

  /// Where the stick's centre currently is. For a floating stick this is where
  /// the finger first landed; otherwise the control's own centre. Not final:
  /// a drifting stick moves its own origin to follow a finger that has pushed
  /// past the ring.
  Offset origin;

  Offset position;

  _Pointer({required this.geom, required this.origin, required this.position});
}

/// Visual feedback the painter needs, kept separate from the wire state.
class ControlVisuals {
  final Set<String> pressed = {};

  /// Knob displacement from the control centre, in logical pixels.
  final Map<String, Offset> knob = {};

  /// Where a floating stick's ring is currently drawn.
  final Map<String, Offset> floatingCentre = {};

  /// 0..1 travel for analogue triggers.
  final Map<String, double> triggerTravel = {};

  /// Which directions each d-pad is currently reporting, so the painter can
  /// light the arm being pushed rather than the whole control.
  final Map<String, int> dpadBits = {};

  void clear() {
    pressed.clear();
    knob.clear();
    floatingCentre.clear();
    triggerTravel.clear();
    dpadBits.clear();
  }
}

class TouchRouter extends ChangeNotifier {
  TouchRouter({
    required this.state,
    this.onStateChanged,
    this.onPressFeedback,
    this.onLongPress,
  });

  /// The state object that gets serialised onto the wire. Mutated in place so
  /// the hot path allocates nothing.
  final ControllerState state;

  /// Called after any change, so the transport can send immediately rather
  /// than waiting for its next tick.
  final VoidCallback? onStateChanged;

  /// Called once per *new* press, for haptics. Never called on move.
  final void Function(ControlSpec spec)? onPressFeedback;

  /// Called when a control is held without being released. The button itself
  /// still reports normally throughout, so a long press is additive: holding
  /// Guide sends Guide, and also offers the repair.
  final void Function(ControlSpec spec)? onLongPress;

  Timer? _longPressTimer;

  List<ControlGeometry> geometry = const [];
  final ControlVisuals visuals = ControlVisuals();
  final Map<int, _Pointer> _pointers = {};

  bool get hasActiveTouches => _pointers.isNotEmpty;

  /// Force a repaint. The editor mutates geometry directly rather than through
  /// touch, so it needs a way to say "redraw" without faking an event.
  void notify() => notifyListeners();

  void updateGeometry(List<ControlGeometry> g) {
    geometry = g;
    if (_pointers.isNotEmpty) {
      // A layout change while fingers are down would leave them pointing at
      // stale geometry, so start clean. This notifies as a side effect.
      releaseAll();
      return;
    }
    // Must still notify with no fingers down: the painter repaints only when
    // this listenable fires, so a rotation with an idle screen would otherwise
    // keep drawing the previous orientation's frame.
    notifyListeners();
  }

  ControlGeometry? _hit(Offset p) {
    // Iterate in reverse so controls drawn on top win the touch.
    for (var i = geometry.length - 1; i >= 0; i--) {
      final g = geometry[i];
      if (g.hitTest(p.dx, p.dy)) return g;
    }
    return null;
  }

  void onPointerDown(int id, Offset pos) {
    final g = _hit(pos);
    if (g == null) return;

    // A floating stick and a sliding trigger both measure from wherever the
    // finger actually landed; everything else measures from the drawn centre.
    final relative =
        (g.spec.type == ControlType.stick && g.spec.floating) ||
        (g.spec.type == ControlType.trigger && g.spec.analogSlide);
    final origin = relative ? pos : Offset(g.cx, g.cy);

    _pointers[id] = _Pointer(geom: g, origin: origin, position: pos);
    onPressFeedback?.call(g.spec);

    if (onLongPress != null && g.spec.type == ControlType.button) {
      _longPressTimer?.cancel();
      _longPressTimer = Timer(const Duration(milliseconds: 700), () {
        // Only if that finger is still down on the same control.
        if (_pointers[id]?.geom.spec.id == g.spec.id) onLongPress!(g.spec);
      });
    }
    _recompute();
  }

  void onPointerMove(int id, Offset pos) {
    final p = _pointers[id];
    if (p == null) return;
    if (p.position == pos) return;
    p.position = pos;
    _recompute();
  }

  void onPointerUp(int id) {
    if (_pointers.remove(id) == null) return;
    _longPressTimer?.cancel();
    _recompute();
  }

  /// Forget every finger and report neutral.
  ///
  /// For moments where a pointer-up may genuinely never arrive: app pause, a
  /// layout change, leaving the screen. Not for a dropped connection — the
  /// fingers are still on the glass then, and forgetting them means they stay
  /// dead until they are lifted and put back down.
  void releaseAll() {
    _longPressTimer?.cancel();
    if (_pointers.isEmpty && state.isNeutral) return;
    _pointers.clear();
    _recompute();
  }

  /// Rebuild the reported state from the fingers that are still down.
  ///
  /// Used when the link comes back: whatever is being held should take effect
  /// again immediately, without the player having to lift and re-place a thumb
  /// that never moved.
  void resync() => _recompute();

  @override
  void dispose() {
    _longPressTimer?.cancel();
    super.dispose();
  }

  void _recompute() {
    state.reset();
    visuals.clear();

    for (final p in _pointers.values) {
      _apply(p);
    }

    onStateChanged?.call();
    notifyListeners();
  }

  void _apply(_Pointer p) {
    final spec = p.geom.spec;
    final id = spec.id;

    switch (spec.type) {
      case ControlType.button:
        state.buttons |= spec.mapping.buttons;
        visuals.pressed.add(id);

      case ControlType.dpad:
        final d = p.position - Offset(p.geom.cx, p.geom.cy);
        final bits = computeDpad(
          dx: d.dx,
          dy: d.dy,
          radius: p.geom.radius,
          deadzone: spec.deadzone,
        );
        state.buttons |= bits;
        visuals.pressed.add(id);
        visuals.dpadBits[id] = bits;

      case ControlType.stick:
        var d = p.position - p.origin;

        // Past the ring, drag the origin along so the finger stays on the rim.
        // Without this, easing off full lock means retracing however far the
        // finger overshot before anything changes.
        final limit = p.geom.radius * spec.saturation;
        if (spec.drift && limit > 0 && d.distance > limit) {
          p.origin = p.position - d / d.distance * limit;
          d = p.position - p.origin;
        }

        final v = computeStick(
          dx: d.dx,
          dy: d.dy,
          radius: p.geom.radius,
          deadzone: spec.deadzone,
          antiDeadzone: spec.antiDeadzone,
          saturation: spec.saturation,
          sensitivity: spec.sensitivity,
          circular: spec.circularRange,
          curve: spec.responseCurve,
        );
        if (spec.mapping.stick == StickSide.left) {
          state.lx = v.x;
          state.ly = v.y;
        } else {
          state.rx = v.x;
          state.ry = v.y;
        }

        // Clamp the drawn knob to the ring so it never escapes its control.
        var draw = d;
        final r = p.geom.radius;
        if (draw.distance > r) draw = draw / draw.distance * r;
        visuals.knob[id] = draw;
        if (spec.floating) visuals.floatingCentre[id] = p.origin;

      case ControlType.trigger:
        final int value;
        if (spec.analogSlide) {
          // Relative, not absolute. Measuring travel from the top edge would
          // mean a tap in the middle of the control is half throttle, which
          // gets the common case wrong; a trigger tap should be a full press.
          // Touch-down is full, and sliding *up* feathers it back.
          final lift = p.origin.dy - p.position.dy;
          value = computeTrigger(
            travel: p.geom.height - lift,
            height: p.geom.height,
            curve: spec.responseCurve,
          );
        } else {
          value = 255;
        }
        if (spec.mapping.trigger == StickSide.left) {
          state.lt = value;
        } else {
          state.rt = value;
        }
        visuals.pressed.add(id);
        visuals.triggerTravel[id] = value / 255.0;
    }
  }
}
