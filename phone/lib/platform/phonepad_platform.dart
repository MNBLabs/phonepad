/// Thin wrapper over the native method channel.
///
/// Everything here is called at most a few times per second — never on the
/// input hot path. See `docs/ADR-001-architecture.md` for why the packet path
/// deliberately stays in Dart.
library;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// What kind of link a [LocalNetwork] is, as far as the user is concerned.
enum NetworkKind {
  wifi('Wi-Fi'),
  hotspot('Hotspot'),
  usb('USB tethering'),
  bluetooth('Bluetooth tethering'),
  ethernet('Ethernet'),
  other('Local network');

  const NetworkKind(this.label);
  final String label;

  static NetworkKind parse(String? v) =>
      values.firstWhere((k) => k.name == v, orElse: () => NetworkKind.other);
}

/// One local IPv4 link a PC could be on: the phone's own Wi-Fi, or a network
/// the phone is providing (hotspot, USB or Bluetooth tethering).
class LocalNetwork {
  final String interfaceName;
  final String address;
  final int prefixLength;
  final String broadcast;
  final NetworkKind kind;

  const LocalNetwork({
    required this.interfaceName,
    required this.address,
    required this.prefixLength,
    required this.broadcast,
    required this.kind,
  });

  /// Whether [ip] is on this link, so a probe to it stays local.
  bool contains(String ip) {
    final a = _ipv4(address);
    final b = _ipv4(ip);
    if (a == null || b == null || prefixLength < 1 || prefixLength > 32) {
      return false;
    }
    final mask = (0xFFFFFFFF << (32 - prefixLength)) & 0xFFFFFFFF;
    return (a & mask) == (b & mask);
  }

  static int? _ipv4(String s) {
    final parts = s.split('.');
    if (parts.length != 4) return null;
    var v = 0;
    for (final p in parts) {
      final n = int.tryParse(p);
      if (n == null || n < 0 || n > 255) return null;
      v = (v << 8) | n;
    }
    return v;
  }
}

/// Every local link, plus what the phone's default network is. The default
/// matters because 255.255.255.255 only ever leaves on the default network.
class LinkInfo {
  final List<LocalNetwork> networks;

  /// `wifi`, `ethernet`, `cellular`, `vpn`, `other` or `none`.
  final String defaultKind;

  const LinkInfo({this.networks = const [], this.defaultKind = 'none'});

  static const empty = LinkInfo();

  bool get isUsable => networks.isNotEmpty;

  /// Whether the all-ones broadcast reaches a local link rather than the
  /// carrier's network.
  bool get defaultIsLocal => defaultKind == 'wifi' || defaultKind == 'ethernet';
}

class DisplayInfo {
  final double refreshRate;
  final double maxRefreshRate;
  final String model;
  final int androidSdk;

  const DisplayInfo({
    this.refreshRate = 60,
    this.maxRefreshRate = 60,
    this.model = 'Android device',
    this.androidSdk = 0,
  });
}

enum HapticKind { tick, press, release, connect, disconnect }

class PhonePadPlatform {
  static const _channel = MethodChannel('dev.phonepad/platform');

  /// Every call is wrapped: a platform failure must degrade a feature, never
  /// interrupt play.
  static Future<T?> _call<T>(String method, [Map<String, dynamic>? args]) async {
    try {
      return await _channel.invokeMethod<T>(method, args);
    } on MissingPluginException {
      return null; // running in a test or on an unsupported platform
    } catch (e) {
      debugPrint('PhonePad: platform call "$method" failed: $e');
      return null;
    }
  }
  /// Drive the phone's motor from the game's rumble.
  ///
  /// Separate from [haptic] because it is a level, not an event: the game
  /// holds a strength for as long as it wants and the vibration has to hold
  /// with it. Firing one-shot effects at 20 Hz instead would stutter.
  static Future<void> rumble(int large, int small) => _call<void>('rumble', {
        'large': large.clamp(0, 255),
        'small': small.clamp(0, 255),
      });


  static Future<String> deviceName() async =>
      await _call<String>('getDeviceName') ?? 'Android phone';

  static Future<LinkInfo> linkInfo() async {
    final map = await _call<Map<Object?, Object?>>('getLocalNetworks');
    if (map == null) return LinkInfo.empty;
    final networks = <LocalNetwork>[];
    for (final raw in (map['networks'] as List<Object?>?) ?? const []) {
      if (raw is! Map) continue;
      final address = raw['address'] as String?;
      final broadcast = raw['broadcast'] as String?;
      if (address == null || broadcast == null) continue;
      networks.add(LocalNetwork(
        interfaceName: (raw['interface'] as String?) ?? '',
        address: address,
        prefixLength: (raw['prefixLength'] as int?) ?? 0,
        broadcast: broadcast,
        kind: NetworkKind.parse(raw['kind'] as String?),
      ));
    }
    // Wi-Fi first, so the screen leads with the link people expect.
    networks.sort((a, b) => a.kind.index.compareTo(b.kind.index));
    return LinkInfo(
      networks: networks,
      defaultKind: (map['defaultKind'] as String?) ?? 'none',
    );
  }

  static Future<DisplayInfo> displayInfo() async {
    final map = await _call<Map<Object?, Object?>>('getDisplayInfo');
    if (map == null) return const DisplayInfo();
    return DisplayInfo(
      refreshRate: (map['refreshRate'] as num?)?.toDouble() ?? 60,
      maxRefreshRate: (map['maxRefreshRate'] as num?)?.toDouble() ?? 60,
      model: (map['model'] as String?) ?? 'Android device',
      androidSdk: (map['androidSdk'] as int?) ?? 0,
    );
  }

  static Future<void> setKeepAwake(bool on) =>
      _call<void>('setKeepAwake', {'on': on});

  static Future<void> setLowLatencyWifi(bool on) =>
      _call<void>('setLowLatencyWifi', {'on': on});

  static Future<void> setGestureExclusion(bool on) =>
      _call<void>('setGestureExclusion', {'on': on});

  static Future<void> haptic(HapticKind kind, int amplitude) =>
      _call<void>('haptic', {'kind': kind.name, 'amplitude': amplitude});
}
