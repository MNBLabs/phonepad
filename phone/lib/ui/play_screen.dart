/// The controller itself.
///
/// The whole point of this screen's structure: touch events go
/// `Listener → TouchRouter → ControllerState → socket`, and the only thing that
/// repaints is one `CustomPaint` inside a `RepaintBoundary`. There is no
/// `setState` anywhere on that path, so pressing a button costs a repaint of a
/// single layer — no build, no layout, no diff.
///
/// A raw [Listener] is used rather than any gesture detector on purpose: the
/// gesture arena introduces disambiguation delay and would let controls steal
/// each other's pointers.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app_controller.dart';
import '../core/net/connection.dart';
import '../core/protocol/packets.dart';
import '../model/layout.dart';
import '../platform/phonepad_platform.dart';
import 'controller_surface.dart';
import 'diagnostics_screen.dart';
import 'theme.dart';
import 'touch_router.dart';

class PlayScreen extends StatefulWidget {
  const PlayScreen({super.key, required this.controller});

  final AppController controller;

  @override
  State<PlayScreen> createState() => _PlayScreenState();
}

class _PlayScreenState extends State<PlayScreen> with WidgetsBindingObserver {
  AppController get c => widget.controller;

  late final TouchRouter _router;
  Size _viewport = Size.zero;
  bool _wasLandscape = true;
  bool _overlayVisible = false;
  bool _busyControl = false;
  bool _wasConnected = true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);

    _router = TouchRouter(
      state: c.connection.state,
      onStateChanged: c.connection.sendNow,
      onPressFeedback: _feedback,
      onLongPress: _onLongPress,
    );

    c.connection.addListener(_onConnectionChanged);
    c.connection.onRumble = _onRumble;
    _enterGameMode();
    PhonePadPlatform.haptic(HapticKind.connect, _hapticAmplitude);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    c.connection.removeListener(_onConnectionChanged);
    c.connection.onRumble = null;
    // Leaving with the motor running would keep the phone buzzing on a screen
    // that is no longer a controller.
    PhonePadPlatform.rumble(0, 0);
    _router.dispose();
    _exitGameMode();
    // Leaving the controller must release everything, not leave a button held.
    c.connection.state.reset();
    unawaited(c.connection.disconnect());
    super.dispose();
  }

  int get _hapticAmplitude =>
      (c.settings.hapticStrength.clamp(0, 100) * 255 / 100).round().clamp(
        1,
        255,
      );

  /// Holding the Guide button re-attaches the pad on the PC.
  ///
  /// This is the one repair a cloud game ever needs, and burying it behind the
  /// pause menu means finding it mid-session, over a fullscreen stream, with a
  /// game that is not responding. Guide is the right home for it: it is already
  /// the "something is wrong, get me out" button on a real pad.
  void _onLongPress(ControlSpec spec) {
    if (spec.mapping.buttons != Btn.guide) return;
    if (_busyControl || !c.connection.isConnected) return;
    PhonePadPlatform.haptic(HapticKind.connect, _hapticAmplitude);
    _runControl(
      ControlCommand.reattachPad,
      'Controller re-attached on the PC.',
    );
  }

  /// Play what the game asked the pad's motors to do.
  ///
  /// Gated on the same setting as press haptics: someone who turned vibration
  /// off meant all of it, not just the button clicks.
  void _onRumble(int large, int small) {
    if (!c.settings.hapticsEnabled) return;
    final scale = c.settings.hapticStrength.clamp(0, 100) / 100;
    PhonePadPlatform.rumble((large * scale).round(), (small * scale).round());
  }

  void _feedback(ControlSpec spec) {
    if (!c.settings.hapticsEnabled) return;
    // Sticks fire continuously while aiming; buzzing on every re-grab would be
    // noise rather than feedback.
    if (spec.type == ControlType.stick) return;
    PhonePadPlatform.haptic(HapticKind.press, _hapticAmplitude);
  }

  Future<void> _enterGameMode() async {
    await SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky);
    await PhonePadPlatform.setKeepAwake(true);
    await PhonePadPlatform.setLowLatencyWifi(true);
    await PhonePadPlatform.setGestureExclusion(true);
  }

  Future<void> _exitGameMode() async {
    await SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
    await PhonePadPlatform.setKeepAwake(false);
    await PhonePadPlatform.setLowLatencyWifi(false);
    await PhonePadPlatform.setGestureExclusion(false);
  }

  void _onConnectionChanged() {
    if (!mounted) return;
    final connected = c.connection.isConnected;

    // Deliberately does *not* forget the fingers that are down. Nothing is
    // transmitted while disconnected and the PC's watchdog releases the pad
    // within 120 ms, so nothing can stick — whereas clearing the pointer map
    // would leave thumbs that never left the glass dead until they were lifted
    // and put back. On a flapping link that happens repeatedly, and it reads as
    // multi-touch breaking.
    if (!connected) {
      c.connection.state.reset();
      PhonePadPlatform.rumble(0, 0);
    } else if (_wasConnected == false) {
      // Back again: whatever is still held takes effect immediately.
      _router.resync();
    }
    _wasConnected = connected;
    setState(() {});
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    // Backgrounding, a call, the power button: any of these can swallow the
    // pointer-up events, so release everything rather than trust them to come.
    if (state != AppLifecycleState.resumed) {
      _router.releaseAll();
      PhonePadPlatform.rumble(0, 0);
    } else {
      _enterGameMode();
    }
  }

  @override
  Widget build(BuildContext context) {
    final conn = c.connection;

    return Theme(
      data: buildNightTheme(),
      child: PopScope(
        // A stray back gesture mid-game should not silently disconnect.
        canPop: false,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop) _openMenu();
        },
        child: Scaffold(
          backgroundColor: kNight,
          body: LayoutBuilder(
            builder: (context, constraints) {
              final size = Size(constraints.maxWidth, constraints.maxHeight);
              final isLandscape = size.width >= size.height;

              // Recompute geometry only when it can actually have changed.
              if (size != _viewport || isLandscape != _wasLandscape) {
                _viewport = size;
                _wasLandscape = isLandscape;
                WidgetsBinding.instance.addPostFrameCallback((_) {
                  _router.updateGeometry(
                    resolveGeometry(
                      c.activeLayout.forOrientation(isLandscape: isLandscape),
                      size.width,
                      size.height,
                    ),
                  );
                });
              }

              return Stack(
                children: [
                  Positioned.fill(child: ControllerSurface(router: _router)),

                  // Top-left, not centred: the Guide button owns the centre of
                  // the top edge, and a readout sitting on top of a control is
                  // both unreadable and in the way of pressing it.
                  Positioned(
                    top: 8,
                    left: 10,
                    child: _StatusPill(connection: conn),
                  ),

                  Positioned(
                    top: 0,
                    right: 4,
                    child: IconButton(
                      icon: const Icon(Icons.more_horiz, color: kNightInk3),
                      onPressed: _openMenu,
                    ),
                  ),

                  if (_overlayVisible) _buildMenu(context),
                ],
              );
            },
          ),
        ),
      ),
    );
  }

  void _openMenu() {
    // The overlay covers the controls, so anything held would otherwise stay
    // held for as long as the menu is open.
    _router.releaseAll();
    setState(() => _overlayVisible = true);
  }

  Future<void> _runControl(ControlCommand command, String done) async {
    final messenger = ScaffoldMessenger.of(context);
    setState(() => _busyControl = true);
    await c.connection.sendControl(command);
    if (!mounted) return;
    setState(() => _busyControl = false);
    messenger.showSnackBar(
      SnackBar(content: Text(done), duration: const Duration(seconds: 3)),
    );
  }

  Widget _buildMenu(BuildContext context) {
    final conn = c.connection;
    return Positioned.fill(
      child: GestureDetector(
        onTap: () => setState(() => _overlayVisible = false),
        child: ColoredBox(
          color: kNight.withValues(alpha: 0.82),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420, maxHeight: 560),
              // The controller is often held in landscape where vertical space
              // is tight, so this scrolls rather than overflowing.
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(12),
                child: GestureDetector(
                  // Swallow taps inside the panel so they don't dismiss it.
                  onTap: () {},
                  child: Container(
                    padding: const EdgeInsets.fromLTRB(22, 20, 22, 22),
                    decoration: BoxDecoration(
                      color: kNightHi,
                      borderRadius: BorderRadius.circular(kRadiusL),
                    ),
                    child: Panel(
                      title: 'Paused',
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          StatRow('PC', conn.connectedPc?.hostname ?? 'none'),
                          StatRow(
                            'Round trip',
                            conn.rttUs == 0
                                ? 'measuring'
                                : '${(conn.rttUs / 1000).toStringAsFixed(1)} ms',
                          ),
                          StatRow('Sending', '${conn.stats.sentPps} /sec'),
                          StatRow(
                            'PC accepting',
                            '${conn.stats.pcAcceptedPps} /sec',
                            color: conn.stats.pcAcceptedPps > 0
                                ? kNightGood
                                : kNightWarn,
                          ),

                          const SizedBox(height: 14),
                          FilledButton.icon(
                            onPressed: () =>
                                setState(() => _overlayVisible = false),
                            icon: const Icon(Icons.play_arrow),
                            label: const Text('Resume'),
                          ),

                          const Divider(height: 28),
                          const Align(
                            alignment: Alignment.centerLeft,
                            child: SectionLabel('If the game ignores the pad'),
                          ),
                          const SizedBox(height: 8),
                          const Align(
                            alignment: Alignment.centerLeft,
                            child: Text(
                              'If Windows sees the pad but the game ignores it, the '
                              'game probably started before the controller existed. '
                              'Re-attaching makes it appear as a fresh plug-in '
                              'without touching this connection.',
                              style: TextStyle(
                                color: kNightInk2,
                                fontSize: 13,
                                height: 1.4,
                              ),
                            ),
                          ),
                          const SizedBox(height: 10),
                          const Align(
                            alignment: Alignment.centerLeft,
                            child: Text(
                              'You can also hold the XBOX button on the '
                              'controller to do this without pausing.',
                              style: TextStyle(
                                color: kNightInk2,
                                fontSize: 13,
                                height: 1.4,
                              ),
                            ),
                          ),
                          const SizedBox(height: 10),
                          OutlinedButton.icon(
                            onPressed: _busyControl || !conn.isConnected
                                ? null
                                : () => _runControl(
                                    ControlCommand.reattachPad,
                                    'Controller re-attached on the PC.',
                                  ),
                            icon: _busyControl
                                ? const SizedBox(
                                    width: 16,
                                    height: 16,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 2,
                                    ),
                                  )
                                : const Icon(Icons.usb_rounded, size: 18),
                            label: const Text('Re-attach controller on PC'),
                          ),
                          const SizedBox(height: 8),
                          OutlinedButton.icon(
                            onPressed: _busyControl || !conn.isConnected
                                ? null
                                : () => _runControl(
                                    ControlCommand.releaseAll,
                                    'All controls released.',
                                  ),
                            icon: const Icon(
                              Icons.pan_tool_alt_outlined,
                              size: 18,
                            ),
                            label: const Text('Release all inputs'),
                          ),
                          const SizedBox(height: 8),
                          OutlinedButton.icon(
                            onPressed: () {
                              setState(() => _overlayVisible = false);
                              Navigator.of(context).push(
                                MaterialPageRoute(
                                  builder: (_) =>
                                      DiagnosticsScreen(controller: c),
                                ),
                              );
                            },
                            icon: const Icon(Icons.speed, size: 18),
                            label: const Text('Diagnostics'),
                          ),

                          const Divider(height: 28),
                          OutlinedButton.icon(
                            onPressed: () => Navigator.of(context).pop(),
                            icon: const Icon(Icons.logout, size: 18),
                            label: const Text('Disconnect'),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _StatusPill extends StatelessWidget {
  const _StatusPill({required this.connection});

  final PhonePadConnection connection;

  @override
  Widget build(BuildContext context) {
    final (text, colour) = switch (connection.phase) {
      ConnectionPhase.connected => (
        connection.rttUs == 0
            ? 'connected'
            : '${(connection.rttUs / 1000).toStringAsFixed(1)} ms',
        kNightGood,
      ),
      ConnectionPhase.reconnecting => ('reconnecting', kNightWarn),
      ConnectionPhase.connecting => ('connecting', kNightWarn),
      _ => ('disconnected', kNightBad),
    };

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: kNightHi.withValues(alpha: 0.85),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 7,
            height: 7,
            decoration: BoxDecoration(color: colour, shape: BoxShape.circle),
          ),
          const SizedBox(width: 7),
          Text(
            text,
            style: TextStyle(
              color: colour,
              fontSize: 12,
              fontWeight: FontWeight.w600,
              fontFeatures: kFigures,
            ),
          ),
        ],
      ),
    );
  }
}
