/// Asserts the Dart codec produces exactly the bytes in `protocol/vectors.json`.
///
/// The Rust codec asserts against the same file. Together these two tests are
/// what guarantee the phone and the companion agree on the wire format — a
/// mismatch fails here rather than as mysterious "bad MAC" counters at runtime.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/core/protocol/packets.dart';

Uint8List unhex(String s) {
  final out = Uint8List(s.length ~/ 2);
  for (var i = 0; i < out.length; i++) {
    out[i] = int.parse(s.substring(i * 2, i * 2 + 2), radix: 16);
  }
  return out;
}

String hex(List<int> b) =>
    b.map((x) => x.toRadixString(16).padLeft(2, '0')).join();

void main() {
  late Map<String, dynamic> doc;

  setUpAll(() {
    final file = File('../protocol/vectors.json');
    expect(
      file.existsSync(),
      isTrue,
      reason: 'run: cargo run -p pp-protocol --example gen_vectors',
    );
    doc = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
  });

  test('vectors were generated for this protocol version', () {
    expect(doc['protocolVersion'], kVersion);
  });

  test('every committed vector round-trips', () {
    final cases = (doc['cases'] as List).cast<Map<String, dynamic>>();
    expect(cases, isNotEmpty);

    var checked = 0;
    for (final c in cases) {
      final name = c['name'] as String;
      final expected = unhex(c['bytes'] as String);
      late Uint8List actual;

      switch (c['type'] as String) {
        case 'input':
          final key = unhex(c['sessionKey'] as String);
          final state = ControllerState()
            ..buttons = c['buttons'] as int
            ..lx = c['lx'] as int
            ..ly = c['ly'] as int
            ..rx = c['rx'] as int
            ..ry = c['ry'] as int
            ..lt = c['lt'] as int
            ..rt = c['rt'] as int;
          actual = Uint8List.fromList(
            InputEncoder(key).encode(
              sessionId: c['sessionId'] as int,
              seq: c['seq'] as int,
              clientTimeMs: c['clientTimeMs'] as int,
              rttUs: c['rttUs'] as int,
              flags: c['flags'] as int,
              state: state,
            ),
          );

          // Decoding must recover exactly what was encoded. The extremes vector
          // deliberately sets an axis to -32767, right next to the value the
          // decoder rejects, so this also pins that boundary.
          final back = decodeInput(actual, key);
          expect(back.sessionId, c['sessionId'], reason: name);
          expect(back.seq, c['seq'], reason: name);
          expect(back.rttUs, c['rttUs'], reason: name);
          expect(back.state.buttons, state.buttons, reason: name);
          expect(back.state.lx, state.lx, reason: name);
          expect(back.state.ly, state.ly, reason: name);
          expect(back.state.rx, state.rx, reason: name);
          expect(back.state.ry, state.ry, reason: name);
          expect(back.state.lt, state.lt, reason: name);
          expect(back.state.rt, state.rt, reason: name);

        case 'feedback':
          final key = unhex(c['sessionKey'] as String);
          actual = encodeFeedback(
            FeedbackPacket(
              sessionId: c['sessionId'] as int,
              echoClientTimeMs: c['echoClientTimeMs'] as int,
              rumbleLarge: c['rumbleLarge'] as int,
              rumbleSmall: c['rumbleSmall'] as int,
              acceptedPps: c['acceptedPps'] as int,
              lossPermille: c['lossPermille'] as int,
            ),
            key,
          );
          final back = decodeFeedback(actual, key);
          expect(back.echoClientTimeMs, c['echoClientTimeMs'], reason: name);
          expect(back.rumbleLarge, c['rumbleLarge'], reason: name);

        case 'sessionReq':
          actual = encodeSessionReq(
            deviceId: unhex(c['deviceId'] as String),
            clientNonce: unhex(c['clientNonce'] as String),
            token: unhex(c['token'] as String),
          );

        case 'sessionResp':
          // Encoded by the PC; the phone only decodes it.
          final resp = decodeSessionResp(expected, unhex(c['token'] as String));
          expect(resp.status, SessionStatus.ok, reason: name);
          expect(resp.sessionId, c['sessionId'], reason: name);
          expect(hex(resp.serverNonce), c['serverNonce'], reason: name);
          actual = expected;

        case 'bye':
          actual = encodeBye(
            c['sessionId'] as int,
            unhex(c['sessionKey'] as String),
          );

        case 'discoverReq':
          actual = encodeDiscoverReq(
            deviceId: unhex(c['deviceId'] as String),
            nonce: c['nonce'] as int,
            name: c['deviceName'] as String,
          );

        case 'discoverResp':
          final resp = decodeDiscoverResp(expected);
          expect(resp.nonce, c['nonce'], reason: name);
          expect(resp.inputPort, c['inputPort'], reason: name);
          expect(resp.alreadyPaired, c['alreadyPaired'], reason: name);
          expect(resp.pairingMode, c['pairingMode'], reason: name);
          expect(resp.hostname, c['hostname'], reason: name);
          expect(resp.backend, c['backend'], reason: name);
          actual = expected;

        case 'control':
          actual = encodeControl(
            sessionId: c['sessionId'] as int,
            controlSeq: c['controlSeq'] as int,
            command: ControlCommand.values
                .firstWhere((v) => v.code == c['command'] as int),
            sessionKey: unhex(c['sessionKey'] as String),
          );

        case 'pairing':
          // Not a wire-format case; covered byte-for-byte in
          // pairing_crypto_test.dart against the same fixture.
          checked++;
          continue;

        default:
          fail('$name: unknown vector type ${c['type']}');
      }

      expect(hex(actual), hex(expected), reason: '$name: bytes differ');
      checked++;
    }

    expect(checked, cases.length);
  });

  group('rejection', () {
    final key = unhex('5a' * 32);

    Uint8List sample() {
      final state = ControllerState()
        ..buttons = Btn.a
        ..lx = 1000
        ..lt = 50;
      return Uint8List.fromList(
        InputEncoder(key).encode(
          sessionId: 7,
          seq: 3,
          clientTimeMs: 100,
          rttUs: 0,
          flags: 0,
          state: state,
        ),
      );
    }

    test('a flipped bit fails the MAC', () {
      for (var byte = 0; byte < 32; byte++) {
        final buf = sample();
        buf[byte] ^= 0x01;
        // Byte 0-2 break the header check first; either way it must be refused.
        expect(() => decodeInput(buf, key), throwsA(isA<ProtocolException>()),
            reason: 'byte $byte was accepted after tampering');
      }
    });

    test('the wrong key fails', () {
      expect(
        () => decodeInput(sample(), unhex('00' * 32)),
        throwsA(isA<ProtocolException>()),
      );
    });

    test('truncation at any length is refused, never a crash', () {
      final full = sample();
      for (var len = 0; len < full.length; len++) {
        expect(
          () => decodeInput(Uint8List.sublistView(full, 0, len), key),
          throwsA(isA<ProtocolException>()),
          reason: 'length $len was accepted',
        );
      }
    });

    test('peekType only recognises well-formed headers', () {
      expect(peekType(sample()), Msg.input);
      expect(peekType(Uint8List.fromList([])), isNull);
      expect(peekType(Uint8List.fromList([0x50, 99, 0x10, 0])), isNull);
      expect(peekType(Uint8List.fromList([0x58, 1, 0x10, 0])), isNull);
    });

    test('names longer than the limit are cut on a rune boundary', () {
      // 'é' is two bytes, so a naive 64-byte cut would split the last one.
      final bytes = encodeDiscoverReq(
        deviceId: Uint8List(16),
        nonce: 0,
        name: 'é' * 50,
      );
      final nameLen = bytes[24];
      expect(nameLen, lessThanOrEqualTo(kMaxNameLen));
      expect(nameLen.isEven, isTrue, reason: 'cut mid-rune');
    });
  });

  test('controller state helpers behave', () {
    final s = ControllerState();
    expect(s.isNeutral, isTrue);
    s.setButton(Btn.a, true);
    expect(s.buttons, Btn.a);
    expect(s.isNeutral, isFalse);
    s.setButton(Btn.b, true);
    expect(s.buttons, Btn.a | Btn.b);
    s.setButton(Btn.a, false);
    expect(s.buttons, Btn.b);

    final t = ControllerState()..copyFrom(s);
    expect(t.sameAs(s), isTrue);
    t.lx = 5;
    expect(t.sameAs(s), isFalse);
    t.reset();
    expect(t.isNeutral, isTrue);
  });
}
