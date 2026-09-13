/// Multi-touch through the real widget tree.
///
/// The router's own tests call its methods directly, which proves the state
/// machine but not that Flutter actually delivers a second pointer to it. This
/// drives genuine gestures through [ControllerSurface] instead, so hit testing,
/// pointer routing and cancellation are all exercised — the layer where a
/// second finger would silently kill the first.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/core/protocol/packets.dart';
import 'package:phonepad/model/layout.dart';
import 'package:phonepad/model/presets.dart';
import 'package:phonepad/ui/controller_surface.dart';
import 'package:phonepad/ui/touch_router.dart';

void main() {
  late ControllerState state;
  late TouchRouter router;
  const size = Size(1000, 500);

  Future<void> pump(WidgetTester tester) async {
    // The default test surface is 800x600, which would put the right-hand
    // controls off-screen — they would then never be hit-tested, and the test
    // would report a multi-touch failure that is really a viewport that is too
    // small.
    await tester.binding.setSurfaceSize(size);
    addTearDown(() => tester.binding.setSurfaceSize(null));

    state = ControllerState();
    router = TouchRouter(state: state);
    router.updateGeometry(
      resolveGeometry(standardLayout().landscape, size.width, size.height),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: size.width,
            height: size.height,
            child: ControllerSurface(router: router),
          ),
        ),
      ),
    );
  }

  Offset centreOf(String id) {
    final g = router.geometry.firstWhere((g) => g.spec.id == id);
    return Offset(g.cx, g.cy);
  }

  double radiusOf(String id) =>
      router.geometry.firstWhere((g) => g.spec.id == id).radius;

  testWidgets('a second finger does not cancel the first', (tester) async {
    await pump(tester);

    final left = await tester.startGesture(centreOf('lstick'));
    await tester.pump();
    await left.moveBy(Offset(radiusOf('lstick') * 0.6, 0));
    await tester.pump();
    expect(state.lx, greaterThan(0), reason: 'left stick should be deflected');
    final held = state.lx;

    final button = await tester.startGesture(centreOf('a'));
    await tester.pump();
    expect(state.buttons & Btn.a, Btn.a, reason: 'A should register');
    expect(state.lx, held,
        reason: 'the first finger must survive the second arriving');

    await button.up();
    await tester.pump();
    expect(state.buttons, 0);
    expect(state.lx, held, reason: 'releasing A must not disturb the stick');

    await left.up();
    await tester.pump();
    expect(state.lx, 0);
  });

  testWidgets('both sticks move at once', (tester) async {
    await pump(tester);

    final l = await tester.startGesture(centreOf('lstick'));
    final r = await tester.startGesture(centreOf('rstick'));
    await tester.pump();

    await l.moveBy(Offset(radiusOf('lstick') * 0.5, 0));
    await r.moveBy(Offset(0, -radiusOf('rstick') * 0.5));
    await tester.pump();

    expect(state.lx, greaterThan(0), reason: 'lx after left move');
    expect(state.ry, greaterThan(0), reason: 'ry: screen up is XInput +Y');

    await l.up();
    await tester.pump();
    expect(state.lx, 0);
    expect(state.ry, greaterThan(0), reason: 'right stick outlives the left');

    await r.up();
    await tester.pump();
    expect(state.ry, 0);
  });

  testWidgets('four fingers register together', (tester) async {
    await pump(tester);

    final gestures = <TestGesture>[];
    for (final id in ['a', 'b', 'lb', 'rb']) {
      gestures.add(await tester.startGesture(centreOf(id)));
    }
    await tester.pump();
    expect(state.buttons, Btn.a | Btn.b | Btn.lb | Btn.rb);

    await gestures[1].up();
    await tester.pump();
    expect(state.buttons, Btn.a | Btn.lb | Btn.rb);

    for (final g in gestures.skip(2)) {
      await g.up();
    }
    await gestures[0].up();
    await tester.pump();
    expect(state.buttons, 0);
  });

  testWidgets('a cancelled pointer releases only its own control',
      (tester) async {
    // Android cancels a pointer when the system claims the gesture. That must
    // drop the control it was on and nothing else.
    await pump(tester);

    final left = await tester.startGesture(centreOf('lstick'));
    await left.moveBy(Offset(radiusOf('lstick') * 0.6, 0));
    final button = await tester.startGesture(centreOf('a'));
    await tester.pump();

    await button.cancel();
    await tester.pump();
    expect(state.buttons, 0);
    expect(state.lx, greaterThan(0),
        reason: 'cancelling one pointer must not release the other');

    await left.up();
    await tester.pump();
    expect(state.lx, 0);
  });
}
