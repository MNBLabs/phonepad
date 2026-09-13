/// What a stranger sees first.
///
/// PhonePad cannot work until something is installed on the PC, and nothing in
/// the app can do that for the user. Before this existed, a first run showed an
/// empty PC list and a spinner, which looks like a broken app rather than an
/// unfinished setup. Three screens, skippable, shown once.
library;

import 'package:flutter/material.dart';

import '../core/links.dart';
import 'controller_preview.dart';
import 'mark.dart';
import 'theme.dart';

class OnboardingScreen extends StatefulWidget {
  const OnboardingScreen({super.key, required this.onDone});

  final VoidCallback onDone;

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  final _pages = PageController();
  int _page = 0;

  static const _steps = <_Step>[
    _Step(
      title: 'No controller?\nUse your phone.',
      picture: true,
      body:
          'PhonePad turns this phone into a wireless Xbox-compatible '
          'controller for your Windows PC. No Bluetooth, no cable, nothing '
          'to buy.',
      note:
          'Windows sees a real Xbox pad, so anything that takes a controller '
          'takes this, including Xbox Cloud Gaming.',
    ),
    _Step(
      title: 'Install the companion\non your PC',
      body:
          'One small program has to run on the PC. It is what creates the '
          'virtual controller Windows can see.',
      note:
          'Get it from phonepad.dynshift.com. You will also need the ViGEmBus '
          'driver, once. The setup page walks through both.',
      action: 'Open the setup guide',
      url: kSetupUrl,
    ),
    _Step(
      title: 'Pair once,\nthen just play',
      body:
          'Put both devices on the same Wi-Fi. Your PC shows up here on its '
          'own; tap it and type the six digits it displays.',
      note:
          'After that, opening PhonePad and tapping your PC goes straight to '
          'the controller.',
    ),
  ];

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
  }

  void _next() {
    if (_page >= _steps.length - 1) {
      widget.onDone();
      return;
    }
    _pages.nextPage(duration: kBeat, curve: kEase);
  }

  @override
  Widget build(BuildContext context) {
    final last = _page == _steps.length - 1;

    return Scaffold(
      body: SafeArea(
        child: ContentColumn(
          child: Column(
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(24, 16, 12, 0),
                child: Row(
                  children: [
                    const BrandTile(size: 56),
                    const Spacer(),
                    TextButton(
                      onPressed: widget.onDone,
                      style: TextButton.styleFrom(foregroundColor: kInk2),
                      child: const Text('Skip'),
                    ),
                  ],
                ),
              ),
              Expanded(
                child: PageView.builder(
                  controller: _pages,
                  itemCount: _steps.length,
                  onPageChanged: (i) => setState(() => _page = i),
                  itemBuilder: (_, i) => _StepView(step: _steps[i]),
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(24, 0, 24, 20),
                child: Row(
                  children: [
                    for (var i = 0; i < _steps.length; i++)
                      AnimatedContainer(
                        duration: kBeat,
                        curve: kEase,
                        margin: const EdgeInsets.only(right: 6),
                        width: i == _page ? 22 : 6,
                        height: 6,
                        decoration: BoxDecoration(
                          color: i == _page ? kInk : kLine,
                          borderRadius: BorderRadius.circular(3),
                        ),
                      ),
                    const Spacer(),
                    FilledButton(
                      onPressed: _next,
                      child: Text(last ? 'Find my PC' : 'Next'),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Step {
  const _Step({
    required this.title,
    required this.body,
    required this.note,
    this.action,
    this.url,
    this.picture = false,
  });

  final String title;
  final bool picture;
  final String body;
  final String note;
  final String? action;
  final String? url;
}

class _StepView extends StatelessWidget {
  const _StepView({required this.step});

  final _Step step;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      padding: const EdgeInsets.symmetric(horizontal: 24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const SizedBox(height: 32),
          Text(step.title, style: kDisplay.copyWith(color: kInk)),
          const SizedBox(height: 22),
          if (step.picture) ...[
            const ControllerPreview(),
            const SizedBox(height: 24),
          ],
          Text(
            step.body,
            style: const TextStyle(fontSize: 17, height: 1.45, color: kInk),
          ),
          const SizedBox(height: 14),
          Text(
            step.note,
            style: const TextStyle(fontSize: 15, height: 1.45, color: kInk2),
          ),
          if (step.action != null) ...[
            const SizedBox(height: 24),
            OutlinedButton.icon(
              onPressed: () => launchProjectUrl(step.url!),
              icon: const Icon(Icons.arrow_outward, size: 18),
              label: Text(step.action!),
            ),
          ],
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}
