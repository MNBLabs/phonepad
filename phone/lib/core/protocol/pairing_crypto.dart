/// Pairing key agreement — the Dart half of what `pp-protocol/src/crypto.rs`
/// does on the PC.
///
/// X25519 ECDH authenticated by the 6-digit code the companion displays. The
/// code binds the exchange to the two public keys, so a device sniffing the LAN
/// cannot substitute its own key and sit in the middle.
///
/// The resulting 32-byte token is derived independently on both sides and is
/// **never transmitted**.
library;

import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
// Both packages export `Hmac`; we use the synchronous one from `crypto` on the
// hot path and want only the X25519 pieces from here.
import 'package:cryptography/cryptography.dart'
    show KeyPairType, SimpleKeyPair, SimplePublicKey, X25519;

const _hkdfSalt = 'phonepad-v1';

/// HKDF-SHA256 producing exactly one 32-byte block, matching the Rust `hkdf`
/// crate's `Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut [u8; 32])`.
Uint8List hkdf32(List<int> ikm, List<int> salt, List<int> info) {
  // Extract.
  final prk = Hmac(sha256, salt).convert(ikm).bytes;
  // Expand, first block only: T(1) = HMAC(PRK, info || 0x01).
  final input = <int>[...info, 0x01];
  final okm = Hmac(sha256, prk).convert(input).bytes;
  return Uint8List.fromList(okm.sublist(0, 32));
}

/// Everything derived once both public keys are known.
class PairingSecrets {
  final Uint8List _confirmKey;

  /// The long-term credential for this phone/PC pair.
  final Uint8List token;

  const PairingSecrets._(this._confirmKey, this.token);

  Uint8List _confirm(
    String label,
    Uint8List clientPub,
    Uint8List serverPub,
    String code,
  ) {
    final msg = <int>[
      ...utf8.encode(label),
      ...clientPub,
      ...serverPub,
      ...utf8.encode(code),
    ];
    return Uint8List.fromList(Hmac(sha256, _confirmKey).convert(msg).bytes);
  }

  /// What the PC should have sent, if it knew the code the user typed.
  Uint8List serverConfirm(Uint8List clientPub, Uint8List serverPub, String code) =>
      _confirm('pp-srv', clientPub, serverPub, code);

  /// Our proof to the PC that we know the same code.
  Uint8List clientConfirm(Uint8List clientPub, Uint8List serverPub, String code) =>
      _confirm('pp-cli', clientPub, serverPub, code);
}

/// One side's X25519 keypair for a pairing exchange.
class PairingKeys {
  final SimpleKeyPair _keyPair;
  final Uint8List publicKey;

  PairingKeys._(this._keyPair, this.publicKey);

  static final _algorithm = X25519();

  static Future<PairingKeys> generate() async {
    final kp = await _algorithm.newKeyPair();
    final pub = await kp.extractPublicKey();
    return PairingKeys._(kp, Uint8List.fromList(pub.bytes));
  }

  /// Deterministic variant, used only by the conformance tests.
  static Future<PairingKeys> fromSeed(List<int> seed) async {
    final kp = await _algorithm.newKeyPairFromSeed(seed);
    final pub = await kp.extractPublicKey();
    return PairingKeys._(kp, Uint8List.fromList(pub.bytes));
  }

  /// Complete the Diffie–Hellman and derive the confirmation key and token.
  Future<PairingSecrets> agree(Uint8List theirPublic, Uint8List deviceId) async {
    final shared = await _algorithm.sharedSecretKey(
      keyPair: _keyPair,
      remotePublicKey: SimplePublicKey(theirPublic, type: KeyPairType.x25519),
    );
    final sharedBytes = await shared.extractBytes();

    final salt = utf8.encode(_hkdfSalt);
    final tokenInfo = <int>[...utf8.encode('token'), ...deviceId];

    return PairingSecrets._(
      hkdf32(sharedBytes, salt, utf8.encode('pair')),
      hkdf32(sharedBytes, salt, tokenInfo),
    );
  }
}

/// Session key for the input stream. A fresh one per session means a captured
/// stream cannot be replayed into a later session.
Uint8List deriveSessionKey(
  List<int> token,
  Uint8List clientNonce,
  Uint8List serverNonce,
) {
  final salt = Uint8List(32)
    ..setRange(0, 16, clientNonce)
    ..setRange(16, 32, serverNonce);
  return hkdf32(token, salt, utf8.encode('session'));
}

final _random = Random.secure();

Uint8List randomBytes(int n) {
  final out = Uint8List(n);
  for (var i = 0; i < n; i++) {
    out[i] = _random.nextInt(256);
  }
  return out;
}
