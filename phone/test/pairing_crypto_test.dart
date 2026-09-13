/// Cross-checks the Dart pairing crypto against the same deterministic vector
/// the Rust side asserts on.
///
/// Without this, a mismatch in X25519 clamping, HKDF salt/info handling or the
/// confirmation-tag layout would surface only as an unexplained "wrong pairing
/// code" on a real device, with nothing pointing at the cause.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:phonepad/core/protocol/pairing_crypto.dart';

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
  late Map<String, dynamic> v;

  setUpAll(() {
    final doc =
        jsonDecode(File('../protocol/vectors.json').readAsStringSync())
            as Map<String, dynamic>;
    v = (doc['cases'] as List)
        .cast<Map<String, dynamic>>()
        .firstWhere((c) => c['type'] == 'pairing');
  });

  test('X25519 public keys match the Rust vector', () async {
    final client = await PairingKeys.fromSeed(unhex(v['clientScalar'] as String));
    final server = await PairingKeys.fromSeed(unhex(v['serverScalar'] as String));
    expect(hex(client.publicKey), v['clientPub']);
    expect(hex(server.publicKey), v['serverPub']);
  });

  test('both sides derive the same token, matching Rust', () async {
    final client = await PairingKeys.fromSeed(unhex(v['clientScalar'] as String));
    final server = await PairingKeys.fromSeed(unhex(v['serverScalar'] as String));
    final deviceId = unhex(v['deviceId'] as String);

    final cs = await client.agree(server.publicKey, deviceId);
    final ss = await server.agree(client.publicKey, deviceId);

    expect(hex(cs.token), hex(ss.token), reason: 'sides disagree');
    expect(hex(cs.token), v['token'], reason: 'differs from Rust');
  });

  test('confirmation tags match the Rust vector', () async {
    final client = await PairingKeys.fromSeed(unhex(v['clientScalar'] as String));
    final server = await PairingKeys.fromSeed(unhex(v['serverScalar'] as String));
    final deviceId = unhex(v['deviceId'] as String);
    final code = v['code'] as String;

    final cs = await client.agree(server.publicKey, deviceId);

    expect(
      hex(cs.serverConfirm(client.publicKey, server.publicKey, code)),
      v['serverConfirm'],
    );
    expect(
      hex(cs.clientConfirm(client.publicKey, server.publicKey, code)),
      v['clientConfirm'],
    );
  });

  test('a wrong code produces a different tag', () async {
    final client = await PairingKeys.fromSeed(unhex(v['clientScalar'] as String));
    final server = await PairingKeys.fromSeed(unhex(v['serverScalar'] as String));
    final cs = await client.agree(server.publicKey, unhex(v['deviceId'] as String));

    final right = cs.serverConfirm(client.publicKey, server.publicKey, '314159');
    final wrong = cs.serverConfirm(client.publicKey, server.publicKey, '314158');
    expect(hex(right), isNot(hex(wrong)));
  });

  test('session key matches the Rust vector', () async {
    final key = deriveSessionKey(
      unhex(v['token'] as String),
      unhex(v['clientNonce'] as String),
      unhex(v['serverNonce'] as String),
    );
    expect(hex(key), v['sessionKey']);
  });

  test('different nonces give different session keys', () {
    final token = unhex(v['token'] as String);
    final a = deriveSessionKey(token, Uint8List(16), Uint8List(16));
    final b = deriveSessionKey(token, Uint8List(16)..[0] = 1, Uint8List(16));
    expect(hex(a), isNot(hex(b)));
  });

  test('randomBytes returns the requested length and varies', () {
    final a = randomBytes(16);
    final b = randomBytes(16);
    expect(a.length, 16);
    expect(hex(a), isNot(hex(b)));
  });
}
