/// Input maths and touch routing.
///
/// These are the parts where a bug shows up as "the stick feels wrong" or "a
/// button stuck down" rather than as a crash, so they are worth pinning down
/// precisely.
library;

import 'dart:math' as math;

import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/core/protocol/packets.dart';
import 'package:phonepad/model/layout.dart';
import 'package:phonepad/model/presets.dart';
import 'package:phonepad/ui/touch_router.dart';

void main() {
  group('stick maths', () {
    StickValue at(
      double dx,
      double dy, {
      double deadzone = 0.0,
      double antiDeadzone = 0.0,
      double saturation = 1.0,
      double curve = 1.0,
      double sensitivity = 1.0,
      bool circular = true,
    }) =>
        computeStick(
          dx: dx,
          dy: dy,
          radius: 100,
          deadzone: deadzone,
          antiDeadzone: antiDeadzone,
          saturation: saturation,
          sensitivity: sensitivity,
          circular: circular,
          curve: curve,
        );

    test('centre is exactly zero', () {
      expect(at(0, 0), const StickValue(0, 0));
    });

    test('centre stays exactly zero even with anti-deadzone applied', () {
      // The single most damaging way to get anti-deadzone wrong: apply the
      // floor unconditionally and every game reads the pad as permanently
      // deflected, with no way for the user to centre it.
      final v = at(0, 0, antiDeadzone: 0.35);
      expect(v.x, 0);
      expect(v.y, 0);
    });

    test('movement below the noise floor is zero', () {
      expect(at(1, 0, deadzone: 0.02).x, 0);
      expect(at(0, 1.5, deadzone: 0.02).y, 0);
    });

    test('anti-deadzone lifts the first real movement clear of the floor', () {
      // The whole point: leaving centre must produce something a game with its
      // own inner deadzone will actually act on, rather than nothing at all.
      final v = at(3, 0, deadzone: 0.02, antiDeadzone: 0.15);
      expect(v.x / 32767, closeTo(0.15, 0.02));
    });

    test('without anti-deadzone the response still starts from zero', () {
      final v = at(11, 0, deadzone: 0.1);
      expect(v.x, greaterThan(0));
      expect(v.x, lessThan(1000));
    });

    test('anti-deadzone compresses the range but never exceeds it', () {
      for (final d in [10.0, 40.0, 70.0, 100.0]) {
        final v = at(d, 0, antiDeadzone: 0.2);
        expect(v.x, greaterThan(0));
        expect(v.x, lessThanOrEqualTo(32767));
      }
      expect(at(100, 0, antiDeadzone: 0.2).x, 32767);
    });

    test('the response climbs without a cliff once it has left centre', () {
      // A step between neighbouring positions is what reads as "nothing, then
      // suddenly too much".
      //
      // The one deliberate step is the anti-deadzone lift at the noise floor:
      // that jump is sized to land just past the *game's* inner deadzone, so
      // what the player sees on screen starts from zero. Everything after it
      // must climb evenly, which is what this pins down.
      var prev = at(3, 0, deadzone: 0.02, antiDeadzone: 0.15, saturation: 0.92)
              .x /
          32767;
      for (var d = 4; d <= 100; d++) {
        final now = at(d.toDouble(), 0,
                deadzone: 0.02, antiDeadzone: 0.15, saturation: 0.92)
            .x /
            32767;
        expect(now, greaterThanOrEqualTo(prev - 0.001),
            reason: 'response went backwards at ${d}px');
        expect(now - prev, lessThan(0.05), reason: 'cliff at ${d}px');
        prev = now;
      }
    });

    test('saturation reaches full scale before the edge of the ring', () {
      expect(at(92, 0, saturation: 0.92).x, 32767);
      expect(at(100, 0, saturation: 0.92).x, 32767);
      expect(at(80, 0, saturation: 0.92).x, lessThan(32767));
    });

    test('full deflection reaches the maximum, never int16 minimum', () {
      expect(at(100, 0).x, 32767);
      expect(at(-100, 0).x, -32767);
      expect(at(-1000, 0).x, -32767, reason: 'must clamp, and never reach -32768');
    });

    test('screen Y is inverted to XInput Y', () {
      // Dragging down the screen must push the stick down, i.e. negative Y.
      expect(at(0, 100).y, lessThan(0));
      expect(at(0, -100).y, greaterThan(0));
    });

    test('circular range keeps diagonals inside the unit circle', () {
      final v = at(100, 100);
      final mag = (v.x / 32767) * (v.x / 32767) + (v.y / 32767) * (v.y / 32767);
      expect(mag, lessThanOrEqualTo(1.02), reason: 'diagonal exceeded full range');
    });

    test('square range allows full deflection on both axes at once', () {
      final v = at(100, 100, deadzone: 0.1, circular: false);
      expect(v.x, 32767);
      expect(v.y, -32767);
    });

    test('anti-deadzone preserves direction exactly', () {
      // The floor is applied to magnitude, so a 3-4-5 offset must stay on that
      // diagonal rather than being pushed toward an axis.
      final v = at(30, 40, antiDeadzone: 0.25);
      expect(v.x / -v.y, closeTo(30 / 40, 0.01));
    });

    test('a curve above 1 softens the middle but keeps the extremes', () {
      final linear = at(50, 0).x;
      final curved = at(50, 0, curve: 2.0).x;
      expect(curved, lessThan(linear));
      expect(at(100, 0, curve: 2.0).x, 32767);
    });

    test('sensitivity scales but still clamps', () {
      expect(at(60, 0, sensitivity: 3.0).x, 32767);
    });

    test('a saturation at or below the noise floor does not divide by zero', () {
      final v = at(50, 0, deadzone: 0.3, saturation: 0.3);
      expect(v.x, 32767);
    });
  });

  group('d-pad', () {
    int at(double dx, double dy) =>
        computeDpad(dx: dx, dy: dy, radius: 100, deadzone: 0.2);

    test('centre presses nothing', () => expect(at(0, 0), 0));

    test('cardinal directions', () {
      expect(at(80, 0), Btn.dpadRight);
      expect(at(-80, 0), Btn.dpadLeft);
      expect(at(0, -80), Btn.dpadUp, reason: 'up the screen is dpad up');
      expect(at(0, 80), Btn.dpadDown);
    });

    test('diagonals press two directions', () {
      expect(at(60, -60), Btn.dpadRight | Btn.dpadUp);
      expect(at(-60, 60), Btn.dpadLeft | Btn.dpadDown);
    });

    test('never presses opposing directions at once', () {
      for (var angle = 0; angle < 360; angle += 7) {
        final r = angle * math.pi / 180;
        final bits = at(80 * math.cos(r), 80 * math.sin(r));
        expect(bits & Btn.dpadLeft != 0 && bits & Btn.dpadRight != 0, isFalse);
        expect(bits & Btn.dpadUp != 0 && bits & Btn.dpadDown != 0, isFalse);
      }
    });
  });

  group('trigger', () {
    test('no travel is zero, full travel is 255', () {
      expect(computeTrigger(travel: 0, height: 100, curve: 1), 0);
      expect(computeTrigger(travel: 100, height: 100, curve: 1), 255);
      expect(computeTrigger(travel: 500, height: 100, curve: 1), 255);
      expect(computeTrigger(travel: -50, height: 100, curve: 1), 0);
    });
  });

  group('touch routing', () {
    late ControllerState state;
    late TouchRouter router;
    var changes = 0;

    setUp(() {
      state = ControllerState();
      changes = 0;
      router = TouchRouter(state: state, onStateChanged: () => changes++);
      router.updateGeometry(
        resolveGeometry(standardLayout().landscape, 1000, 500),
      );
    });

    Offset centreOf(String id) {
      final g = router.geometry.firstWhere((g) => g.spec.id == id);
      return Offset(g.cx, g.cy);
    }

    test('pressing a button sets exactly its bit', () {
      router.onPointerDown(1, centreOf('a'));
      expect(state.buttons, Btn.a);
      router.onPointerUp(1);
      expect(state.buttons, 0);
      expect(changes, 2);
    });

    test('four simultaneous presses all register', () {
      router.onPointerDown(1, centreOf('a'));
      router.onPointerDown(2, centreOf('b'));
      router.onPointerDown(3, centreOf('lb'));
      router.onPointerDown(4, centreOf('rb'));
      expect(state.buttons, Btn.a | Btn.b | Btn.lb | Btn.rb);

      router.onPointerUp(2);
      expect(state.buttons, Btn.a | Btn.lb | Btn.rb);
      expect(state.buttons & Btn.b, 0);
    });

    test('two fingers on one button: releasing one keeps it pressed', () {
      // The classic stuck/unstuck button bug. State is rebuilt from live
      // pointers, so this is handled by construction.
      final c = centreOf('a');
      router.onPointerDown(1, c);
      router.onPointerDown(2, c + const Offset(3, 3));
      expect(state.buttons, Btn.a);
      router.onPointerUp(1);
      expect(state.buttons, Btn.a, reason: 'one finger is still on it');
      router.onPointerUp(2);
      expect(state.buttons, 0);
    });

    test('a stick keeps its pointer after the finger leaves the ring', () {
      final c = centreOf('lstick');
      router.onPointerDown(1, c);
      router.onPointerMove(1, c + const Offset(600, 0));
      expect(state.lx, 32767, reason: 'pointer capture should hold the stick');
      router.onPointerUp(1);
      expect(state.lx, 0);
    });

    test('sticks and buttons do not interfere', () {
      router.onPointerDown(1, centreOf('lstick'));
      router.onPointerMove(1, centreOf('lstick') + const Offset(50, 0));
      router.onPointerDown(2, centreOf('a'));
      expect(state.lx, greaterThan(0));
      expect(state.buttons, Btn.a);
      // Releasing the button must not disturb the stick.
      router.onPointerUp(2);
      expect(state.buttons, 0);
      expect(state.lx, greaterThan(0));
    });

    test('left and right sticks are independent', () {
      router.onPointerDown(1, centreOf('lstick'));
      router.onPointerMove(1, centreOf('lstick') + const Offset(60, 0));
      router.onPointerDown(2, centreOf('rstick'));
      router.onPointerMove(2, centreOf('rstick') - const Offset(0, 60));
      expect(state.lx, greaterThan(0));
      expect(state.ly, 0);
      expect(state.rx, 0);
      expect(state.ry, greaterThan(0));
    });

    test('a touch on empty space does nothing', () {
      router.onPointerDown(1, const Offset(500, 250));
      expect(state.isNeutral, isTrue);
    });

    test('releaseAll clears everything', () {
      router.onPointerDown(1, centreOf('a'));
      router.onPointerDown(2, centreOf('lstick'));
      router.onPointerMove(2, centreOf('lstick') + const Offset(50, 20));
      expect(state.isNeutral, isFalse);

      router.releaseAll();
      expect(state.isNeutral, isTrue);
      expect(router.hasActiveTouches, isFalse);
    });

    test('a dropped link does not forget fingers that are still down', () {
      // The connection flapping used to call releaseAll(), which clears the
      // pointer map. The thumbs are still on the glass at that point, so they
      // went dead until they were lifted and put back — which reads exactly
      // like multi-touch breaking, and on a flapping link happens repeatedly.
      final g = router.geometry.firstWhere((g) => g.spec.id == 'lstick');
      router.onPointerDown(1, Offset(g.cx, g.cy));
      router.onPointerMove(1, Offset(g.cx + g.radius * 0.6, g.cy));
      router.onPointerDown(2, centreOf('a'));
      expect(state.lx, greaterThan(0));
      expect(state.buttons, Btn.a);

      // What the screen does on a drop: clear what is being reported, without
      // touching the router's idea of which fingers exist.
      state.reset();
      expect(state.lx, 0);

      // ...and on reconnect the held fingers take effect again on their own.
      router.resync();
      expect(state.lx, greaterThan(0));
      expect(state.buttons, Btn.a);
    });

    test('changing geometry releases held controls', () {
      router.onPointerDown(1, centreOf('a'));
      expect(state.buttons, Btn.a);
      router.updateGeometry(
        resolveGeometry(standardLayout().portrait, 500, 1000),
      );
      expect(state.isNeutral, isTrue, reason: 'stale geometry must not hold a button');
    });

    test('a trigger tap is a full press', () {
      // Analogue triggers measure from wherever the finger landed, so a plain
      // tap has to be full throttle. Measuring from the top edge instead would
      // make a tap in the middle of the control half throttle.
      router.onPointerDown(1, centreOf('lt'));
      expect(state.lt, 255);
      expect(state.rt, 0);
      router.onPointerUp(1);
      expect(state.lt, 0);
    });

    test('sliding up a trigger feathers it back toward zero', () {
      final g = router.geometry.firstWhere((g) => g.spec.id == 'rt');
      final down = Offset(g.cx, g.cy);
      router.onPointerDown(1, down);
      expect(state.rt, 255);

      router.onPointerMove(1, down.translate(0, -g.height / 2));
      expect(state.rt, greaterThan(0));
      expect(state.rt, lessThan(255));

      router.onPointerMove(1, down.translate(0, -g.height));
      expect(state.rt, 0);

      // Pushing back down returns to full, and past it stays clamped.
      router.onPointerMove(1, down.translate(0, g.height));
      expect(state.rt, 255);
    });

    test('a floating stick starts from wherever the finger landed', () {
      // Touching a stick off-centre must not deflect it. Anchored sticks turn
      // the offset between the drawn centre and the thumb into instant
      // deflection, which is why they never behave the same way twice.
      final g = router.geometry.firstWhere((g) => g.spec.id == 'lstick');
      expect(g.spec.floating, isTrue, reason: 'sticks must float by default');

      router.onPointerDown(1, Offset(g.cx + g.radius * 0.7, g.cy));
      expect(state.lx, 0);
      expect(state.ly, 0);
    });

    test('a drifting stick follows a finger pushed past the ring', () {
      final g = router.geometry.firstWhere((g) => g.spec.id == 'lstick');
      expect(g.spec.drift, isTrue);
      final down = Offset(g.cx, g.cy);

      router.onPointerDown(1, down);
      router.onPointerMove(1, down.translate(g.radius * 3, 0));
      expect(state.lx, 32767);

      // Easing back by less than the overshoot must register immediately,
      // rather than retracing the whole overshoot first.
      router.onPointerMove(1, down.translate(g.radius * 2.6, 0));
      expect(state.lx, lessThan(32767));
      expect(state.lx, greaterThan(0));
    });

    test('changing geometry notifies, even with no fingers down', () {
      // Regression: rotation with an idle screen used to change the geometry
      // without firing the listenable, so the painter kept drawing the previous
      // orientation's frame and the controller looked broken after a rotate.
      var notifications = 0;
      router.addListener(() => notifications++);

      router.updateGeometry(
        resolveGeometry(standardLayout().portrait, 500, 1000),
      );
      expect(notifications, 1, reason: 'idle geometry change must repaint');

      router.updateGeometry(
        resolveGeometry(standardLayout().landscape, 1000, 500),
      );
      expect(notifications, 2);
    });

    test('an unknown pointer up is harmless', () {
      router.onPointerUp(99);
      router.onPointerMove(99, Offset.zero);
      expect(state.isNeutral, isTrue);
    });
  });

  group('layout model', () {
    test('every preset round-trips through JSON', () {
      for (final preset in builtInLayouts()) {
        final back = ControllerLayout.fromJson(preset.toJson());
        expect(back.id, preset.id);
        expect(back.name, preset.name);
        expect(back.landscape.length, preset.landscape.length);
        expect(back.portrait.length, preset.portrait.length);
        for (var i = 0; i < preset.landscape.length; i++) {
          final a = preset.landscape[i];
          final b = back.landscape[i];
          expect(b.id, a.id);
          expect(b.type, a.type);
          expect(b.x, a.x);
          expect(b.size, a.size);
          expect(b.mapping.buttons, a.mapping.buttons);
          expect(b.mapping.stick, a.mapping.stick);
        }
      }
    });

    test('presets cover every control the brief requires', () {
      const required = {
        'A': Btn.a, 'B': Btn.b, 'X': Btn.x, 'Y': Btn.y,
        'LB': Btn.lb, 'RB': Btn.rb, 'LS': Btn.ls, 'RS': Btn.rs,
        'Start': Btn.start, 'Back': Btn.back,
      };

      for (final preset in builtInLayouts()) {
        for (final orientation in [preset.landscape, preset.portrait]) {
          final bits = orientation.fold<int>(
            0,
            (acc, s) => acc | s.mapping.buttons,
          );
          required.forEach((name, mask) {
            expect(bits & mask, mask,
                reason: '${preset.name} is missing $name');
          });

          final sticks = orientation
              .where((s) => s.type == ControlType.stick)
              .map((s) => s.mapping.stick)
              .toSet();
          expect(sticks, {StickSide.left, StickSide.right},
              reason: '${preset.name} is missing a stick');

          final triggers = orientation
              .where((s) => s.type == ControlType.trigger)
              .map((s) => s.mapping.trigger)
              .toSet();
          expect(triggers, {StickSide.left, StickSide.right},
              reason: '${preset.name} is missing a trigger');

          expect(
            orientation.any((s) => s.type == ControlType.dpad),
            isTrue,
            reason: '${preset.name} is missing the d-pad',
          );
        }
      }
    });

    test('no control overflows the screen on any preset', () {
      // Centres inside 0..1 is not enough: a control is only usable if its
      // whole body is on screen, and a control flush against the edge cannot
      // be slid outward. Checked against the real S24 Ultra viewport.
      const cases = [(832.0, 384.0, true), (384.0, 832.0, false)];
      for (final preset in builtInLayouts()) {
        for (final (w, h, landscape) in cases) {
          final specs = preset.forOrientation(isLandscape: landscape);
          for (final g in resolveGeometry(specs, w, h)) {
            final id = '${preset.name}/${g.spec.id}';
            expect(g.cx - g.width / 2, greaterThanOrEqualTo(0.0), reason: '$id left');
            expect(g.cx + g.width / 2, lessThanOrEqualTo(w), reason: '$id right');
            expect(g.cy - g.height / 2, greaterThanOrEqualTo(0.0), reason: '$id top');
            expect(g.cy + g.height / 2, lessThanOrEqualTo(h), reason: '$id bottom');
          }
        }
      }
    });

    test('an analogue trigger has room on screen for its own slide', () {
      // Feathering a trigger means sliding up by its full height. A trigger
      // placed so that travel runs off the top of the screen can be pressed
      // but never modulated, which silently makes it a digital button again.
      for (final preset in builtInLayouts()) {
        for (final (w, h, landscape) in const [(832.0, 384.0, true), (384.0, 832.0, false)]) {
          final specs = preset.forOrientation(isLandscape: landscape);
          for (final g in resolveGeometry(specs, w, h)) {
            if (g.spec.type != ControlType.trigger || !g.spec.analogSlide) continue;
            expect(g.cy - g.height, greaterThanOrEqualTo(0.0),
                reason: '${preset.name}/${g.spec.id} cannot be feathered');
          }
        }
      }
    });

    test('geometry scales with the viewport but keeps control size stable', () {
      final specs = standardLayout().landscape;
      final wide = resolveGeometry(specs, 2000, 900);
      final narrow = resolveGeometry(specs, 1000, 900);

      final wideA = wide.firstWhere((g) => g.spec.id == 'a');
      final narrowA = narrow.firstWhere((g) => g.spec.id == 'a');

      // Same shorter edge, so the button is the same physical size...
      expect(wideA.width, closeTo(narrowA.width, 0.001));
      // ...but spread further apart on the wider screen.
      expect(wideA.cx, greaterThan(narrowA.cx));
    });

    test('a layout saved before the new tuning fields decodes to the defaults', () {
      // Layouts on disk predate antiDeadzone/saturation/drift. They must decode
      // to the current defaults rather than to the old behaviour: the old
      // values are precisely what this release exists to correct, so silently
      // preserving them would leave upgraders on the broken response.
      final legacy = <String, dynamic>{
        'id': 'stick',
        'type': 'stick',
        'mapping': {'stick': 'left'},
        'x': 0.2,
        'y': 0.7,
        'size': 0.34,
        'deadzone': 0.12,
        'sensitivity': 1.0,
        'circularRange': true,
        'floating': false,
      };
      final spec = ControlSpec.fromJson(legacy);
      expect(spec.antiDeadzone, kStickAntiDeadzone);
      expect(spec.saturation, kStickSaturation);
      expect(spec.drift, isTrue);
      // Fields the file did carry are still honoured.
      expect(spec.deadzone, 0.12);
      expect(spec.floating, isFalse);
    });

    test('hidden controls are not laid out', () {
      final specs = standardLayout().landscape;
      specs.firstWhere((s) => s.id == 'a').visible = false;
      final geometry = resolveGeometry(specs, 1000, 500);
      expect(geometry.any((g) => g.spec.id == 'a'), isFalse);
    });
  });
}
