import 'package:flutter/material.dart';

import '../app_controller.dart';
import '../core/demo.dart';
import '../core/links.dart';
import '../platform/phonepad_platform.dart';
import 'theme.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key, required this.controller});

  final AppController controller;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  @override
  Widget build(BuildContext context) {
    final c = widget.controller;
    final s = c.settings;

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(20, 8, 20, 40),
        children: [
          ContentColumn(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Panel(
                  title: 'Haptics',
                  child: Column(
                    children: [
                      SwitchListTile(
                        contentPadding: EdgeInsets.zero,
                        title: const Text('Vibrate on press'),
                        value: s.hapticsEnabled,
                        onChanged: (v) => setState(() {
                          c.updateSettings((s) => s.hapticsEnabled = v);
                          if (v) {
                            PhonePadPlatform.haptic(
                              HapticKind.press,
                              (s.hapticStrength * 255 / 100).round().clamp(
                                1,
                                255,
                              ),
                            );
                          }
                        }),
                      ),
                      if (s.hapticsEnabled) ...[
                        const Align(
                          alignment: Alignment.centerLeft,
                          child: Text(
                            'Strength',
                            style: TextStyle(color: kInk2),
                          ),
                        ),
                        Slider(
                          value: s.hapticStrength.toDouble(),
                          min: 5,
                          max: 100,
                          divisions: 19,
                          label: '${s.hapticStrength}%',
                          onChanged: (v) =>
                              setState(() => s.hapticStrength = v.round()),
                          // Fire a sample and persist only when the drag ends, so a
                          // slider sweep is not a buzzing storm plus 19 file writes.
                          onChangeEnd: (v) {
                            c.updateSettings(
                              (s) => s.hapticStrength = v.round(),
                            );
                            PhonePadPlatform.haptic(
                              HapticKind.press,
                              (v * 255 / 100).round().clamp(1, 255),
                            );
                          },
                        ),
                      ],
                    ],
                  ),
                ),
                const SizedBox(height: 32),

                Panel(
                  title: 'Network',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Send rate: ${s.sendRateHz} Hz',
                        style: const TextStyle(color: kInk2),
                      ),
                      Slider(
                        value: s.sendRateHz.toDouble(),
                        min: 60,
                        max: 250,
                        divisions: 19,
                        label: '${s.sendRateHz} Hz',
                        onChanged: (v) =>
                            setState(() => s.sendRateHz = v.round()),
                        onChangeEnd: (v) =>
                            c.updateSettings((s) => s.sendRateHz = v.round()),
                      ),
                      const Text(
                        'Higher is smoother and costs a little more battery. Each '
                        'packet is 40 bytes, so even 250 Hz is about 10 KB/s. The new '
                        'rate applies on the next connection.',
                        style: TextStyle(color: kInk2, fontSize: 12),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 32),

                Panel(
                  title: 'About',
                  child: Column(
                    children: [
                      StatRow('Device name', c.data.deviceName),
                      StatRow(
                        'Device ID',
                        '${demoOr(c.data.deviceId, kDemoDeviceId).substring(0, 8)}…',
                      ),
                      StatRow('Protocol', 'PhonePad v1 (binary UDP)'),
                      const SizedBox(height: 8),
                      const Text(
                        'The PC needs the PhonePad companion running. Pairing uses '
                        'X25519 with a six-digit code, and every input packet is '
                        'authenticated, so no other device on the network can inject '
                        'controller input.',
                        style: TextStyle(
                          color: kInk2,
                          fontSize: 12.5,
                          height: 1.45,
                        ),
                      ),
                    ],
                  ),
                ),

                const SizedBox(height: 32),
                Panel(
                  title: 'Support',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Text(
                        'PhonePad is free and open source. If it saved you buying a '
                        'controller, you can support development.',
                        style: TextStyle(
                          color: kInk2,
                          fontSize: 12.5,
                          height: 1.45,
                        ),
                      ),
                      const SizedBox(height: 14),
                      OutlinedButton.icon(
                        onPressed: () => launchProjectUrl(kProjectUrl),
                        icon: const Icon(Icons.code, size: 18),
                        label: const Text('View the source'),
                      ),
                      const SizedBox(height: 8),
                      OutlinedButton.icon(
                        onPressed: () => launchProjectUrl(kSupportUrl),
                        icon: const Icon(Icons.favorite_border, size: 18),
                        label: const Text('Support PhonePad'),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
