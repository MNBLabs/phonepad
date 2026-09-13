/// Live diagnostics. Every number here is measured — nothing is estimated and
/// nothing is invented when it is unknown, which is why "measuring…" appears
/// instead of a plausible-looking zero.
library;

import 'dart:async';

import 'package:flutter/material.dart';

import '../app_controller.dart';
import '../core/demo.dart';
import '../core/net/connection.dart';
import 'theme.dart';

class DiagnosticsScreen extends StatefulWidget {
  const DiagnosticsScreen({super.key, required this.controller});

  final AppController controller;

  @override
  State<DiagnosticsScreen> createState() => _DiagnosticsScreenState();
}

class _DiagnosticsScreenState extends State<DiagnosticsScreen> {
  Timer? _refresh;

  @override
  void initState() {
    super.initState();
    // The connection notifies once a second; refresh a little faster so the
    // display does not look frozen between windows.
    _refresh = Timer.periodic(const Duration(milliseconds: 400), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _refresh?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = widget.controller;
    final conn = c.connection;
    final s = conn.stats;

    final (phaseText, phaseColour) = switch (conn.phase) {
      ConnectionPhase.connected => ('Connected', kGood),
      ConnectionPhase.reconnecting => ('Reconnecting', kWarn),
      ConnectionPhase.connecting => ('Connecting', kWarn),
      ConnectionPhase.searching => ('Searching', kWarn),
      ConnectionPhase.failed => ('Failed', kBad),
      ConnectionPhase.idle => ('Idle', kInk2),
    };

    final lossPercent = s.pcLossPermille / 10.0;

    return Scaffold(
      appBar: AppBar(title: const Text('Diagnostics')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
        children: [
          Panel(
            title: 'Connection',
            child: Column(
              children: [
                StatRow('State', phaseText, color: phaseColour),
                StatRow('Transport', 'Wi-Fi / UDP'),
                StatRow('PC', conn.connectedPc?.hostname ?? '—'),
                if (conn.error != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text(
                      conn.error!,
                      style: const TextStyle(color: kWarn, fontSize: 13),
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: 16),

          Panel(
            title: 'Latency',
            child: Column(
              children: [
                StatRow(
                  'Round trip',
                  s.rttUs == 0
                      ? 'measuring…'
                      : '${(s.rttUs / 1000).toStringAsFixed(2)} ms',
                  color: s.rttUs == 0
                      ? kInk2
                      : (s.rttUs < 10000
                            ? kGood
                            : s.rttUs < 25000
                            ? kWarn
                            : kBad),
                ),
                StatRow(
                  'Jitter',
                  s.rttUs == 0
                      ? '—'
                      : '${(s.jitterUs / 1000).toStringAsFixed(2)} ms',
                ),
                const Padding(
                  padding: EdgeInsets.only(top: 8),
                  child: Text(
                    'Round trip is measured from the timestamp the PC echoes '
                    'back, so it covers phone → PC → phone including the '
                    'virtual controller update.',
                    style: TextStyle(color: kInk2, fontSize: 12),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 16),

          Panel(
            title: 'Packets',
            child: Column(
              children: [
                StatRow('Sent', '${s.sentPps} /sec'),
                StatRow('Accepted by PC', '${s.pcAcceptedPps} /sec'),
                StatRow(
                  'Packet loss',
                  '${lossPercent.toStringAsFixed(1)} %',
                  color: lossPercent < 1
                      ? kGood
                      : lossPercent < 5
                      ? kWarn
                      : kBad,
                ),
                StatRow('Feedback', '${s.feedbackPps} /sec'),
                StatRow('Sequence', '${s.seq}'),
              ],
            ),
          ),
          const SizedBox(height: 16),

          Panel(
            title: 'Device',
            child: Column(
              children: [
                StatRow('Phone', c.display.model),
                StatRow('Android SDK', '${c.display.androidSdk}'),
                StatRow(
                  'Refresh rate',
                  '${c.display.refreshRate.round()} Hz of '
                      '${c.display.maxRefreshRate.round()} Hz',
                  color: c.display.refreshRate >= c.display.maxRefreshRate - 1
                      ? kGood
                      : kWarn,
                ),
                StatRow(
                  'Wi-Fi address',
                  demoOr(c.link.address ?? 'unknown', kDemoAddress),
                ),
                StatRow('Subnet broadcast', c.link.broadcast ?? 'unknown'),
                StatRow('Send rate target', '${c.settings.sendRateHz} Hz'),
              ],
            ),
          ),
          const SizedBox(height: 16),

          Panel(
            title: 'Live input',
            child: _LiveInput(connection: conn),
          ),
        ],
      ),
    );
  }
}

/// Shows the exact values currently on the wire, which is the quickest way to
/// tell a layout problem from a network problem.
class _LiveInput extends StatelessWidget {
  const _LiveInput({required this.connection});

  final PhonePadConnection connection;

  @override
  Widget build(BuildContext context) {
    final s = connection.state;
    return Column(
      children: [
        StatRow('Buttons', '0x${s.buttons.toRadixString(16).padLeft(4, '0')}'),
        StatRow('Left stick', '${s.lx}, ${s.ly}'),
        StatRow('Right stick', '${s.rx}, ${s.ry}'),
        StatRow('Triggers', 'LT ${s.lt}   RT ${s.rt}'),
        StatRow(
          'Rumble from game',
          '${connection.stats.rumbleLarge} / ${connection.stats.rumbleSmall}',
        ),
      ],
    );
  }
}
