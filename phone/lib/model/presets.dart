/// Built-in layouts.
///
/// The standard arrangement follows the Xbox convention from the brief: sticks
/// low and outboard where thumbs naturally rest, face buttons upper-right,
/// D-pad lower-left, shoulders and triggers along the top edge.
library;

import 'dart:math' as math;

import '../core/protocol/packets.dart';
import 'layout.dart';

ControlSpec _button(
  String id,
  int mask,
  String label,
  double x,
  double y, {
  double size = 0.115,
}) =>
    ControlSpec(
      id: id,
      type: ControlType.button,
      mapping: ControlMapping(buttons: mask),
      x: x,
      y: y,
      size: size,
      label: label,
    );

ControlSpec _shoulder(String id, int mask, String label, double x, double y) =>
    ControlSpec(
      id: id,
      type: ControlType.button,
      mapping: ControlMapping(buttons: mask),
      x: x,
      y: y,
      size: 0.10,
      aspect: 2.1,
      label: label,
      shape: ControlShape.pill,
    );

/// Triggers are analogue by default and taller than they are wide: the slide
/// distance *is* the resolution, so a squat trigger cannot be feathered. A
/// racing game with digital throttle and brake is unplayable, and that is what
/// a full-press-on-touch trigger amounts to.
ControlSpec _trigger(String id, StickSide side, String label, double x, double y) =>
    ControlSpec(
      id: id,
      type: ControlType.trigger,
      mapping: ControlMapping(trigger: side),
      x: x,
      y: y,
      size: 0.185,
      aspect: 0.66,
      label: label,
      shape: ControlShape.roundedRect,
      analogSlide: true,
      responseCurve: 1.0,
    );

ControlSpec _stick(String id, StickSide side, double x, double y) => ControlSpec(
      id: id,
      type: ControlType.stick,
      mapping: ControlMapping(stick: side),
      x: x,
      y: y,
      size: 0.34,
      label: '',
      responseCurve: kStickResponseCurve,
      opacity: 0.7,
    );

ControlSpec _dpad(String id, double x, double y) => ControlSpec(
      id: id,
      type: ControlType.dpad,
      mapping: const ControlMapping(),
      x: x,
      y: y,
      size: 0.235,
      label: '',
      deadzone: 0.28,
      opacity: 0.75,
    );

ControlSpec _small(String id, int mask, String label, double x, double y) =>
    ControlSpec(
      id: id,
      type: ControlType.button,
      mapping: ControlMapping(buttons: mask),
      x: x,
      y: y,
      size: 0.062,
      label: label,
      opacity: 0.6,
    );

List<ControlSpec> _standardLandscape() => [
      // Shoulders and triggers ride the top edge, triggers outboard of the
      // bumpers so an index finger reaches the trigger without crossing the
      // bumper. Kept clear of the very edge: a control flush against the
      // boundary cannot be slid outward, and an analogue trigger is slid.
      _trigger('lt', StickSide.left, 'LT', 0.085, 0.20),
      _trigger('rt', StickSide.right, 'RT', 0.915, 0.20),
      _shoulder('lb', Btn.lb, 'LB', 0.225, 0.10),
      _shoulder('rb', Btn.rb, 'RB', 0.775, 0.10),

      _stick('lstick', StickSide.left, 0.175, 0.68),
      _stick('rstick', StickSide.right, 0.825, 0.68),

      _dpad('dpad', 0.375, 0.72),

      _button('y', Btn.y, 'Y', 0.625, 0.44),
      _button('x', Btn.x, 'X', 0.545, 0.62),
      _button('b', Btn.b, 'B', 0.705, 0.62),
      _button('a', Btn.a, 'A', 0.625, 0.80),

      // Stick clicks get their own targets. Pressing the stick itself would
      // fight with aiming, which is exactly when you least want a stray L3.
      _small('ls', Btn.ls, 'L3', 0.055, 0.44),
      _small('rs', Btn.rs, 'R3', 0.945, 0.44),

      // The status readout sits centred on the top edge, so these go below it.
      _small('view', Btn.back, 'VIEW', 0.445, 0.24),
      _small('menu', Btn.start, 'MENU', 0.555, 0.24),
      _small('guide', Btn.guide, 'XBOX', 0.5, 0.10),
    ];

List<ControlSpec> _standardPortrait() => [
      _trigger('lt', StickSide.left, 'LT', 0.115, 0.44),
      _trigger('rt', StickSide.right, 'RT', 0.885, 0.44),
      _shoulder('lb', Btn.lb, 'LB', 0.285, 0.355),
      _shoulder('rb', Btn.rb, 'RB', 0.715, 0.355),

      _stick('lstick', StickSide.left, 0.26, 0.85)..size = 0.30,
      _stick('rstick', StickSide.right, 0.74, 0.85)..size = 0.30,

      _dpad('dpad', 0.26, 0.625)..size = 0.22,

      _button('y', Btn.y, 'Y', 0.74, 0.545, size: 0.105),
      _button('x', Btn.x, 'X', 0.645, 0.625, size: 0.105),
      _button('b', Btn.b, 'B', 0.835, 0.625, size: 0.105),
      _button('a', Btn.a, 'A', 0.74, 0.705, size: 0.105),

      _small('ls', Btn.ls, 'L3', 0.095, 0.755),
      _small('rs', Btn.rs, 'R3', 0.905, 0.755),

      _small('view', Btn.back, 'VIEW', 0.40, 0.475),
      _small('menu', Btn.start, 'MENU', 0.60, 0.475),
      _small('guide', Btn.guide, 'XBOX', 0.5, 0.27),
    ];

/// The default. Every other preset is a transformation of this one, so fixing
/// the ergonomics here fixes them everywhere.
ControllerLayout standardLayout() => ControllerLayout(
      id: 'builtin-standard',
      name: 'Standard',
      builtIn: true,
      landscape: _standardLandscape(),
      portrait: _standardPortrait(),
    );

/// Steering wants resolution near centre far more than it wants a fast whip to
/// full lock, and it wants throttle and brake it can feather. The D-pad is
/// hidden because nothing in a racing game uses it and it only crowds the thumb.
ControllerLayout racingLayout() {
  final l = standardLayout().copyWith(id: 'builtin-racing', name: 'Racing');
  for (final list in [l.landscape, l.portrait]) {
    for (final c in list) {
      switch (c.type) {
        case ControlType.stick:
          if (c.mapping.stick == StickSide.left) {
            c.size *= 1.20;
            c.responseCurve = 1.55;
          }
        case ControlType.trigger:
          c.size *= 1.15;
          c.analogSlide = true;
          // A taller trigger needs more room above it to be feathered into.
          // Growing one without moving it down turns it back into a button.
          c.y = math.max(c.y, c.size + 0.02);
        case ControlType.dpad:
          c.visible = false;
        case ControlType.button:
          break;
      }
    }
  }
  return l;
}

/// Aiming is the opposite trade: a smaller, quicker right stick so a flick
/// crosses the ring, with the curve kept mild so small corrections survive.
ControllerLayout precisionLayout() {
  final l = standardLayout().copyWith(
    id: 'builtin-precision',
    name: 'Precision',
  );
  for (final list in [l.landscape, l.portrait]) {
    for (final c in list) {
      if (c.type == ControlType.stick && c.mapping.stick == StickSide.right) {
        c.size *= 0.88;
        c.sensitivity = 1.25;
        c.responseCurve = 1.20;
      }
    }
  }
  return l;
}

/// Sticks swapped with the D-pad/face cluster, for left-handed stick use.
ControllerLayout southpawLayout() {
  final l = standardLayout().copyWith(id: 'builtin-southpaw', name: 'Southpaw');
  for (final list in [l.landscape, l.portrait]) {
    final left = list.firstWhere((c) => c.id == 'lstick');
    final dpad = list.firstWhere((c) => c.id == 'dpad');
    final lx = left.x, ly = left.y;
    left.x = dpad.x;
    left.y = dpad.y;
    dpad.x = lx;
    dpad.y = ly;
  }
  return l;
}

List<ControllerLayout> builtInLayouts() => [
      standardLayout(),
      racingLayout(),
      precisionLayout(),
      southpawLayout(),
    ];
