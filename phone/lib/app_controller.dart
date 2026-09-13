/// Application state: storage, layouts, settings and the connection.
///
/// Held above the widget tree and passed down. Note that this notifies for
/// *structural* changes only — connecting, switching layout, editing. Touch
/// input never routes through here; it goes straight from the touch router to
/// the transport, which is what keeps the hot path free of rebuilds.
library;

import 'package:flutter/foundation.dart';

import 'core/demo.dart';
import 'core/net/connection.dart';
import 'core/store.dart';
import 'model/layout.dart';
import 'model/presets.dart';
import 'platform/phonepad_platform.dart';

class AppController extends ChangeNotifier {
  AppController._(this.store, this.connection) {
    connection.addListener(notifyListeners);
    _loadLayouts();
  }

  final Store store;
  final PhonePadConnection connection;

  final List<ControllerLayout> layouts = [];
  DisplayInfo display = const DisplayInfo();
  LinkInfo link = LinkInfo.empty;

  AppSettings get settings => store.data.settings;
  AppData get data => store.data;

  static Future<AppController> create() async {
    final store = await Store.open();

    // Name ourselves after the actual handset, so the PC's device list reads
    // "Galaxy S24 Ultra" rather than "Android phone".
    final name = demoOr(await PhonePadPlatform.deviceName(), kDemoDeviceName);
    if (store.data.deviceName != name) {
      store.data.deviceName = name;
      await store.save();
    }

    final controller = AppController._(store, PhonePadConnection(store));
    await controller.refreshPlatformInfo();
    return controller;
  }

  Future<void> refreshPlatformInfo() async {
    display = await PhonePadPlatform.displayInfo();
    link = await PhonePadPlatform.linkInfo();
    notifyListeners();
  }

  // --- layouts ---------------------------------------------------------------

  void _loadLayouts() {
    layouts
      ..clear()
      ..addAll(builtInLayouts());

    for (final raw in store.data.layouts) {
      try {
        final l = ControllerLayout.fromJson(raw);
        // A saved layout with a built-in id is an edited copy of that preset;
        // it replaces the pristine version.
        layouts.removeWhere((existing) => existing.id == l.id);
        layouts.add(l);
      } catch (e) {
        debugPrint('PhonePad: skipping unreadable layout: $e');
      }
    }
  }

  ControllerLayout get activeLayout {
    final id = settings.activeLayoutId;
    for (final l in layouts) {
      if (l.id == id) return l;
    }
    return layouts.first;
  }

  Future<void> setActiveLayout(String id) async {
    settings.activeLayoutId = id;
    await store.save();
    notifyListeners();
  }

  Future<void> saveLayout(ControllerLayout layout) async {
    // An edited built-in stops being pristine.
    layout.builtIn = false;

    final index = layouts.indexWhere((l) => l.id == layout.id);
    if (index >= 0) {
      layouts[index] = layout;
    } else {
      layouts.add(layout);
    }

    store.data.layouts
      ..removeWhere((raw) => raw['id'] == layout.id)
      ..add(layout.toJson());
    await store.save();
    notifyListeners();
  }

  Future<ControllerLayout> duplicateLayout(
    ControllerLayout source,
    String name,
  ) async {
    final copy = source.copyWith(
      id: 'layout-${DateTime.now().microsecondsSinceEpoch}',
      name: name,
      builtIn: false,
    );
    await saveLayout(copy);
    await setActiveLayout(copy.id);
    return copy;
  }

  Future<void> renameLayout(ControllerLayout layout, String name) async {
    layout.name = name;
    await saveLayout(layout);
  }

  Future<void> deleteLayout(ControllerLayout layout) async {
    layouts.removeWhere((l) => l.id == layout.id);
    store.data.layouts.removeWhere((raw) => raw['id'] == layout.id);

    // Deleting the edited version of a preset restores the original rather
    // than leaving a gap.
    final preset = builtInLayouts().where((b) => b.id == layout.id).firstOrNull;
    if (preset != null) layouts.add(preset);

    if (settings.activeLayoutId == layout.id) {
      settings.activeLayoutId = layouts.isEmpty ? null : layouts.first.id;
    }
    await store.save();
    notifyListeners();
  }

  /// Throw away edits to a preset and reinstate the shipped arrangement.
  Future<void> resetLayout(ControllerLayout layout) async {
    final preset = builtInLayouts().where((b) => b.id == layout.id).firstOrNull;
    if (preset == null) return;
    layouts[layouts.indexWhere((l) => l.id == layout.id)] = preset;
    store.data.layouts.removeWhere((raw) => raw['id'] == layout.id);
    await store.save();
    notifyListeners();
  }

  // --- settings --------------------------------------------------------------

  Future<void> updateSettings(void Function(AppSettings s) change) async {
    change(settings);
    await store.save();
    notifyListeners();
  }

  Future<void> forgetPc(String serverId) async {
    store.data.forgetPc(serverId);
    await store.save();
    notifyListeners();
  }

  // --- connection ------------------------------------------------------------

  /// Discovery, seeded with the subnet broadcast address from the platform
  /// channel because some access points drop 255.255.255.255.
  Future<List<DiscoveredPc>> discover() async {
    link = await PhonePadPlatform.linkInfo();
    final extras = <String>[if (link.broadcast != null) link.broadcast!];
    return connection.discover(extraTargets: extras);
  }

  @override
  void dispose() {
    connection.removeListener(notifyListeners);
    connection.dispose();
    super.dispose();
  }
}
