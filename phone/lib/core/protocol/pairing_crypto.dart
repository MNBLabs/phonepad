/// Pairing key agreement — the Dart half of what `pp-protocol/src/crypto.rs`
/// does on the PC.
///
/// X25519 ECDH authenticated by the 6-digit code the companion displays. The
/// code binds the exchange to the two public keys, so a device sniffing the LAN
/// cannot substitute its own key and sit in the middle.
///
/// Six digits only stay secret if nobody can test guesses offline, so each
/// side commits to its proof of the code before the other reveals one (pairing
/// v2; see protocol/PROTOCOL.md). The phone never reveals its proof until the
/// PC has opened a commitment it made before it could have learned the code.
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

/// `SHA-256(label || tag || nonce)`. Hiding while the nonce is secret; binding
/// because a commitment made without the code cannot later open to a proof of
/// it.
Uint8List _commitment(String label, List<int> tag, List<int> nonce) =>
    Uint8List.fromList(
      sha256.convert(<int>[...utf8.encode(label), ...tag, ...nonce]).bytes,
    );

/// The PC's commitment, carried in PAIR_RESP.
Uint8List serverCommitment(List<int> serverConfirm, List<int> nonce) =>
    _commitment('pp-commit-srv', serverConfirm, nonce);

/// The phone's commitment, carried in PAIR_COMMIT.
Uint8List clientCommitment(List<int> clientConfirm, List<int> nonce) =>
    _commitment('pp-commit-cli', clientConfirm, nonce);

/// Whether the PC's reveal proves it knew the code before it saw anything of
/// ours: it must open the commitment from PAIR_RESP, and the opened proof must
/// be the one for the code the user typed. Only then may the phone reveal its
/// own proof; revealing it to anything else would hand over the code.
bool serverRevealIsValid({
  required Uint8List serverCommit,
  required Uint8List revealedConfirm,
  required Uint8List revealedNonce,
  required Uint8List expectedConfirm,
}) {
  final opens = _constantTimeEquals(
    serverCommitment(revealedConfirm, revealedNonce),
    serverCommit,
  );
  final proves = _constantTimeEquals(revealedConfirm, expectedConfirm);
  return opens && proves;
}

bool _constantTimeEquals(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  var diff = 0;
  for (var i = 0; i < a.length; i++) {
    diff |= a[i] ^ b[i];
  }
  return diff == 0;
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
