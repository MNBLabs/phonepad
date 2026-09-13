import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'app_controller.dart';
import 'ui/home_screen.dart';
import 'ui/onboarding_screen.dart';
import 'ui/theme.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Landscape and portrait are both supported; the layout model carries an
  // arrangement for each and swaps automatically.
  await SystemChrome.setPreferredOrientations(DeviceOrientation.values);

  final controller = await AppController.create();
  runApp(PhonePadApp(controller: controller));
}

class PhonePadApp extends StatefulWidget {
  const PhonePadApp({super.key, required this.controller});

  final AppController controller;

  @override
  State<PhonePadApp> createState() => _PhonePadAppState();
}

class _PhonePadAppState extends State<PhonePadApp> {
  late bool _onboarded = widget.controller.settings.onboarded;

  void _finishOnboarding() {
    widget.controller.updateSettings((s) => s.onboarded = true);
    setState(() => _onboarded = true);
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'PhonePad',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(),
      home: _onboarded
          ? HomeScreen(controller: widget.controller)
          : OnboardingScreen(onDone: _finishOnboarding),
    );
  }
}
