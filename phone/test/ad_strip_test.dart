/// The ad strip in a build without an ad unit, which is what every build from
/// source is, and what the strip falls back to whenever no ad is available.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/core/ads.dart';
import 'package:phonepad/ui/ad_strip.dart';

Widget _screen({EdgeInsets gesture = EdgeInsets.zero}) => MediaQuery(
      data: MediaQueryData(
        size: const Size(400, 800),
        systemGestureInsets: gesture,
      ),
      child: MaterialApp(
        home: Scaffold(
          bottomNavigationBar: const AdStrip(),
          body: ListView(
            children: [
              for (var i = 0; i < 40; i++)
                SizedBox(height: 60, child: Text('row $i')),
            ],
          ),
        ),
      ),
    );

/// A phone-shaped surface. The default test view is 800x600 (see
/// docs/engineering.md), which would say nothing about a phone.
void _phone(WidgetTester tester) {
  tester.view.physicalSize = const Size(400, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

void main() {
  test('a build without an ad unit never starts the ad SDK', () {
    expect(kAdsConfigured, isFalse);
    expect(Ads.instance.state, AdsState.off);
  });

  testWidgets('without an ad, the strip carries the house placement',
      (tester) async {
    _phone(tester);
    await tester.pumpWidget(_screen());
    expect(find.text('PhonePad is free and open source'), findsOneWidget);
    expect(find.text('Support'), findsOneWidget);
  });

  testWidgets('content ends above the strip and never runs underneath it',
      (tester) async {
    _phone(tester);
    await tester.pumpWidget(_screen());
    final strip = tester.getRect(find.byType(AdStrip));
    final list = tester.getRect(find.byType(ListView));
    expect(list.bottom, lessThanOrEqualTo(strip.top));
    expect(strip.bottom, 800);
  });

  testWidgets('the strip clears the system gesture area', (tester) async {
    _phone(tester);
    await tester.pumpWidget(_screen());
    final plain = tester.getRect(find.text('Support'));

    await tester.pumpWidget(_screen(gesture: const EdgeInsets.only(bottom: 30)));
    await tester.pump();
    final raised = tester.getRect(find.text('Support'));
    expect(plain.center.dy - raised.center.dy, closeTo(30, 0.5));
  });

  testWidgets('a phone in landscape gets no strip at all', (tester) async {
    tester.view.physicalSize = const Size(890, 410);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(
          size: Size(890, 410),
          systemGestureInsets: EdgeInsets.only(bottom: 24),
        ),
        child: MaterialApp(
          home: Scaffold(
            bottomNavigationBar: const AdStrip(),
            body: const SizedBox.expand(),
          ),
        ),
      ),
    );
    expect(find.text('PhonePad is free and open source'), findsNothing);
    expect(tester.getSize(find.byType(AdStrip)).height, 0);
  });
}
