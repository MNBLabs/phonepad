/// The layout import path, treated as a security boundary.
///
/// A shared layout comes from a stranger. These tests are mostly about what a
/// hostile or broken file must *not* be able to do.
library;

import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/model/layout.dart';
import 'package:phonepad/model/layout_file.dart';
import 'package:phonepad/model/presets.dart';

String withControl(Map<String, dynamic> control) => jsonEncode({
      'format': kLayoutFormat,
      'formatVersion': 1,
      'name': 'test',
      'landscape': [
        {'id': 'c1', 'type': 'stick', 'mapping': {'stick': 'left'}, ...control},
      ],
      'portrait': <dynamic>[],
    });

ControlSpec firstOf(String source) => importLayout(source).landscape.first;

void main() {
  group('round trip', () {
    test('every preset survives export and import', () {
      for (final preset in builtInLayouts()) {
        final back = importLayout(exportLayout(preset));
        expect(back.landscape.length, preset.landscape.length,
            reason: preset.name);
        for (var i = 0; i < preset.landscape.length; i++) {
          final a = preset.landscape[i];
          final b = back.landscape[i];
          expect(b.id, a.id);
          expect(b.type, a.type);
          expect(b.x, closeTo(a.x, 1e-9));
          expect(b.size, closeTo(a.size, 1e-9));
          expect(b.antiDeadzone, closeTo(a.antiDeadzone, 1e-9));
          expect(b.mapping.buttons, a.mapping.buttons);
        }
      }
    });

    test('export is deterministic', () {
      final l = standardLayout();
      expect(exportLayout(l), exportLayout(standardLayout()));
    });

    test('an imported layout never keeps the id it came with', () {
      // Otherwise a shared file could silently replace one of the user's own.
      final source = exportLayout(standardLayout());
      final back = importLayout(source);
      expect(back.id, isNot('builtin-standard'));
      expect(back.id, startsWith('imported-'));
    });
  });

  group('rejects what it cannot safely read', () {
    test('not JSON', () {
      expect(() => importLayout('not json at all'), throwsA(isA<LayoutFileError>()));
    });

    test('JSON that is not a layout', () {
      expect(() => importLayout('{"hello":"world"}'),
          throwsA(isA<LayoutFileError>()));
      expect(() => importLayout('[1,2,3]'), throwsA(isA<LayoutFileError>()));
    });

    test('a future format version', () {
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 99,
        'landscape': [],
        'portrait': [],
      });
      expect(() => importLayout(s), throwsA(isA<LayoutFileError>()));
    });

    test('a layout with no controls', () {
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 1,
        'landscape': <dynamic>[],
        'portrait': <dynamic>[],
      });
      expect(() => importLayout(s), throwsA(isA<LayoutFileError>()));
    });
  });

  group('repairs rather than trusting', () {
    test('out-of-range numbers are clamped, not honoured', () {
      final c = firstOf(withControl({
        'x': 99.0,
        'y': -50.0,
        'size': 1000.0,
        'opacity': 7.0,
        'sensitivity': 500.0,
        'responseCurve': -3.0,
      }));
      expect(c.x, lessThanOrEqualTo(1.0));
      expect(c.y, greaterThanOrEqualTo(0.0));
      expect(c.size, lessThanOrEqualTo(0.6));
      expect(c.opacity, lessThanOrEqualTo(1.0));
      expect(c.sensitivity, lessThanOrEqualTo(2.5));
      expect(c.responseCurve, greaterThanOrEqualTo(0.5));
    });

    test('NaN and infinity do not reach the layout maths', () {
      // These arrive as JSON strings, since JSON has no way to write them.
      final c = firstOf(withControl({'x': 'NaN', 'size': 'Infinity'}));
      expect(c.x.isFinite, isTrue);
      expect(c.size.isFinite, isTrue);
    });

    test('wrong types fall back instead of throwing', () {
      final c = firstOf(withControl({
        'x': 'left a bit',
        'visible': 'yes',
        'shape': 'hexagon',
        'floating': 3,
      }));
      expect(c.x, 0.5);
      expect(c.visible, isTrue);
      expect(c.shape, ControlShape.circle);
      expect(c.floating, isTrue);
    });

    test('control characters are stripped from labels', () {
      // Written with escapes rather than literal bytes: a NUL typed straight
      // into a source file makes git treat it as binary, and it is invisible in
      // every diff after that.
      final c = firstOf(withControl({'label': 'A\u0000B\u0007C\nD'}));
      expect(c.label, 'ABCD');
    });

    test('labels are capped in length', () {
      final c = firstOf(withControl({'label': 'ABCDEFGHIJKLMNOP'}));
      expect(c.label.length, 8);
      expect(c.label, 'ABCDEFGH');
    });

    test('a label is cut on a rune boundary', () {
      final c = firstOf(withControl({'label': '👾👾👾👾👾👾👾👾👾👾👾👾'}));
      expect(c.label.runes.length, lessThanOrEqualTo(8));
      expect(c.label.runes.every((r) => r > 0xFFFF), isTrue,
          reason: 'no half surrogate survived the cut');
    });

    test('unknown button bits are masked off', () {
      // The reserved bit is rejected on the wire, so a layout that set it could
      // otherwise make every input packet fail to decode on the PC.
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 1,
        'landscape': [
          {'id': 'b', 'type': 'button', 'mapping': {'buttons': 0xFFFF}},
        ],
        'portrait': <dynamic>[],
      });
      expect(importLayout(s).landscape.first.mapping.buttons & 0x0800, 0);
    });

    test('unknown fields are ignored rather than rejected', () {
      final c = firstOf(withControl({'somethingNew': 42, 'x': 0.3}));
      expect(c.x, closeTo(0.3, 1e-9));
    });

    test('the control count is capped', () {
      final many = List.generate(
        500,
        (i) => {'id': 'c$i', 'type': 'button', 'mapping': {'buttons': 1}},
      );
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 1,
        'landscape': many,
        'portrait': <dynamic>[],
      });
      expect(importLayout(s).landscape.length, lessThanOrEqualTo(64));
    });

    test('duplicate ids are made unique', () {
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 1,
        'landscape': [
          {'id': 'same', 'type': 'button', 'mapping': {'buttons': 1}},
          {'id': 'same', 'type': 'button', 'mapping': {'buttons': 2}},
        ],
        'portrait': <dynamic>[],
      });
      final ids = importLayout(s).landscape.map((c) => c.id).toList();
      expect(ids.toSet().length, ids.length);
    });

    test('malformed controls are skipped, not fatal', () {
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 1,
        'landscape': [
          'a string',
          {'type': 'button'}, // no id
          {'id': 'ok', 'type': 'nonsense'}, // unknown type
          {'id': 'good', 'type': 'button', 'mapping': {'buttons': 1}},
        ],
        'portrait': <dynamic>[],
      });
      final out = importLayout(s).landscape;
      expect(out.length, 1);
      expect(out.first.id, 'good');
    });

    test('one missing orientation is filled from the other', () {
      final s = jsonEncode({
        'format': kLayoutFormat,
        'formatVersion': 1,
        'landscape': [
          {'id': 'a', 'type': 'button', 'mapping': {'buttons': 1}},
        ],
        'portrait': <dynamic>[],
      });
      final l = importLayout(s);
      expect(l.portrait.length, 1);
      // A copy, not the same object: editing one orientation must not silently
      // edit the other.
      expect(identical(l.portrait.first, l.landscape.first), isFalse);
    });
  });

  group('credits', () {
    test('author and notes come back', () {
      final s = exportLayout(standardLayout(), author: 'someone', notes: 'hi');
      final c = layoutCredits(s);
      expect(c.author, 'someone');
      expect(c.notes, 'hi');
    });

    test('a broken file yields no credits rather than throwing', () {
      final c = layoutCredits('{{{');
      expect(c.author, isNull);
      expect(c.notes, isNull);
    });
  });
}
