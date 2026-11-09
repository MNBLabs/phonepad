/// Connect screen: find the PC, pair once, then play.
///
/// The whole screen is built around one sentence — "open the app, tap your PC,
/// play" — so discovery starts on open, a known PC is one tap from the
/// controller, and everything else on the screen is quieter than that tap.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app_controller.dart';
import '../core/demo.dart';
import '../core/net/connection.dart';
import '../platform/phonepad_platform.dart';
import 'diagnostics_screen.dart';
import 'layouts_screen.dart';
import 'mark.dart';
import 'play_screen.dart';
import 'settings_screen.dart';
import 'ad_strip.dart';
import 'theme.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.controller});

  final AppController controller;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  AppController get c => widget.controller;
  bool _searching = false;
  String? _notice;

  @override
  void initState() {
    super.initState();
    c.addListener(_onChanged);
    // Restore the normal system bars in case we came back from the controller.
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
    WidgetsBinding.instance.addPostFrameCallback((_) => _search());
  }

  @override
  void dispose() {
    c.removeListener(_onChanged);
    super.dispose();
  }

  void _onChanged() {
    if (mounted) setState(() {});
  }

  Future<void> _search() async {
    if (_searching) return;
    setState(() {
      _searching = true;
      _notice = null;
    });
    await c.discover();
    if (!mounted) return;
    setState(() => _searching = false);
  }

  Future<void> _onTapPc(DiscoveredPc pc) async {
    final known = c.data.pcById(pc.serverId) != null;
    if (!known) {
      final code = await _askForCode(pc);
      if (code == null || !mounted) return;

      final failure = await c.connection.pair(pc, code);
      if (!mounted) return;
      if (failure != null) {
        setState(() => _notice = failure);
        return;
      }
    }

    final ok = await c.connection.connect(pc);
    if (!mounted) return;
    if (!ok) {
      setState(() => _notice = c.connection.error ?? 'Could not connect.');
      return;
    }
    await Navigator.of(
      context,
    ).push(MaterialPageRoute(builder: (_) => PlayScreen(controller: c)));
  }

  Future<String?> _askForCode(DiscoveredPc pc) async {
    // Deliberately does not pre-check `pc.pairingMode`. That flag is a snapshot
    // from whenever discovery last ran, so refusing on it strands the user when
    // they open the pairing window *after* searching — they would have to guess
    // that a re-search was needed. The PC answers `NotInPairingMode` if the
    // window really is shut, and that reply is never stale.
    final field = TextEditingController();
    return showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Pair with ${pc.hostname}'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              'Enter the six digits shown in the PhonePad window on your PC. '
              'If there is no code there, click "Pair a phone" first.',
              style: TextStyle(color: kInk2, fontSize: 14, height: 1.4),
            ),
            const SizedBox(height: 20),
            TextField(
              controller: field,
              autofocus: true,
              keyboardType: TextInputType.number,
              maxLength: 6,
              style: const TextStyle(
                fontSize: 32,
                letterSpacing: 8,
                fontWeight: FontWeight.w500,
                fontFeatures: kFigures,
              ),
              textAlign: TextAlign.center,
              inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              onSubmitted: (v) => Navigator.of(context).pop(v),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(field.text),
            child: const Text('Pair'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final discovered = c.connection.discovered;

    return Scaffold(
      appBar: AppBar(
        titleSpacing: 20,
        title: const Row(
          children: [
            PhonePadMark(size: 22, color: kInk),
            SizedBox(width: 10),
            Text('PhonePad'),
          ],
        ),
        actions: [
          IconButton(
            tooltip: 'Diagnostics',
            icon: const Icon(Icons.speed),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(
                builder: (_) => DiagnosticsScreen(controller: c),
              ),
            ),
          ),
          IconButton(
            tooltip: 'Layouts',
            icon: const Icon(Icons.dashboard_customize_outlined),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(builder: (_) => LayoutsScreen(controller: c)),
            ),
          ),
          IconButton(
            tooltip: 'Settings',
            icon: const Icon(Icons.settings_outlined),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(builder: (_) => SettingsScreen(controller: c)),
            ),
          ),
        ],
      ),
      bottomNavigationBar: const AdStrip(),
      body: RefreshIndicator(
        onRefresh: _search,
        child: ListView(
          padding: const EdgeInsets.fromLTRB(20, 4, 20, 40),
          children: [
            ContentColumn(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (c.store.loadWarning != null) ...[
                    _Notice(text: c.store.loadWarning!),
                    const SizedBox(height: 16),
                  ],
                  if (_notice != null) ...[
                    _Notice(text: _notice!),
                    const SizedBox(height: 16),
                  ],
                  if (_searching && discovered.isEmpty)
                    const _Searching()
                  else if (discovered.isEmpty)
                    _NothingFound(onRetry: _search, link: c.link)
                  else ...[
                    for (final pc in discovered) ...[
                      _PcTile(
                        pc: pc,
                        paired: c.data.pcById(pc.serverId) != null,
                        onTap: () => _onTapPc(pc),
                      ),
                      const SizedBox(height: 10),
                    ],
                    Align(
                      alignment: Alignment.center,
                      child: TextButton.icon(
                        onPressed: _searching ? null : _search,
                        icon: const Icon(Icons.refresh, size: 17),
                        style: TextButton.styleFrom(foregroundColor: kInk2),
                        label: Text(_searching ? 'Searching' : 'Search again'),
                      ),
                    ),
                  ],
                  const SizedBox(height: 28),
                  Panel(
                    title: 'This phone',
                    child: Column(
                      children: [
                        StatRow('Device', c.data.deviceName),
                        if (c.link.networks.isEmpty)
                          const StatRow(
                            'Network',
                            'No Wi-Fi or hotspot',
                            color: kWarn,
                          )
                        else
                          for (final n in c.link.networks)
                            StatRow(
                              n.kind.label,
                              demoOr(n.address, kDemoAddress),
                            ),
                        StatRow(
                          'Display',
                          '${c.display.refreshRate.round()} Hz '
                              '(max ${c.display.maxRefreshRate.round()})',
                        ),
                        StatRow('Layout', c.activeLayout.name),
                      ],
                    ),
                  ),
                  if (c.data.pairedPcs.isNotEmpty) ...[
                    const SizedBox(height: 24),
                    Panel(
                      title: 'Paired PCs',
                      child: Column(
                        children: [
                          for (final pc in c.data.pairedPcs)
                            Padding(
                              padding: const EdgeInsets.symmetric(vertical: 4),
                              child: Row(
                                children: [
                                  Expanded(
                                    child: Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                        Text(
                                          pc.hostname,
                                          style: const TextStyle(
                                            fontSize: 15,
                                            fontWeight: FontWeight.w500,
                                          ),
                                        ),
                                        Text(
                                          demoOr(
                                            pc.lastAddress ?? 'address unknown',
                                            kDemoAddress,
                                          ),
                                          style: const TextStyle(
                                            color: kInk3,
                                            fontSize: 13,
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                  TextButton(
                                    onPressed: () => c.forgetPc(pc.serverId),
                                    style: TextButton.styleFrom(
                                      foregroundColor: kInk2,
                                    ),
                                    child: const Text('Forget'),
                                  ),
                                ],
                              ),
                            ),
                        ],
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Searching extends StatelessWidget {
  const _Searching();

  @override
  Widget build(BuildContext context) => const Padding(
    padding: EdgeInsets.symmetric(vertical: 40),
    child: Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        SizedBox(
          width: 16,
          height: 16,
          child: CircularProgressIndicator(strokeWidth: 2, color: kInk2),
        ),
        SizedBox(width: 12),
        Text('Looking for your PC', style: TextStyle(color: kInk2)),
      ],
    ),
  );
}

/// The empty state does the work an error message should: it names the three
/// things that are actually ever wrong, in the order they are worth checking.
/// A spinner that quietly stops tells the user nothing they can act on.
class _NothingFound extends StatelessWidget {
  const _NothingFound({required this.onRetry, required this.link});

  final VoidCallback onRetry;
  final LinkInfo link;

  @override
  Widget build(BuildContext context) {
    final sharing = link.networks.any((n) =>
        n.kind == NetworkKind.hotspot ||
        n.kind == NetworkKind.usb ||
        n.kind == NetworkKind.bluetooth);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 20),
        Text('No PC found', style: kTitle.copyWith(color: kInk)),
        const SizedBox(height: 16),
        const _Check('Is the PhonePad companion running on your PC?'),
        const Hairline(),
        if (link.networks.isEmpty)
          const _Check(
            'This phone is not on Wi-Fi and is not sharing a hotspot. '
            'Join the same Wi-Fi as the PC, or turn on the hotspot and '
            'connect the PC to it.',
          )
        else
          const _Check(
            "Is the PC on the same Wi-Fi, or connected to this phone's "
            'hotspot or USB tethering?',
          ),
        const Hairline(),
        // A network Windows has not seen before is filed as public, and its
        // firewall blocks PhonePad there. A phone's hotspot always starts out
        // that way, so say so where it applies.
        _Check(
          sharing
              ? 'Windows treats a new hotspot as a public network and blocks '
                  'PhonePad on it. The PhonePad window on the PC offers to '
                  'trust it.'
              : 'Did Windows Firewall ask to allow PhonePad? The PhonePad '
                  'window on the PC says if it is being blocked.',
        ),
        const SizedBox(height: 22),
        Align(
          alignment: Alignment.centerLeft,
          child: FilledButton(
            onPressed: onRetry,
            child: const Text('Search again'),
          ),
        ),
        const SizedBox(height: 12),
      ],
    );
  }
}

class _Check extends StatelessWidget {
  const _Check(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 12),
    child: Text(
      text,
      style: const TextStyle(color: kInk2, fontSize: 15, height: 1.4),
    ),
  );
}

/// The one thing on the screen that matters. Raised white on paper, a
/// hairline, and the action as a pill on the right so a known PC is one tap
/// from the controller.
class _PcTile extends StatelessWidget {
  const _PcTile({required this.pc, required this.paired, required this.onTap});

  final DiscoveredPc pc;
  final bool paired;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Pressable(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.fromLTRB(18, 16, 12, 16),
        decoration: BoxDecoration(
          color: kRaised,
          borderRadius: BorderRadius.circular(kRadiusM),
          border: Border.all(color: kLine),
        ),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(pc.hostname, style: kHeading.copyWith(color: kInk)),
                  const SizedBox(height: 4),
                  Text(
                    paired
                        ? demoOr(pc.address.address, kDemoAddress)
                        : 'Not paired yet',
                    style: TextStyle(
                      color: paired ? kInk2 : kWarn,
                      fontSize: 14,
                      fontFeatures: kFigures,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 12),
            IgnorePointer(
              child: FilledButton(
                onPressed: () {},
                style: FilledButton.styleFrom(
                  minimumSize: const Size(0, 40),
                  padding: const EdgeInsets.symmetric(horizontal: 18),
                ),
                child: Text(paired ? 'Play' : 'Pair'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Notice extends StatelessWidget {
  const _Notice({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: kWarn.withValues(alpha: 0.08),
        borderRadius: BorderRadius.circular(kRadiusM),
      ),
      child: Text(
        text,
        style: const TextStyle(color: kWarn, fontSize: 14, height: 1.4),
      ),
    );
  }
}
