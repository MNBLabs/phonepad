/// The PhonePad mark, drawn from the same path data as `brand/mark.svg`.
///
/// It is stroked rather than filled because that is how it was designed: two
/// four-petal outlines at 0° and 45°, with a stroke wide enough that the arms
/// fill solid. Rendering it any other way changes the tips.
library;

import 'package:flutter/material.dart';

/// Path data copied verbatim from the Figma export, 128 x 128 units.
const _petals = [
  'M57.0607 78.6838L40.843 94.9025C37.1883 98.5572 32.4935 100.534 27.7043 100.832C28.0025 96.0434 29.9806 91.3489 33.635 87.6945L49.8527 71.4768L57.0607 78.6838ZM94.9025 87.6945C98.5569 91.3489 100.534 96.0434 100.832 100.832C96.0434 100.534 91.3489 98.5569 87.6945 94.9025L71.4768 78.6838L78.6838 71.4768L94.9025 87.6945ZM71.4758 64.2687L64.2688 71.4758L57.0607 64.2687L64.2688 57.0607L71.4758 64.2687ZM27.7043 27.7043C32.4935 28.0022 37.1883 29.9803 40.843 33.635L57.0607 49.8527L49.8527 57.0607L33.635 40.843C29.9804 37.1883 28.0022 32.4935 27.7043 27.7043ZM100.832 27.7043C100.534 32.4935 98.5572 37.1883 94.9025 40.843L78.6838 57.0607L71.4768 49.8527L87.6945 33.635C91.3489 29.9806 96.0434 28.0025 100.832 27.7043Z',
  'M69.096 79.2898L69.0967 102.226C69.0967 107.394 67.175 112.112 63.9992 115.709C60.8238 112.112 58.9031 107.394 58.903 102.226V79.2905L69.096 79.2898ZM102.226 58.9032C107.394 58.9032 112.111 60.8246 115.708 64C112.111 67.1754 107.394 69.0968 102.226 69.0968L79.2897 69.0961V58.9039L102.226 58.9032ZM69.096 58.9039V69.0961L58.903 69.0968V58.9032L69.096 58.9039ZM12.2899 64C15.8871 60.8242 20.6055 58.9032 25.774 58.9032H48.7094V69.0968H25.774C20.6055 69.0968 15.8871 67.1758 12.2899 64ZM63.9992 12.2907C67.175 15.888 69.0967 20.6056 69.0967 25.7741L69.096 48.7102L58.903 48.7095V25.7741C58.9031 20.606 60.8238 15.8878 63.9992 12.2907Z',
];

const _strokeWidth = 10.1935;

/// Parses the absolute M/L/H/V/C/Z subset the export uses. Anything else in
/// the data would be a change to the mark, and should fail loudly here.
Path _parse(String d) {
  final path = Path();
  final tokens = RegExp(
    r'[MLHVCZ]|-?[\d.]+(?:e-?\d+)?',
  ).allMatches(d).map((m) => m.group(0)!).toList();
  var i = 0;
  var cmd = '';
  double x = 0, y = 0;
  double next() => double.parse(tokens[i++]);

  while (i < tokens.length) {
    final t = tokens[i];
    if (RegExp(r'^[MLHVCZ]$').hasMatch(t)) {
      cmd = t;
      i++;
      if (cmd == 'Z') path.close();
      continue;
    }
    switch (cmd) {
      case 'M':
        x = next();
        y = next();
        path.moveTo(x, y);
        cmd = 'L';
      case 'L':
        x = next();
        y = next();
        path.lineTo(x, y);
      case 'H':
        x = next();
        path.lineTo(x, y);
      case 'V':
        y = next();
        path.lineTo(x, y);
      case 'C':
        final x1 = next(), y1 = next(), x2 = next(), y2 = next();
        x = next();
        y = next();
        path.cubicTo(x1, y1, x2, y2, x, y);
      default:
        throw StateError('unsupported path command in mark: $cmd');
    }
  }
  return path;
}

final Path _markPath = () {
  final p = Path();
  for (final d in _petals) {
    p.addPath(_parse(d), Offset.zero);
  }
  return p;
}();

class PhonePadMark extends StatelessWidget {
  const PhonePadMark({super.key, required this.size, required this.color});

  final double size;
  final Color color;

  @override
  Widget build(BuildContext context) =>
      CustomPaint(size: Size.square(size), painter: _MarkPainter(color));
}

class _MarkPainter extends CustomPainter {
  _MarkPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final scale = size.width / 128;
    canvas.scale(scale);
    canvas.drawPath(
      _markPath,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = _strokeWidth
        ..strokeJoin = StrokeJoin.miter
        ..color = color,
    );
  }

  @override
  bool shouldRepaint(_MarkPainter old) => old.color != color;
}

/// The app icon: the Figma colour frame, used as the designed asset rather
/// than reconstructed.
class BrandTile extends StatelessWidget {
  const BrandTile({super.key, required this.size});

  final double size;

  @override
  Widget build(BuildContext context) => ClipRRect(
    borderRadius: BorderRadius.circular(size * 0.22),
    child: Image.asset(
      'assets/brand/tile.png',
      width: size,
      height: size,
      fit: BoxFit.cover,
      filterQuality: FilterQuality.medium,
    ),
  );
}
