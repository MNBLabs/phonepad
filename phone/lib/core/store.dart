/// Local persistence: device identity, paired PCs, settings and layouts.
///
/// One JSON file under the app's private documents directory. Writes go to a
/// temporary file and are then renamed, so an interrupted save cannot leave a
/// half-written pairing token behind.
library;

import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import 'protocol/pairing_crypto.dart';

String hex(List<int> b) =>
    b.map((x) => x.toRadixString(16).padLeft(2, '0')).join();

Uint8List? unhex(String s, int expectedLen) {
  if (s.length != expectedLen * 2) return null;
  final out = Uint8List(expectedLen);
  for (var i = 0; i < expectedLen; i++) {
    final v = int.tryParse(s.substring(i * 2, i * 2 + 2), radix: 16);
    if (v == null) return null;
    out[i] = v;
  }
  return out;
}

/// A PC this phone has completed pairing with.
class PairedPc {
  final String serverId; // hex, 16 bytes
  String hostname;
  String token; // hex, 32 bytes
  String? lastAddress;
  int lastInputPort;

  PairedPc({
    required this.serverId,
    required this.hostname,
    required this.token,
    this.lastAddress,
    this.lastInputPort = 47801,
  });

  Uint8List? get tokenBytes => unhex(token, 32);

  Map<String, dynamic> toJson() => {
        'serverId': serverId,
        'hostname': hostname,
        'token': token,
        'lastAddress': lastAddress,
        'lastInputPort': lastInputPort,
      };

  static PairedPc fromJson(Map<String, dynamic> j) => PairedPc(
        serverId: j['serverId'] as String,
        hostname: (j['hostname'] as String?) ?? 'PC',
        token: j['token'] as String,
        lastAddress: j['lastAddress'] as String?,
        lastInputPort: (j['lastInputPort'] as int?) ?? 47801,
      );
}

class AppSettings {
  bool hapticsEnabled = true;
  int hapticStrength = 60; // 0..100
  int sendRateHz = 250;
  String? activeLayoutId;

  /// Whether the first-run introduction has been seen. Persisted rather than
  /// inferred from "has a paired PC": forgetting a PC should not replay the
  /// introduction at someone who has been using the app for months.
  bool onboarded = false;

  Map<String, dynamic> toJson() => {
        'hapticsEnabled': hapticsEnabled,
        'hapticStrength': hapticStrength,
        'sendRateHz': sendRateHz,
        'activeLayoutId': activeLayoutId,
        'onboarded': onboarded,
      };

  static AppSettings fromJson(Map<String, dynamic> j) => AppSettings()
    ..hapticsEnabled = (j['hapticsEnabled'] as bool?) ?? true
    ..hapticStrength = (j['hapticStrength'] as int?) ?? 60
    ..sendRateHz = (j['sendRateHz'] as int?) ?? 250
    ..activeLayoutId = j['activeLayoutId'] as String?
    ..onboarded = (j['onboarded'] as bool?) ?? false;
}

/// Everything the app persists. Layouts are stored as raw JSON maps here and
/// parsed by the layout model, so this file has no dependency on layout shape.
class AppData {
  String deviceId; // hex, 16 bytes
  String deviceName;
  List<PairedPc> pairedPcs;
  AppSettings settings;
  List<Map<String, dynamic>> layouts;

  AppData({
    required this.deviceId,
    required this.deviceName,
    required this.pairedPcs,
    required this.settings,
    required this.layouts,
  });

  Uint8List get deviceIdBytes => unhex(deviceId, 16) ?? Uint8List(16);

  PairedPc? pcById(String serverId) {
    for (final p in pairedPcs) {
      if (p.serverId == serverId) return p;
    }
    return null;
  }

  void upsertPc(PairedPc pc) {
    final existing = pcById(pc.serverId);
    if (existing == null) {
      pairedPcs.add(pc);
    } else {
      existing.hostname = pc.hostname;
      existing.token = pc.token;
      existing.lastAddress = pc.lastAddress ?? existing.lastAddress;
      existing.lastInputPort = pc.lastInputPort;
    }
  }

  void forgetPc(String serverId) =>
      pairedPcs.removeWhere((p) => p.serverId == serverId);

  Map<String, dynamic> toJson() => {
        'version': 1,
        'deviceId': deviceId,
        'deviceName': deviceName,
        'pairedPcs': pairedPcs.map((p) => p.toJson()).toList(),
        'settings': settings.toJson(),
        'layouts': layouts,
      };

  static AppData fresh() => AppData(
        deviceId: hex(randomBytes(16)),
        deviceName: 'Android phone',
        pairedPcs: [],
        settings: AppSettings(),
        layouts: [],
      );

  static AppData fromJson(Map<String, dynamic> j) => AppData(
        deviceId: (j['deviceId'] as String?) ?? hex(randomBytes(16)),
        deviceName: (j['deviceName'] as String?) ?? 'Android phone',
        pairedPcs: ((j['pairedPcs'] as List?) ?? const [])
            .cast<Map<String, dynamic>>()
            .map(PairedPc.fromJson)
            .toList(),
        settings: AppSettings.fromJson(
          (j['settings'] as Map<String, dynamic>?) ?? const {},
        ),
        layouts: ((j['layouts'] as List?) ?? const [])
            .cast<Map<String, dynamic>>()
            .toList(),
      );
}

class Store {
  Store._(this._file, this.data);

  final File _file;
  final AppData data;

  /// Set when an existing file could not be parsed, so the UI can say so
  /// instead of silently presenting a blank slate.
  String? loadWarning;

  static Future<Store> open() async {
    final dir = await getApplicationDocumentsDirectory();
    final file = File('${dir.path}/phonepad.json');
    return openAt(file);
  }

  /// Injectable path, so tests never touch the real app directory.
  static Future<Store> openAt(File file) async {
    if (!await file.exists()) {
      final store = Store._(file, AppData.fresh());
      await store.save();
      return store;
    }
    try {
      final text = await file.readAsString();
      final json = jsonDecode(text) as Map<String, dynamic>;
      return Store._(file, AppData.fromJson(json));
    } catch (e) {
      // Keep the unreadable file rather than overwriting somebody's pairings.
      final backup = File('${file.path}.bad');
      try {
        await file.rename(backup.path);
      } catch (_) {
        /* best effort */
      }
      final store = Store._(file, AppData.fresh())
        ..loadWarning = 'Saved data was unreadable ($e); it was kept at '
            '${backup.path} and a fresh profile was started.';
      await store.save();
      return store;
    }
  }

  Future<void> save() async {
    final tmp = File('${_file.path}.tmp');
    try {
      await tmp.writeAsString(
        const JsonEncoder.withIndent('  ').convert(data.toJson()),
        flush: true,
      );
      await tmp.rename(_file.path);
    } catch (e) {
      debugPrint('PhonePad: could not save settings: $e');
      rethrow;
    }
  }
}
