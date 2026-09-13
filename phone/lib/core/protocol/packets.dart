/// PhonePad wire protocol v1 — Dart side.
///
/// This mirrors `companion/crates/pp-protocol` byte for byte. Both are asserted
/// against `protocol/vectors.json`, so the two cannot drift apart silently.
/// See `protocol/PROTOCOL.md` for the authoritative layout.
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

const int kMagic = 0x50; // 'P'
const int kVersion = 1;

const int kDiscoveryPort = 47800;
const int kInputPort = 47801;

/// Message type codes.
abstract final class Msg {
  static const int discoverReq = 0x01;
  static const int discoverResp = 0x02;
  static const int pairReq = 0x03;
  static const int pairResp = 0x04;
  static const int sessionReq = 0x05;
  static const int sessionResp = 0x06;
  static const int pairConfirm = 0x07;
  static const int pairResult = 0x08;
  static const int input = 0x10;
  static const int feedback = 0x11;
  static const int bye = 0x12;
  static const int control = 0x13;
}

/// Out-of-band actions the phone can ask the PC to perform.
///
/// These exist because the companion window is unreachable behind a fullscreen
/// game — the moment you most need to fix the controller is the moment you
/// cannot get to the PC's UI.
enum ControlCommand {
  reattachPad(1),
  releaseAll(2);

  const ControlCommand(this.code);
  final int code;
}

/// Button bits. Identical to XInput's `wButtons`, so the companion copies the
/// field straight through with no translation.
abstract final class Btn {
  static const int dpadUp = 0x0001;
  static const int dpadDown = 0x0002;
  static const int dpadLeft = 0x0004;
  static const int dpadRight = 0x0008;
  static const int start = 0x0010;
  static const int back = 0x0020;
  static const int ls = 0x0040;
  static const int rs = 0x0080;
  static const int lb = 0x0100;
  static const int rb = 0x0200;
  static const int guide = 0x0400;
  static const int a = 0x1000;
  static const int b = 0x2000;
  static const int x = 0x4000;
  static const int y = 0x8000;

  /// Carries no meaning in v1; the companion rejects packets that set it.
  static const int reserved = 0x0800;
}

const int kInputLen = 40;
const int kFeedbackLen = 28;
const int kByeLen = 16;
const int kSessionReqLen = 44;
const int kSessionRespLen = 33;
const int kPairRespLen = 69;
const int kPairConfirmLen = 52;
const int kPairResultLen = 5;
const int kMaxNameLen = 64;

/// Truncated HMAC-SHA256 tag, as carried on the wire.
Uint8List mac8(List<int> key, Uint8List data) {
  final full = Hmac(sha256, key).convert(data).bytes;
  return Uint8List.fromList(full.sublist(0, 8));
}

/// Constant-time comparison. `==` would leak timing information about how much
/// of a forged tag was correct.
bool constantTimeEquals(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  var diff = 0;
  for (var i = 0; i < a.length; i++) {
    diff |= a[i] ^ b[i];
  }
  return diff == 0;
}

bool _verifyMac(List<int> key, Uint8List buf, int macOffset) {
  final body = Uint8List.sublistView(buf, 0, macOffset);
  final tag = Uint8List.sublistView(buf, macOffset, macOffset + 8);
  return constantTimeEquals(mac8(key, body), tag);
}

class ProtocolException implements Exception {
  final String message;
  const ProtocolException(this.message);
  @override
  String toString() => 'ProtocolException: $message';
}

/// Read the message type without validating anything else, so a receiver can
/// route a datagram before it knows which key applies. Returns null if this is
/// not a PhonePad packet at all.
int? peekType(Uint8List buf) {
  if (buf.length < 4 || buf[0] != kMagic || buf[1] != kVersion) return null;
  return buf[2];
}

void _checkHeader(Uint8List buf, int expectedType, int minLen) {
  if (buf.length < minLen) {
    throw ProtocolException(
      'packet too short: expected $minLen bytes, got ${buf.length}',
    );
  }
  if (buf[0] != kMagic) {
    throw ProtocolException('bad magic 0x${buf[0].toRadixString(16)}');
  }
  if (buf[1] != kVersion) {
    throw ProtocolException('unsupported protocol version ${buf[1]}');
  }
  if (buf[2] != expectedType) {
    throw ProtocolException(
      'wrong message type: expected 0x${expectedType.toRadixString(16)}, '
      'got 0x${buf[2].toRadixString(16)}',
    );
  }
}

void _putHeader(ByteData view, int type, int flags) {
  view.setUint8(0, kMagic);
  view.setUint8(1, kVersion);
  view.setUint8(2, type);
  view.setUint8(3, flags);
}

/// UTF-8 encode, truncated to [kMaxNameLen] bytes without splitting a rune.
Uint8List _encodeName(String name) {
  var bytes = utf8.encode(name);
  if (bytes.length <= kMaxNameLen) return Uint8List.fromList(bytes);
  var end = kMaxNameLen;
  // Back off until we are not in the middle of a multi-byte sequence.
  while (end > 0 && (bytes[end] & 0xC0) == 0x80) {
    end--;
  }
  return Uint8List.fromList(bytes.sublist(0, end));
}

/// Reads a length-prefixed name, advancing [cursor].
String _readName(Uint8List buf, _Cursor cursor) {
  if (cursor.at >= buf.length) {
    throw const ProtocolException('truncated: missing name length');
  }
  final len = buf[cursor.at++];
  if (len > kMaxNameLen) {
    throw ProtocolException('name too long: $len');
  }
  if (cursor.at + len > buf.length) {
    throw const ProtocolException('truncated: name shorter than declared');
  }
  final s = utf8.decode(
    buf.sublist(cursor.at, cursor.at + len),
    allowMalformed: true,
  );
  cursor.at += len;
  return s;
}

class _Cursor {
  int at;
  _Cursor(this.at);
}

// --- controller state --------------------------------------------------------

/// The complete controller state carried by one INPUT packet.
///
/// Mutable and reused: this is written on every touch event and read up to 250
/// times a second, so allocating a fresh object each time would create real GC
/// pressure on the hot path.
class ControllerState {
  int buttons = 0;
  int lx = 0;
  int ly = 0;
  int rx = 0;
  int ry = 0;
  int lt = 0;
  int rt = 0;

  ControllerState();

  bool get isNeutral =>
      buttons == 0 && lx == 0 && ly == 0 && rx == 0 && ry == 0 && lt == 0 && rt == 0;

  void reset() {
    buttons = 0;
    lx = 0;
    ly = 0;
    rx = 0;
    ry = 0;
    lt = 0;
    rt = 0;
  }

  void copyFrom(ControllerState o) {
    buttons = o.buttons;
    lx = o.lx;
    ly = o.ly;
    rx = o.rx;
    ry = o.ry;
    lt = o.lt;
    rt = o.rt;
  }

  bool sameAs(ControllerState o) =>
      buttons == o.buttons &&
      lx == o.lx &&
      ly == o.ly &&
      rx == o.rx &&
      ry == o.ry &&
      lt == o.lt &&
      rt == o.rt;

  void setButton(int mask, bool down) {
    if (down) {
      buttons |= mask;
    } else {
      buttons &= ~mask;
    }
  }

  @override
  String toString() =>
      'ControllerState(btn=0x${buttons.toRadixString(16)}, '
      'L=($lx,$ly) R=($rx,$ry) LT=$lt RT=$rt)';
}

// --- INPUT -------------------------------------------------------------------

/// Encoder that owns its buffer, so the hot path allocates nothing per packet.
class InputEncoder {
  final Uint8List buffer = Uint8List(kInputLen);
  late final ByteData _view = ByteData.sublistView(buffer);
  late final Uint8List _body = Uint8List.sublistView(buffer, 0, 32);
  final Hmac _hmac;

  /// Kept so the same session key can verify inbound FEEDBACK and sign BYE,
  /// rather than being threaded around separately.
  final Uint8List sessionKey;

  InputEncoder(List<int> key)
      : sessionKey = Uint8List.fromList(key),
        _hmac = Hmac(sha256, key);

  /// Fills [buffer] and returns it. The result is only valid until the next
  /// call — callers send it immediately.
  Uint8List encode({
    required int sessionId,
    required int seq,
    required int clientTimeMs,
    required int rttUs,
    required int flags,
    required ControllerState state,
  }) {
    _putHeader(_view, Msg.input, flags);
    _view.setUint32(4, sessionId, Endian.little);
    _view.setUint32(8, seq, Endian.little);
    _view.setUint32(12, clientTimeMs, Endian.little);
    _view.setUint16(16, state.buttons, Endian.little);
    _view.setInt16(18, state.lx, Endian.little);
    _view.setInt16(20, state.ly, Endian.little);
    _view.setInt16(22, state.rx, Endian.little);
    _view.setInt16(24, state.ry, Endian.little);
    _view.setUint8(26, state.lt);
    _view.setUint8(27, state.rt);
    _view.setUint32(28, rttUs, Endian.little);

    final tag = _hmac.convert(_body).bytes;
    for (var i = 0; i < 8; i++) {
      buffer[32 + i] = tag[i];
    }
    return buffer;
  }
}

/// Decoded INPUT packet. Only used by tests and the loopback tools — the phone
/// sends these, it does not receive them.
class InputPacket {
  final int sessionId;
  final int seq;
  final int clientTimeMs;
  final int rttUs;
  final int flags;
  final ControllerState state;

  InputPacket({
    required this.sessionId,
    required this.seq,
    required this.clientTimeMs,
    required this.rttUs,
    required this.flags,
    required this.state,
  });
}

InputPacket decodeInput(Uint8List buf, List<int> sessionKey) {
  _checkHeader(buf, Msg.input, kInputLen);
  if (!_verifyMac(sessionKey, buf, 32)) {
    throw const ProtocolException('MAC verification failed');
  }
  final v = ByteData.sublistView(buf);
  final buttons = v.getUint16(16, Endian.little);
  if (buttons & Btn.reserved != 0) {
    throw const ProtocolException('reserved button bit set');
  }
  final state = ControllerState()
    ..buttons = buttons
    ..lx = v.getInt16(18, Endian.little)
    ..ly = v.getInt16(20, Endian.little)
    ..rx = v.getInt16(22, Endian.little)
    ..ry = v.getInt16(24, Endian.little)
    ..lt = v.getUint8(26)
    ..rt = v.getUint8(27);

  for (final axis in [state.lx, state.ly, state.rx, state.ry]) {
    if (axis == -32768) {
      throw const ProtocolException('axis at int16 minimum');
    }
  }

  return InputPacket(
    sessionId: v.getUint32(4, Endian.little),
    seq: v.getUint32(8, Endian.little),
    clientTimeMs: v.getUint32(12, Endian.little),
    rttUs: v.getUint32(28, Endian.little),
    flags: buf[3],
    state: state,
  );
}

// --- FEEDBACK ----------------------------------------------------------------

class FeedbackPacket {
  final int sessionId;
  final int echoClientTimeMs;
  final int rumbleLarge;
  final int rumbleSmall;
  final int acceptedPps;
  final int lossPermille;

  const FeedbackPacket({
    required this.sessionId,
    required this.echoClientTimeMs,
    required this.rumbleLarge,
    required this.rumbleSmall,
    required this.acceptedPps,
    required this.lossPermille,
  });
}

FeedbackPacket decodeFeedback(Uint8List buf, List<int> sessionKey) {
  _checkHeader(buf, Msg.feedback, kFeedbackLen);
  if (!_verifyMac(sessionKey, buf, 20)) {
    throw const ProtocolException('MAC verification failed');
  }
  final v = ByteData.sublistView(buf);
  return FeedbackPacket(
    sessionId: v.getUint32(4, Endian.little),
    echoClientTimeMs: v.getUint32(8, Endian.little),
    rumbleLarge: v.getUint8(12),
    rumbleSmall: v.getUint8(13),
    acceptedPps: v.getUint16(14, Endian.little),
    lossPermille: v.getUint16(16, Endian.little),
  );
}

Uint8List encodeFeedback(FeedbackPacket p, List<int> sessionKey) {
  final buf = Uint8List(kFeedbackLen);
  final v = ByteData.sublistView(buf);
  _putHeader(v, Msg.feedback, 0);
  v.setUint32(4, p.sessionId, Endian.little);
  v.setUint32(8, p.echoClientTimeMs, Endian.little);
  v.setUint8(12, p.rumbleLarge);
  v.setUint8(13, p.rumbleSmall);
  v.setUint16(14, p.acceptedPps, Endian.little);
  v.setUint16(16, p.lossPermille, Endian.little);
  buf.setRange(20, 28, mac8(sessionKey, Uint8List.sublistView(buf, 0, 20)));
  return buf;
}

// --- BYE ---------------------------------------------------------------------

Uint8List encodeBye(int sessionId, List<int> sessionKey) {
  final buf = Uint8List(kByeLen);
  final v = ByteData.sublistView(buf);
  _putHeader(v, Msg.bye, 0);
  v.setUint32(4, sessionId, Endian.little);
  v.setUint32(8, 0, Endian.little); // reserved
  buf.setRange(8, 16, mac8(sessionKey, Uint8List.sublistView(buf, 0, 8)));
  return buf;
}

// --- CONTROL -----------------------------------------------------------------

const int kControlLen = 24;

Uint8List encodeControl({
  required int sessionId,
  required int controlSeq,
  required ControlCommand command,
  required List<int> sessionKey,
}) {
  final buf = Uint8List(kControlLen);
  final v = ByteData.sublistView(buf);
  _putHeader(v, Msg.control, command.code);
  v.setUint32(4, sessionId, Endian.little);
  v.setUint32(8, controlSeq, Endian.little);
  v.setUint32(12, 0, Endian.little); // reserved
  buf.setRange(16, 24, mac8(sessionKey, Uint8List.sublistView(buf, 0, 16)));
  return buf;
}

// --- SESSION -----------------------------------------------------------------

Uint8List encodeSessionReq({
  required Uint8List deviceId,
  required Uint8List clientNonce,
  required List<int> token,
}) {
  final buf = Uint8List(kSessionReqLen);
  _putHeader(ByteData.sublistView(buf), Msg.sessionReq, 0);
  buf.setRange(4, 20, deviceId);
  buf.setRange(20, 36, clientNonce);
  buf.setRange(36, 44, mac8(token, Uint8List.sublistView(buf, 0, 36)));
  return buf;
}

enum SessionStatus {
  ok,
  unknownDevice,
  badMac,
  serverBusy,
  noBackend;

  static SessionStatus fromByte(int v) {
    if (v < 0 || v >= SessionStatus.values.length) {
      throw ProtocolException('unknown session status $v');
    }
    return SessionStatus.values[v];
  }
}

class SessionResp {
  final SessionStatus status;
  final int sessionId;
  final Uint8List serverNonce;

  const SessionResp({
    required this.status,
    required this.sessionId,
    required this.serverNonce,
  });
}

SessionResp decodeSessionResp(Uint8List buf, List<int> token) {
  _checkHeader(buf, Msg.sessionResp, kSessionRespLen);
  if (!_verifyMac(token, buf, 25)) {
    throw const ProtocolException('MAC verification failed');
  }
  final v = ByteData.sublistView(buf);
  return SessionResp(
    status: SessionStatus.fromByte(buf[4]),
    sessionId: v.getUint32(5, Endian.little),
    serverNonce: Uint8List.fromList(buf.sublist(9, 25)),
  );
}

// --- DISCOVERY ---------------------------------------------------------------

Uint8List encodeDiscoverReq({
  required Uint8List deviceId,
  required int nonce,
  required String name,
}) {
  final nameBytes = _encodeName(name);
  final buf = Uint8List(25 + nameBytes.length);
  final v = ByteData.sublistView(buf);
  _putHeader(v, Msg.discoverReq, 0);
  buf.setRange(4, 20, deviceId);
  v.setUint32(20, nonce, Endian.little);
  buf[24] = nameBytes.length;
  buf.setRange(25, 25 + nameBytes.length, nameBytes);
  return buf;
}

class DiscoverResp {
  final int nonce;
  final Uint8List serverId;
  final int inputPort;
  final bool alreadyPaired;
  final bool pairingMode;
  final String hostname;
  final String backend;

  const DiscoverResp({
    required this.nonce,
    required this.serverId,
    required this.inputPort,
    required this.alreadyPaired,
    required this.pairingMode,
    required this.hostname,
    required this.backend,
  });
}

DiscoverResp decodeDiscoverResp(Uint8List buf) {
  _checkHeader(buf, Msg.discoverResp, 28);
  final v = ByteData.sublistView(buf);
  final state = buf[26];
  final cursor = _Cursor(27);
  final hostname = _readName(buf, cursor);
  final backend = _readName(buf, cursor);
  return DiscoverResp(
    nonce: v.getUint32(4, Endian.little),
    serverId: Uint8List.fromList(buf.sublist(8, 24)),
    inputPort: v.getUint16(24, Endian.little),
    alreadyPaired: state & 1 != 0,
    pairingMode: state & 2 != 0,
    hostname: hostname,
    backend: backend,
  );
}

// --- PAIRING -----------------------------------------------------------------

Uint8List encodePairReq({
  required Uint8List deviceId,
  required Uint8List clientPub,
  required String name,
}) {
  final nameBytes = _encodeName(name);
  // 4 header + 16 id + 32 pubkey + 1 length + n name
  final buf = Uint8List(53 + nameBytes.length);
  _putHeader(ByteData.sublistView(buf), Msg.pairReq, 0);
  buf.setRange(4, 20, deviceId);
  buf.setRange(20, 52, clientPub);
  buf[52] = nameBytes.length;
  buf.setRange(53, 53 + nameBytes.length, nameBytes);
  return buf;
}

enum PairStatus {
  ok,
  notInPairingMode,
  codeMismatch,
  rejected;

  static PairStatus fromByte(int v) {
    if (v < 0 || v >= PairStatus.values.length) {
      throw ProtocolException('unknown pair status $v');
    }
    return PairStatus.values[v];
  }
}

class PairResp {
  final PairStatus status;
  final Uint8List serverPub;
  final Uint8List serverConfirm;

  const PairResp({
    required this.status,
    required this.serverPub,
    required this.serverConfirm,
  });
}

PairResp decodePairResp(Uint8List buf) {
  _checkHeader(buf, Msg.pairResp, kPairRespLen);
  return PairResp(
    status: PairStatus.fromByte(buf[4]),
    serverPub: Uint8List.fromList(buf.sublist(5, 37)),
    serverConfirm: Uint8List.fromList(buf.sublist(37, 69)),
  );
}

Uint8List encodePairConfirm({
  required Uint8List deviceId,
  required Uint8List clientConfirm,
}) {
  final buf = Uint8List(kPairConfirmLen);
  _putHeader(ByteData.sublistView(buf), Msg.pairConfirm, 0);
  buf.setRange(4, 20, deviceId);
  buf.setRange(20, 52, clientConfirm);
  return buf;
}

PairStatus decodePairResult(Uint8List buf) {
  _checkHeader(buf, Msg.pairResult, kPairResultLen);
  return PairStatus.fromByte(buf[4]);
}
