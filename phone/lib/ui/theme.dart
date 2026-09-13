/// The design system. Values come from `docs/design.md`; if the two disagree,
/// this file is wrong.
///
/// Two grounds, deliberately. **Paper** is for the surfaces you set things up
/// on: first run, home, settings, the editor's inspector. **Night** is for the
/// surface you play on. The controller is drawn over somebody's game on an
/// OLED in a dim room, and the pause menu sits on top of it; a white panel
/// there would be a torch. Going from one to the other is the moment the phone
/// becomes a controller, and it is meant to be felt.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

// --- paper -------------------------------------------------------------------

const kPaper = Color(0xFFF7F7FC);
const kRaised = Color(0xFFFFFFFF);
const kSunken = Color(0xFFEEEDF7);
const kLine = Color(0xFFE2E1EE);

const kInk = Color(0xFF15142B);
const kInk2 = Color(0xFF5A5878);
const kInk3 = Color(0xFF807EA0);

/// The periwinkle stop of the brand gradient, exactly. Decoration and large
/// marks only: it is 3.2:1 on paper, so it never carries text.
const kBrand = Color(0xFF8178FF);

/// The one interactive colour. Buttons, links, the active state.
const kAccent = Color(0xFF5A4FE6);
const kAccentDeep = Color(0xFF4A40D4);

const kGood = Color(0xFF177F4A);
const kWarn = Color(0xFF935B00);
const kBad = Color(0xFFC43837);

// --- night -------------------------------------------------------------------

const kNight = Color(0xFF0B0B14);
const kNightHi = Color(0xFF15152A);
const kNightLine = Color(0xFF24243C);
const kNightInk = Color(0xFFF3F2FF);
const kNightInk2 = Color(0xFF9B99B8);
const kNightInk3 = Color(0xFF6B6A8A);
const kNightGood = Color(0xFF4FD38E);
const kNightWarn = Color(0xFFF2B84B);
const kNightBad = Color(0xFFF4635F);

// --- type --------------------------------------------------------------------

const kFont = 'Bricolage';
const kFontDisplay = 'BricolageDisplay';

/// Tabular figures, for anything that changes while you look at it.
const kFigures = [FontFeature.tabularFigures()];

const kDisplay = TextStyle(
  fontFamily: kFontDisplay,
  fontWeight: FontWeight.w800,
  fontSize: 44,
  height: 1.0,
  letterSpacing: -1.4,
);

const kTitle = TextStyle(
  fontFamily: kFontDisplay,
  fontWeight: FontWeight.w600,
  fontSize: 22,
  height: 1.15,
  letterSpacing: -0.4,
);

const kHeading = TextStyle(
  fontFamily: kFontDisplay,
  fontWeight: FontWeight.w600,
  fontSize: 17,
  height: 1.2,
  letterSpacing: -0.2,
);

// --- shape and motion --------------------------------------------------------

const kRadiusL = 24.0;
const kRadiusM = 14.0;
const kRadiusS = 8.0;

/// Motion is feedback, not personality. One fast beat for presses, one for
/// anything that moves across the screen, one curve.
const kFast = Duration(milliseconds: 120);
const kBeat = Duration(milliseconds: 200);
const kEase = Cubic(0.23, 1, 0.32, 1);

// --- themes ------------------------------------------------------------------

ThemeData buildTheme() {
  const scheme = ColorScheme.light(
    primary: kAccent,
    onPrimary: Colors.white,
    secondary: kBrand,
    surface: kPaper,
    onSurface: kInk,
    error: kBad,
    outline: kLine,
  );
  return _base(
    scheme,
    ground: kPaper,
    raised: kRaised,
    sunken: kSunken,
    line: kLine,
    ink: kInk,
    ink2: kInk2,
    overlay: SystemUiOverlayStyle.dark,
  );
}

ThemeData buildNightTheme() {
  const scheme = ColorScheme.dark(
    primary: kBrand,
    onPrimary: kNight,
    secondary: kBrand,
    surface: kNightHi,
    onSurface: kNightInk,
    error: kNightBad,
    outline: kNightLine,
  );
  return _base(
    scheme,
    ground: kNight,
    raised: kNightHi,
    sunken: kNightHi,
    line: kNightLine,
    ink: kNightInk,
    ink2: kNightInk2,
    overlay: SystemUiOverlayStyle.light,
  );
}

ThemeData _base(
  ColorScheme scheme, {
  required Color ground,
  required Color raised,
  required Color sunken,
  required Color line,
  required Color ink,
  required Color ink2,
  required SystemUiOverlayStyle overlay,
}) {
  final pill = RoundedRectangleBorder(borderRadius: BorderRadius.circular(999));
  const buttonText = TextStyle(
    fontFamily: kFont,
    fontSize: 15,
    fontWeight: FontWeight.w600,
    letterSpacing: 0,
  );

  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    fontFamily: kFont,
    scaffoldBackgroundColor: ground,
    canvasColor: ground,
    dialogTheme: DialogThemeData(
      backgroundColor: raised,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(kRadiusL),
      ),
      titleTextStyle: kTitle.copyWith(color: ink, fontSize: 20),
      contentTextStyle: TextStyle(
        fontFamily: kFont,
        color: ink2,
        fontSize: 14,
        height: 1.45,
      ),
    ),
    splashFactory: NoSplash.splashFactory,
    highlightColor: ink.withValues(alpha: 0.06),
    textTheme: TextTheme(
      bodyMedium: TextStyle(
        fontFamily: kFont,
        fontSize: 15,
        height: 1.45,
        color: ink,
      ),
      bodySmall: TextStyle(
        fontFamily: kFont,
        fontSize: 13,
        height: 1.4,
        color: ink2,
      ),
      titleMedium: TextStyle(
        fontFamily: kFont,
        fontSize: 16,
        fontWeight: FontWeight.w500,
        color: ink,
      ),
    ),
    appBarTheme: AppBarTheme(
      backgroundColor: ground,
      surfaceTintColor: Colors.transparent,
      centerTitle: false,
      elevation: 0,
      scrolledUnderElevation: 0,
      systemOverlayStyle: overlay,
      titleTextStyle: kTitle.copyWith(color: ink),
      iconTheme: IconThemeData(color: ink, size: 22),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: ButtonStyle(
        minimumSize: const WidgetStatePropertyAll(Size(0, 52)),
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: 22),
        ),
        textStyle: const WidgetStatePropertyAll(buttonText),
        shape: WidgetStatePropertyAll(pill),
        backgroundColor: WidgetStateProperty.resolveWith((s) {
          if (s.contains(WidgetState.disabled)) return sunken;
          if (s.contains(WidgetState.pressed)) return kAccentDeep;
          return scheme.primary;
        }),
        foregroundColor: WidgetStateProperty.resolveWith(
          (s) => s.contains(WidgetState.disabled) ? ink2 : scheme.onPrimary,
        ),
        overlayColor: const WidgetStatePropertyAll(Colors.transparent),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: ButtonStyle(
        minimumSize: const WidgetStatePropertyAll(Size(0, 52)),
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: 22),
        ),
        textStyle: const WidgetStatePropertyAll(buttonText),
        shape: WidgetStatePropertyAll(pill),
        foregroundColor: WidgetStateProperty.resolveWith(
          (s) => s.contains(WidgetState.disabled)
              ? ink2.withValues(alpha: 0.6)
              : ink,
        ),
        side: WidgetStatePropertyAll(BorderSide(color: line, width: 1.5)),
        backgroundColor: WidgetStateProperty.resolveWith(
          (s) => s.contains(WidgetState.pressed) ? sunken : Colors.transparent,
        ),
        overlayColor: const WidgetStatePropertyAll(Colors.transparent),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: ButtonStyle(
        textStyle: const WidgetStatePropertyAll(buttonText),
        foregroundColor: WidgetStatePropertyAll(scheme.primary),
        shape: WidgetStatePropertyAll(pill),
        overlayColor: const WidgetStatePropertyAll(Colors.transparent),
      ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: ButtonStyle(
        foregroundColor: WidgetStatePropertyAll(ink),
        overlayColor: const WidgetStatePropertyAll(Colors.transparent),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: sunken,
      contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 14),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(kRadiusS),
        borderSide: BorderSide.none,
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(kRadiusS),
        borderSide: const BorderSide(color: kBrand, width: 2),
      ),
      counterStyle: TextStyle(color: ink2, fontSize: 12),
    ),
    dividerTheme: DividerThemeData(color: line, space: 1, thickness: 1),
    listTileTheme: ListTileThemeData(iconColor: ink2, textColor: ink),
    sliderTheme: SliderThemeData(
      activeTrackColor: scheme.primary,
      inactiveTrackColor: sunken,
      thumbColor: scheme.primary,
      overlayColor: scheme.primary.withValues(alpha: 0.12),
      trackHeight: 4,
      valueIndicatorColor: ink,
      valueIndicatorTextStyle: const TextStyle(
        fontFamily: kFont,
        color: Colors.white,
        fontFeatures: kFigures,
      ),
      overlayShape: const RoundSliderOverlayShape(overlayRadius: 18),
    ),
    switchTheme: SwitchThemeData(
      thumbColor: WidgetStateProperty.resolveWith(
        (s) => s.contains(WidgetState.selected) ? Colors.white : raised,
      ),
      trackColor: WidgetStateProperty.resolveWith(
        (s) => s.contains(WidgetState.selected) ? scheme.primary : line,
      ),
      trackOutlineColor: const WidgetStatePropertyAll(Colors.transparent),
    ),
    snackBarTheme: SnackBarThemeData(
      backgroundColor: kInk,
      contentTextStyle: const TextStyle(
        fontFamily: kFont,
        color: kNightInk,
        fontSize: 14,
      ),
      behavior: SnackBarBehavior.floating,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(kRadiusM),
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: scheme.primary),
  );
}

// --- structure ---------------------------------------------------------------

/// A titled section: a heading, then its content, separated from the next by
/// space rather than by a box. Boxing every group makes six unrelated things
/// look like six of the same thing.
class Panel extends StatelessWidget {
  const Panel({super.key, this.title, required this.child, this.padding});

  final String? title;
  final Widget child;
  final EdgeInsets? padding;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: padding ?? EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (title != null) ...[
            SectionLabel(title!),
            const SizedBox(height: 8),
          ],
          child,
        ],
      ),
    );
  }
}

/// A section heading in the title face. Not tracked capitals: those are a
/// label for a label, and the heading can carry its own weight.
class SectionLabel extends StatelessWidget {
  const SectionLabel(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: kHeading.copyWith(color: Theme.of(context).colorScheme.onSurface),
  );
}

/// A label/value row. Figures are tabular so a changing readout does not
/// shuffle its own digits sideways.
class StatRow extends StatelessWidget {
  const StatRow(this.label, this.value, {super.key, this.color});

  final String label;
  final String value;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final dim = scheme.brightness == Brightness.dark ? kNightInk2 : kInk2;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: TextStyle(color: dim, fontSize: 15)),
          Text(
            value,
            style: TextStyle(
              color: color ?? scheme.onSurface,
              fontSize: 15,
              fontFeatures: kFigures,
              fontWeight: FontWeight.w500,
            ),
          ),
        ],
      ),
    );
  }
}

/// Constrains body content to a readable column and centres it.
///
/// The phone is 832 dp wide in landscape. Without this, a label and its value
/// end up at opposite edges of the screen with 700 dp of nothing between them,
/// and the pair stops reading as one row.
class ContentColumn extends StatelessWidget {
  const ContentColumn({super.key, required this.child, this.maxWidth = 560});

  final Widget child;
  final double maxWidth;

  @override
  Widget build(BuildContext context) => Center(
    child: ConstrainedBox(
      constraints: BoxConstraints(maxWidth: maxWidth),
      child: child,
    ),
  );
}

/// A hairline between rows in a list. One pixel, not a gap and a shadow.
class Hairline extends StatelessWidget {
  const Hairline({super.key, this.indent = 0});

  final double indent;

  @override
  Widget build(BuildContext context) => Container(
    height: 1,
    margin: EdgeInsets.only(left: indent),
    color: Theme.of(context).colorScheme.outline,
  );
}

/// Scales down while pressed. The same 0.97 on everything pressable, so the
/// whole app answers a touch the same way.
class Pressable extends StatefulWidget {
  const Pressable({super.key, required this.onTap, required this.child});

  final VoidCallback? onTap;
  final Widget child;

  @override
  State<Pressable> createState() => _PressableState();
}

class _PressableState extends State<Pressable> {
  bool _down = false;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTapDown: widget.onTap == null
          ? null
          : (_) => setState(() => _down = true),
      onTapUp: (_) => setState(() => _down = false),
      onTapCancel: () => setState(() => _down = false),
      onTap: widget.onTap,
      child: AnimatedScale(
        scale: _down ? 0.97 : 1.0,
        duration: kFast,
        curve: kEase,
        child: widget.child,
      ),
    );
  }
}
