/// Where discovery probes go (issue #2).
///
/// The fixtures have the shape of what an S24 Ultra reported while sharing its
/// mobile data over USB with Wi-Fi off: the default network is the carrier's,
/// which hands out a private address, and the PC is on the tethering link.
/// The addresses themselves are neutral stand-ins.
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/core/net/connection.dart';
import 'package:phonepad/platform/phonepad_platform.dart';

const _usb = LocalNetwork(
  interfaceName: 'rndis0',
  address: '172.20.4.1',
  prefixLength: 24,
  broadcast: '172.20.4.255',
  kind: NetworkKind.usb,
);

const _hotspot = LocalNetwork(
  interfaceName: 'swlan0',
  address: '172.21.8.1',
  prefixLength: 24,
  broadcast: '172.21.8.255',
  kind: NetworkKind.hotspot,
);

const _wifi = LocalNetwork(
  interfaceName: 'wlan0',
  address: '172.16.0.3',
  prefixLength: 24,
  broadcast: '172.16.0.255',
  kind: NetworkKind.wifi,
);

void main() {
  test('tethering with mobile data as the default reaches the tethered link',
      () {
    const link = LinkInfo(networks: [_usb], defaultKind: 'cellular');
    final targets = discoveryTargets(link, const []);

    expect(targets, contains('172.20.4.255'));
    // The all-ones broadcast would leave over the carrier's network.
    expect(targets, isNot(contains('255.255.255.255')));
  });

  test('Wi-Fi plus a hotspot probes both links', () {
    const link = LinkInfo(networks: [_wifi, _hotspot], defaultKind: 'wifi');
    expect(
      discoveryTargets(link, const []),
      {'172.16.0.255', '172.21.8.255', '255.255.255.255'},
    );
  });

  test('a remembered PC is probed only when it is on a current link', () {
    const link = LinkInfo(networks: [_usb], defaultKind: 'cellular');
    final targets = discoveryTargets(link, const ['172.20.4.18', '172.16.0.4']);

    expect(targets, contains('172.20.4.18'));
    // Last seen on the home network; from here it would go to the carrier.
    expect(targets, isNot(contains('172.16.0.4')));
  });

  test('no local link means no probes at all', () {
    const link = LinkInfo(defaultKind: 'cellular');
    expect(discoveryTargets(link, const ['172.16.0.4']), isEmpty);
  });

  group('LocalNetwork.contains', () {
    test('matches on the prefix', () {
      expect(_usb.contains('172.20.4.18'), isTrue);
      expect(_usb.contains('172.20.5.18'), isFalse);
      expect(_hotspot.contains('172.21.8.200'), isTrue);
    });

    test('rejects malformed input rather than guessing', () {
      expect(_usb.contains('not an address'), isFalse);
      expect(_usb.contains('172.20.4'), isFalse);
      expect(_usb.contains('172.20.4.300'), isFalse);
    });
  });
}
