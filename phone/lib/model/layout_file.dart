/// Reading and writing `.phonepad` layout files.
///
/// The format is specified in `protocol/LAYOUT_FORMAT.md`. The rule that shapes
/// this file: **a layout is data, never code**. Importing one can change what
/// the controller looks like and how it responds, and nothing else.
///
/// [importLayout] is the security boundary, and it assumes the file is hostile.
/// It never trusts a number, never trusts a length, and never fails on
/// something it can safely repair — a layout with an absurd value is clamped
/// rather than rejected, because rejecting it teaches the user nothing and
/// clamping it cannot hurt them.
library;

import 'dart:convert';
import 'dart:math' as math;

import 'layout.dart';

const kLayoutFormat = 'phonepad.layout';
const kLayoutFormatVersion = 1;

/// Beyond this a layout is not a layout, and rendering it would stall the app.
const _maxControls = 64;
const _maxLabel = 8;
const _maxName = 48;
const _maxNotes = 280;

class LayoutFileError implements Exception {
  LayoutFileError(this.message);
  final String message;
  @override
  String toString() => message;
}

/// Serialise a layout for sharing. Deterministic: the same layout always
/// produces the same bytes, so two files can be diffed.
String exportLayout(ControllerLayout layout, {String? author, String? notes}) {
  final map = <String, dynamic>{
    'format': kLayoutFormat,
    'formatVersion': kLayoutFormatVersion,
    'id': layout.id,
    'name': layout.name,
    if (author != null && author.isNotEmpty) 'author': author,
    if (notes != null && notes.isNotEmpty) 'notes': notes,
    'landscape': layout.landscape.map((c) => c.toJson()).toList(),
    'portrait': layout.portrait.map((c) => c.toJson()).toList(),
  };
  return const JsonEncoder.withIndent('  ').convert(map);
}

/// Parse and sanitise a shared layout.
///
/// Throws [LayoutFileError] only for a file this version cannot safely read at
/// all. Everything else is repaired.
ControllerLayout importLayout(String source) {
  final Object? raw;
  try {
    raw = jsonDecode(source);
  } on FormatException {
    throw LayoutFileError('This file is not a PhonePad layout.');
  }

  if (raw is! Map<String, dynamic>) {
    throw LayoutFileError('This file is not a PhonePad layout.');
  }
  if (raw['format'] != kLayoutFormat) {
    throw LayoutFileError('This file is not a PhonePad layout.');
  }

  final version = (raw['formatVersion'] as num?)?.toInt() ?? 0;
  if (version > kLayoutFormatVersion) {
    throw LayoutFileError(
      'This layout was made with a newer version of PhonePad. Update the app '
      'and try again.',
    );
  }

  final landscape = _controls(raw['landscape']);
  final portrait = _controls(raw['portrait']);
  if (landscape.isEmpty && portrait.isEmpty) {
    throw LayoutFileError('This layout has no controls in it.');
  }

  return ControllerLayout(
    // A fresh id, always. Honouring the file's own id would let a shared layout
    // silently overwrite one of the user's, which is the sort of thing that
    // should not depend on the goodwill of whoever made the file.
    id: 'imported-${DateTime.now().millisecondsSinceEpoch.toRadixString(36)}',
    name: _text(raw['name'], _maxName, fallback: 'Imported layout'),
    landscape: landscape.isEmpty ? portrait.map((c) => c.copy()).toList() : landscape,
    portrait: portrait.isEmpty ? landscape.map((c) => c.copy()).toList() : portrait,
  );
}

/// Author and notes, if the file carried them. Separate from the layout itself
/// because they are provenance, not configuration.
({String? author, String? notes}) layoutCredits(String source) {
  try {
    final raw = jsonDecode(source);
    if (raw is! Map<String, dynamic>) return (author: null, notes: null);
    final a = raw['author'];
    final n = raw['notes'];
    return (
      author: a is String ? _text(a, _maxName, fallback: '') : null,
      notes: n is String ? _text(n, _maxNotes, fallback: '') : null,
    );
  } catch (_) {
    return (author: null, notes: null);
  }
}

List<ControlSpec> _controls(Object? raw) {
  if (raw is! List) return const [];
  final out = <ControlSpec>[];
  final seen = <String>{};

  for (final entry in raw.take(_maxControls)) {
    if (entry is! Map<String, dynamic>) continue;
    final spec = _control(entry);
    if (spec == null) continue;
    // Duplicate ids would make the editor's selection ambiguous.
    if (!seen.add(spec.id)) {
      spec.id = '${spec.id}-${seen.length}';
      seen.add(spec.id);
    }
    out.add(spec);
  }
  return out;
}

ControlSpec? _control(Map<String, dynamic> j) {
  final type = _enumOf(j['type'], ControlType.values);
  if (type == null) return null;

  final id = _text(j['id'], 32, fallback: '');
  if (id.isEmpty) return null;

  return ControlSpec(
    id: id,
    type: type,
    mapping: _mapping(j['mapping']),
    x: _num(j['x'], 0.0, 1.0, 0.5),
    y: _num(j['y'], 0.0, 1.0, 0.5),
    size: _num(j['size'], 0.04, 0.6, 0.115),
    aspect: _num(j['aspect'], 0.4, 3.5, 1.0),
    rotation: _num(j['rotation'], -0.8, 0.8, 0.0),
    opacity: _num(j['opacity'], 0.05, 1.0, 0.85),
    visible: j['visible'] is bool ? j['visible'] as bool : true,
    label: _text(j['label'], _maxLabel, fallback: ''),
    shape: _enumOf(j['shape'], ControlShape.values) ?? ControlShape.circle,
    deadzone: _num(j['deadzone'], 0.0, 0.6, kStickNoiseFloor),
    antiDeadzone: _num(j['antiDeadzone'], 0.0, 0.5, kStickAntiDeadzone),
    saturation: _num(j['saturation'], 0.3, 1.0, kStickSaturation),
    sensitivity: _num(j['sensitivity'], 0.3, 2.5, 1.0),
    responseCurve: _num(j['responseCurve'], 0.5, 2.5, 1.0),
    circularRange: j['circularRange'] is bool ? j['circularRange'] as bool : true,
    floating: j['floating'] is bool ? j['floating'] as bool : true,
    drift: j['drift'] is bool ? j['drift'] as bool : true,
    analogSlide: j['analogSlide'] is bool ? j['analogSlide'] as bool : false,
  );
}

ControlMapping _mapping(Object? raw) {
  if (raw is! Map<String, dynamic>) return const ControlMapping();
  final buttons = (raw['buttons'] as num?)?.toInt() ?? 0;
  return ControlMapping(
    // Mask off bit 0x0800. It is reserved and the decoder rejects any packet
    // carrying it, so a layout that set it would make every input packet fail
    // on the PC — a shared file must not be able to do that. Every other bit,
    // including Guide at 0x0400, is a real button and is kept.
    buttons: buttons & 0xF7FF,
    stick: _side(raw['stick']),
    trigger: _side(raw['trigger']),
  );
}

StickSide? _side(Object? raw) => switch (raw) {
      'left' => StickSide.left,
      'right' => StickSide.right,
      _ => null,
    };

T? _enumOf<T extends Enum>(Object? raw, List<T> values) {
  if (raw is! String) return null;
  for (final v in values) {
    if (v.name == raw) return v;
  }
  return null;
}

/// A finite number pulled into range. NaN, infinity, strings and nulls all
/// become the default rather than propagating into layout maths.
double _num(Object? raw, double lo, double hi, double fallback) {
  if (raw is! num) return fallback;
  final v = raw.toDouble();
  if (v.isNaN || v.isInfinite) return fallback;
  return math.min(hi, math.max(lo, v));
}

/// Text with control characters removed and a hard length cap.
String _text(Object? raw, int max, {required String fallback}) {
  if (raw is! String) return fallback;
  final cleaned = raw.replaceAll(RegExp(r'[\x00-\x1F\x7F]'), '').trim();
  if (cleaned.isEmpty) return fallback;
  // Cut on a rune boundary: slicing a string mid-surrogate produces text no
  // renderer can lay out.
  final runes = cleaned.runes.toList();
  if (runes.length <= max) return cleaned;
  return String.fromCharCodes(runes.take(max));
}
