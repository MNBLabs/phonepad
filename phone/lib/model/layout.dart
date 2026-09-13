/// The controller layout model.
///
/// Nothing about the on-screen controller is hardcoded: the play screen and the
/// editor both render whatever is in a [ControllerLayout]. Adding a new control
/// type means extending this file, not rewriting a screen.
///
/// ## Coordinate system
///
/// Positions are the control's **centre**, expressed as a fraction of the safe
/// area (0..1 on each axis). Sizes are a fraction of the *shorter* screen edge.
/// Splitting them this way is what makes one layout correct across aspect
/// ratios: a control keeps its physical size while the spread between controls
/// follows the screen. Nothing here is in pixels, so nothing here is tied to
/// one device.
library;

import 'dart:math' as math;

import '../core/protocol/packets.dart';

/// Defaults for the touch stick response.
///
/// These are starting points measured on real hardware, not received wisdom
/// from physical-controller tuning — see `docs/benchmarks.md` for the transfer
/// curves they came from.
///
/// Just enough to ignore the digitiser's noise under a thumb that is holding
/// still, and no more. Roughly a fifth of a millimetre on a 6.8" phone.
const double kStickNoiseFloor = 0.02;

/// Cancels the inner deadzone the *game* applies. See [ControlSpec.antiDeadzone].
const double kStickAntiDeadzone = 0.15;

/// Full deflection slightly inside the ring, so the edge is reachable.
const double kStickSaturation = 0.92;

/// Above 1.0, fine control near centre at the cost of coarser control far out.
const double kStickResponseCurve = 1.30;

enum ControlType { button, dpad, stick, trigger }

enum ControlShape { circle, roundedRect, pill }

enum StickSide { left, right }

/// Which physical control this maps to.
class ControlMapping {
  /// Button bitmask, for [ControlType.button].
  final int buttons;

  /// Which analogue stick, for [ControlType.stick].
  final StickSide? stick;

  /// Which trigger, for [ControlType.trigger]; also used for the stick-click
  /// button of a stick control.
  final StickSide? trigger;

  const ControlMapping({this.buttons = 0, this.stick, this.trigger});

  Map<String, dynamic> toJson() => {
        if (buttons != 0) 'buttons': buttons,
        if (stick != null) 'stick': stick!.name,
        if (trigger != null) 'trigger': trigger!.name,
      };

  static ControlMapping fromJson(Map<String, dynamic> j) => ControlMapping(
        buttons: (j['buttons'] as int?) ?? 0,
        stick: _side(j['stick'] as String?),
        trigger: _side(j['trigger'] as String?),
      );

  static StickSide? _side(String? s) => switch (s) {
        'left' => StickSide.left,
        'right' => StickSide.right,
        _ => null,
      };
}

class ControlSpec {
  String id;
  ControlType type;
  ControlMapping mapping;

  /// Centre position as a fraction of the safe area.
  double x;
  double y;

  /// Size as a fraction of the shorter screen edge.
  double size;

  /// Width divided by height. 1.0 for circles.
  double aspect;

  double rotation;
  double opacity;
  bool visible;
  String label;
  ControlShape shape;

  // --- stick tuning ---
  /// Movement below this fraction of the radius reads as "not moved".
  ///
  /// This exists only to reject digitiser noise from a resting thumb, so it is
  /// tiny. It is *not* the deadzone a physical controller needs: a stick has
  /// spring slop and sensor drift to hide, a touchscreen has neither, and the
  /// game at the far end is already applying its own. See [antiDeadzone].
  double deadzone;

  /// Fraction of full output emitted the instant the finger leaves centre.
  ///
  /// Games apply their own inner deadzone — typically 0.15-0.25 of raw XInput —
  /// because they assume a physical stick. Stacking ours on top of theirs is
  /// what makes a touch stick feel dead at first and then suddenly violent:
  /// a quarter of the thumb's travel does nothing at all, and by the time both
  /// dead bands are crossed the game's own response curve is already climbing.
  ///
  /// Starting the output above the game's dead band cancels it, so the whole
  /// thumb travel maps onto the range the game actually responds to.
  double antiDeadzone;

  /// Fraction of the radius at which output reaches maximum.
  ///
  /// Below 1.0 the outer edge of the ring is reachable without the thumb
  /// having to arrive exactly on it, which is what full lock in a racing game
  /// needs.
  double saturation;

  double sensitivity;
  bool circularRange;

  /// Recentre the stick wherever the finger lands, instead of anchoring it.
  ///
  /// Anchored, the offset between the drawn centre and wherever the thumb
  /// happened to land *is* deflection, so neutral moves on every re-grab and
  /// the control never behaves the same way twice.
  bool floating;

  /// Once the finger passes the ring, let the origin follow it.
  ///
  /// Without this, holding full lock and then easing off requires travelling
  /// back exactly as far as the finger overshot.
  bool drift;

  // --- trigger tuning ---
  /// Slide distance controls the value, instead of a full-press on touch.
  bool analogSlide;

  /// Exponent applied to stick magnitude and analogue trigger travel.
  /// 1.0 is linear; above 1.0 gives finer control near centre.
  double responseCurve;

  ControlSpec({
    required this.id,
    required this.type,
    required this.mapping,
    required this.x,
    required this.y,
    required this.size,
    this.aspect = 1.0,
    this.rotation = 0.0,
    this.opacity = 0.85,
    this.visible = true,
    this.label = '',
    this.shape = ControlShape.circle,
    this.deadzone = kStickNoiseFloor,
    this.antiDeadzone = kStickAntiDeadzone,
    this.saturation = kStickSaturation,
    this.sensitivity = 1.0,
    this.circularRange = true,
    this.floating = true,
    this.drift = true,
    this.analogSlide = false,
    this.responseCurve = 1.0,
  });

  ControlSpec copy() => ControlSpec(
        id: id,
        type: type,
        mapping: mapping,
        x: x,
        y: y,
        size: size,
        aspect: aspect,
        rotation: rotation,
        opacity: opacity,
        visible: visible,
        label: label,
        shape: shape,
        deadzone: deadzone,
        antiDeadzone: antiDeadzone,
        saturation: saturation,
        sensitivity: sensitivity,
        circularRange: circularRange,
        floating: floating,
        drift: drift,
        analogSlide: analogSlide,
        responseCurve: responseCurve,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'type': type.name,
        'mapping': mapping.toJson(),
        'x': x,
        'y': y,
        'size': size,
        'aspect': aspect,
        'rotation': rotation,
        'opacity': opacity,
        'visible': visible,
        'label': label,
        'shape': shape.name,
        'deadzone': deadzone,
        'antiDeadzone': antiDeadzone,
        'saturation': saturation,
        'sensitivity': sensitivity,
        'circularRange': circularRange,
        'floating': floating,
        'drift': drift,
        'analogSlide': analogSlide,
        'responseCurve': responseCurve,
      };

  static ControlSpec fromJson(Map<String, dynamic> j) => ControlSpec(
        id: j['id'] as String,
        type: ControlType.values.byName(j['type'] as String),
        mapping: ControlMapping.fromJson(
          (j['mapping'] as Map<String, dynamic>?) ?? const {},
        ),
        x: (j['x'] as num).toDouble(),
        y: (j['y'] as num).toDouble(),
        size: (j['size'] as num).toDouble(),
        aspect: (j['aspect'] as num?)?.toDouble() ?? 1.0,
        rotation: (j['rotation'] as num?)?.toDouble() ?? 0.0,
        opacity: (j['opacity'] as num?)?.toDouble() ?? 0.85,
        visible: (j['visible'] as bool?) ?? true,
        label: (j['label'] as String?) ?? '',
        shape: ControlShape.values.byName(
          (j['shape'] as String?) ?? ControlShape.circle.name,
        ),
        // Layouts written before these fields existed decode to the current
        // defaults rather than to the old behaviour: the old values are what
        // this release exists to correct.
        deadzone: (j['deadzone'] as num?)?.toDouble() ?? kStickNoiseFloor,
        antiDeadzone:
            (j['antiDeadzone'] as num?)?.toDouble() ?? kStickAntiDeadzone,
        saturation: (j['saturation'] as num?)?.toDouble() ?? kStickSaturation,
        sensitivity: (j['sensitivity'] as num?)?.toDouble() ?? 1.0,
        circularRange: (j['circularRange'] as bool?) ?? true,
        floating: (j['floating'] as bool?) ?? true,
        drift: (j['drift'] as bool?) ?? true,
        analogSlide: (j['analogSlide'] as bool?) ?? false,
        responseCurve: (j['responseCurve'] as num?)?.toDouble() ?? 1.0,
      );
}

/// A named layout. Holds a separate arrangement per orientation so one layout
/// works in both, switching automatically.
class ControllerLayout {
  String id;
  String name;
  bool builtIn;
  List<ControlSpec> landscape;
  List<ControlSpec> portrait;

  ControllerLayout({
    required this.id,
    required this.name,
    required this.landscape,
    required this.portrait,
    this.builtIn = false,
  });

  List<ControlSpec> forOrientation({required bool isLandscape}) =>
      isLandscape ? landscape : portrait;

  ControllerLayout copyWith({String? id, String? name, bool? builtIn}) =>
      ControllerLayout(
        id: id ?? this.id,
        name: name ?? this.name,
        builtIn: builtIn ?? this.builtIn,
        landscape: landscape.map((c) => c.copy()).toList(),
        portrait: portrait.map((c) => c.copy()).toList(),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'builtIn': builtIn,
        'landscape': landscape.map((c) => c.toJson()).toList(),
        'portrait': portrait.map((c) => c.toJson()).toList(),
      };

  static ControllerLayout fromJson(Map<String, dynamic> j) => ControllerLayout(
        id: j['id'] as String,
        name: j['name'] as String,
        builtIn: (j['builtIn'] as bool?) ?? false,
        landscape: ((j['landscape'] as List?) ?? const [])
            .cast<Map<String, dynamic>>()
            .map(ControlSpec.fromJson)
            .toList(),
        portrait: ((j['portrait'] as List?) ?? const [])
            .cast<Map<String, dynamic>>()
            .map(ControlSpec.fromJson)
            .toList(),
      );
}

/// Resolved on-screen geometry for one control, in logical pixels.
class ControlGeometry {
  final ControlSpec spec;
  final double cx;
  final double cy;
  final double width;
  final double height;

  const ControlGeometry({
    required this.spec,
    required this.cx,
    required this.cy,
    required this.width,
    required this.height,
  });

  double get radius => math.min(width, height) / 2;

  /// Generous hit testing: touch targets extend a little past the drawn shape,
  /// because a thumb that lands 2 mm off the edge of a button meant to press it.
  bool hitTest(double px, double py, {double slop = 1.15}) {
    final dx = px - cx;
    final dy = py - cy;
    if (spec.shape == ControlShape.circle) {
      final r = radius * slop;
      return dx * dx + dy * dy <= r * r;
    }
    return dx.abs() <= width / 2 * slop && dy.abs() <= height / 2 * slop;
  }
}

/// Turn normalised specs into pixel geometry for a given viewport.
List<ControlGeometry> resolveGeometry(
  List<ControlSpec> specs,
  double width,
  double height,
) {
  final unit = math.min(width, height);
  return [
    for (final s in specs)
      if (s.visible)
        ControlGeometry(
          spec: s,
          cx: s.x * width,
          cy: s.y * height,
          width: s.size * unit * s.aspect,
          height: s.size * unit,
        ),
  ];
}

// --- stick / trigger maths ---------------------------------------------------

/// Result of interpreting a finger position on a stick.
class StickValue {
  final int x;
  final int y;
  const StickValue(this.x, this.y);
  static const zero = StickValue(0, 0);
}

/// Convert a finger offset from the stick centre into XInput axis values.
///
/// Screen Y grows downward and XInput's Y grows upward, so Y is inverted here —
/// one place, rather than at every call site.
StickValue computeStick({
  required double dx,
  required double dy,
  required double radius,
  required double deadzone,
  required double antiDeadzone,
  required double saturation,
  required double sensitivity,
  required bool circular,
  required double curve,
}) {
  if (radius <= 0) return StickValue.zero;

  var nx = dx / radius;
  var ny = dy / radius;

  if (circular) {
    final mag = math.sqrt(nx * nx + ny * ny);
    if (mag > 1.0) {
      nx /= mag;
      ny /= mag;
    }
  } else {
    nx = nx.clamp(-1.0, 1.0);
    ny = ny.clamp(-1.0, 1.0);
  }

  // Which norm counts as "fully deflected" depends on the gate. For a circular
  // gate that is the Euclidean distance; for a square gate it is whichever axis
  // is furthest out, because a square gate is exactly the promise that both
  // axes can reach maximum at once. Measuring a square gate with the Euclidean
  // norm would silently cap diagonals at 1/sqrt(2) of full travel.
  final mag = circular
      ? math.sqrt(nx * nx + ny * ny)
      : math.max(nx.abs(), ny.abs());
  if (mag <= deadzone || mag == 0) return StickValue.zero;

  // Travel from the noise floor to saturation, as 0..1. Guarding the span keeps
  // a nonsensical config (saturation at or below the noise floor) from dividing
  // by zero; it degrades to full-scale-on-any-movement rather than crashing.
  final span = saturation - deadzone;
  var t = span > 1e-6 ? ((mag - deadzone) / span).clamp(0.0, 1.0) : 1.0;

  if (curve != 1.0) t = math.pow(t, curve).toDouble();

  // Start above the game's own dead band and spend the rest of the travel on
  // the range it responds to. Applied after the curve so the curve shapes the
  // usable range instead of being half-swallowed by the offset.
  //
  // This branch is only reached when mag > deadzone, so exact neutral has
  // already returned zero above. That ordering is load-bearing: a floor applied
  // unconditionally would leave the pad reading as permanently deflected.
  var scaled = antiDeadzone + (1.0 - antiDeadzone) * t;
  scaled *= sensitivity;

  // Scale the components together so the direction is preserved exactly.
  final k = scaled / mag;

  // 32767, never -32768: the companion rejects int16 minimum because it has no
  // positive counterpart and breaks symmetric scaling.
  return StickValue(
    (nx * k * 32767).round().clamp(-32767, 32767),
    (-ny * k * 32767).round().clamp(-32767, 32767),
  );
}

/// D-pad direction bits from a finger offset. Diagonals are produced when the
/// angle is genuinely between two axes, which is what makes 8-way movement
/// feel right rather than snapping.
int computeDpad({
  required double dx,
  required double dy,
  required double radius,
  required double deadzone,
}) {
  if (radius <= 0) return 0;
  final nx = dx / radius;
  final ny = dy / radius;
  final mag = math.sqrt(nx * nx + ny * ny);
  if (mag <= deadzone) return 0;

  // Split the circle into 8 sectors of 45 degrees.
  var angle = math.atan2(-ny, nx); // screen Y down -> maths Y up
  if (angle < 0) angle += 2 * math.pi;
  final sector = ((angle / (math.pi / 4)).round()) % 8;

  return switch (sector) {
    0 => Btn.dpadRight,
    1 => Btn.dpadRight | Btn.dpadUp,
    2 => Btn.dpadUp,
    3 => Btn.dpadUp | Btn.dpadLeft,
    4 => Btn.dpadLeft,
    5 => Btn.dpadLeft | Btn.dpadDown,
    6 => Btn.dpadDown,
    7 => Btn.dpadDown | Btn.dpadRight,
    _ => 0,
  };
}

/// Analogue trigger value from how far the finger has slid down the control.
int computeTrigger({
  required double travel,
  required double height,
  required double curve,
}) {
  if (height <= 0) return 255;
  var t = (travel / height).clamp(0.0, 1.0);
  if (curve != 1.0) t = math.pow(t, curve).toDouble();
  return (t * 255).round().clamp(0, 255);
}
