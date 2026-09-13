/// Thin wrapper over the native method channel.
///
/// Everything here is called at most a few times per second — never on the
/// input hot path. See `docs/ADR-001-architecture.md` for why the packet path
/// deliberately stays in Dart.
library;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

class LinkInfo {
  final String? address;
  final int prefixLength;
  final String? broadcast;
  final String interfaceName;

  const LinkInfo({
    this.address,
    this.prefixLength = 0,
    this.broadcast,
    this.interfaceName = '',
  });

  static const empty = LinkInfo();

  bool get isUsable => address != null;
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
    final map = await _call<Map<Object?, Object?>>('getLinkInfo');
    if (map == null || map.isEmpty) return LinkInfo.empty;
    return LinkInfo(
      address: map['address'] as String?,
      prefixLength: (map['prefixLength'] as int?) ?? 0,
      broadcast: map['broadcast'] as String?,
      interfaceName: (map['interface'] as String?) ?? '',
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
