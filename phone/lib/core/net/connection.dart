/// Discovery, pairing, session management and the input hot path.
///
/// Everything runs on one UDP socket. The PC always replies unicast to the
/// source address, so the phone never has to receive a broadcast — which
/// sidesteps Android's multicast-lock behaviour entirely.
library;

import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../../platform/phonepad_platform.dart';
import '../protocol/packets.dart';
import '../protocol/pairing_crypto.dart';
import '../store.dart';

enum ConnectionPhase {
  idle,
  searching,
  connecting,
  connected,
  /// Connected recently, packets have stopped, trying to get back.
  reconnecting,
  failed,
}

class DiscoveredPc {
  final InternetAddress address;
  final int inputPort;
  final String serverId; // hex
  final String hostname;
  final String backend;
  final bool alreadyPaired;
  final bool pairingMode;

  const DiscoveredPc({
    required this.address,
    required this.inputPort,
    required this.serverId,
    required this.hostname,
    required this.backend,
    required this.alreadyPaired,
    required this.pairingMode,
  });
}

/// Live numbers for the diagnostics screen. All measured, none estimated.
class LinkStats {
  int sentPps = 0;
  int feedbackPps = 0;
  int rttUs = 0;
  int jitterUs = 0;
  int pcAcceptedPps = 0;
  int pcLossPermille = 0;
  int seq = 0;
  int rumbleLarge = 0;
  int rumbleSmall = 0;
  DateTime? lastFeedback;
}

/// How long without feedback before we assume the link is gone and start
/// trying to rebuild the session.
const _feedbackTimeout = Duration(milliseconds: 900);
const _reconnectInterval = Duration(seconds: 1);
const _discoveryTimeout = Duration(milliseconds: 1200);

/// Where a discovery probe goes.
///
/// * The subnet broadcast of every local link: the phone's Wi-Fi, and any
///   network the phone itself is providing (hotspot, USB or Bluetooth
///   tethering). Some access points drop 255.255.255.255 but forward these.
/// * 255.255.255.255, only when the default network is local. Otherwise the
///   kernel sends it over mobile data.
/// * Each paired PC's last address, only when it falls inside one of those
///   links, in case broadcast is filtered. A stale address from another network
///   would otherwise go to the carrier too.
Set<String> discoveryTargets(LinkInfo link, Iterable<String> lastAddresses) => {
      for (final n in link.networks) n.broadcast,
      if (link.defaultIsLocal) '255.255.255.255',
      for (final a in lastAddresses)
        if (link.networks.any((n) => n.contains(a))) a,
    };

class PhonePadConnection extends ChangeNotifier {
  PhonePadConnection(this._store);

  final Store _store;
  AppData get _data => _store.data;

  RawDatagramSocket? _socket;
  ConnectionPhase _phase = ConnectionPhase.idle;
  String? _error;

  PairedPc? _pc;
  InternetAddress? _pcAddress;
  int _inputPort = kInputPort;

  int _sessionId = 0;
  InputEncoder? _encoder;
  int _seq = 0;
  int _rttUs = 0;
  int _lastEchoMs = -1;

  final ControllerState state = ControllerState();
  final LinkStats stats = LinkStats();

  /// Called when the PC reports what the game wants the motors to do. Set by
  /// the controller screen, so rumble stops the moment that screen is left.
  void Function(int large, int small)? onRumble;

  Timer? _sendTimer;
  Timer? _housekeepingTimer;
  final Stopwatch _clock = Stopwatch()..start();
  DateTime? _lastFeedbackAt;
  int _sentThisSecond = 0;
  int _feedbackThisSecond = 0;
  DateTime _rateWindow = DateTime.now();
  double _smoothedRttUs = 0;
  double _jitterUs = 0;

  /// Pending one-shot replies, keyed by message type. Lets the async request
  /// helpers await a specific reply while the same socket keeps serving the
  /// input path.
  final Map<int, Completer<Uint8List>> _waiters = {};

  final List<DiscoveredPc> discovered = [];

  ConnectionPhase get phase => _phase;
  String? get error => _error;
  PairedPc? get connectedPc => _pc;
  bool get isConnected => _phase == ConnectionPhase.connected;
  int get rttUs => _rttUs;

  void _setPhase(ConnectionPhase p, {String? error}) {
    if (_phase == p && _error == error) return;
    _phase = p;
    _error = error;
    notifyListeners();
  }

  // --- socket ---------------------------------------------------------------

  Future<RawDatagramSocket> _ensureSocket() async {
    final existing = _socket;
    if (existing != null) return existing;

    final sock = await RawDatagramSocket.bind(InternetAddress.anyIPv4, 0);
    sock.broadcastEnabled = true;
    sock.listen(_onSocketEvent, onError: (Object e) {
      debugPrint('PhonePad: socket error $e');
    });
    _socket = sock;
    return sock;
  }

  /// Retries since the link went quiet. See [_housekeeping].
  int _reconnectAttempts = 0;

  /// Drop the socket so the next send binds a new one. Replies still awaited
  /// on the old socket can no longer arrive, so release their waiters now
  /// rather than leave them to time out.
  void _rebindSocket() {
    _socket?.close();
    _socket = null;
    for (final w in _waiters.values) {
      if (!w.isCompleted) w.complete(Uint8List(0));
    }
    _waiters.clear();
  }

  void _onSocketEvent(RawSocketEvent event) {
    if (event != RawSocketEvent.read) return;
    final sock = _socket;
    if (sock == null) return;

    // Drain everything available; under load several datagrams can be queued.
    for (Datagram? dg = sock.receive(); dg != null; dg = sock.receive()) {
      _handleDatagram(dg);
    }
  }

  void _handleDatagram(Datagram dg) {
    final buf = dg.data;
    final type = peekType(buf);
    if (type == null) return;

    if (type == Msg.feedback) {
      _handleFeedback(buf);
      return;
    }

    if (type == Msg.discoverResp) {
      _handleDiscoverResp(buf, dg.address);
      // Discovery is a broadcast fan-in: several PCs may answer, so this does
      // not complete a waiter.
      return;
    }

    final waiter = _waiters.remove(type);
    if (waiter != null && !waiter.isCompleted) {
      waiter.complete(buf);
    }
  }

  Future<Uint8List?> _awaitReply(int type, Duration timeout) {
    // A stale waiter for the same type would never be completed; replace it.
    _waiters.remove(type)?.complete(Uint8List(0));
    final c = Completer<Uint8List>();
    _waiters[type] = c;
    return c.future.timeout(timeout, onTimeout: () => Uint8List(0)).then((v) {
      _waiters.remove(type);
      return v.isEmpty ? null : v;
    });
  }

  // --- discovery ------------------------------------------------------------

  /// Probe every local link and collect answers. See [discoveryTargets].
  Future<List<DiscoveredPc>> discover({
    LinkInfo link = LinkInfo.empty,
    Duration timeout = _discoveryTimeout,
  }) async {
    final sock = await _ensureSocket();
    discovered.clear();
    _setPhase(ConnectionPhase.searching);

    final probe = encodeDiscoverReq(
      deviceId: _data.deviceIdBytes,
      nonce: DateTime.now().millisecondsSinceEpoch & 0x7FFFFFFF,
      name: _data.deviceName,
    );

    final targets = discoveryTargets(
      link,
      _data.pairedPcs.map((p) => p.lastAddress).whereType<String>(),
    );

    for (final t in targets) {
      final addr = InternetAddress.tryParse(t);
      if (addr == null) continue;
      try {
        sock.send(probe, addr, kDiscoveryPort);
      } catch (e) {
        debugPrint('PhonePad: probe to $t failed: $e');
      }
    }

    await Future<void>.delayed(timeout);
    if (_phase == ConnectionPhase.searching) {
      _setPhase(ConnectionPhase.idle);
    }
    notifyListeners();
    return List.unmodifiable(discovered);
  }

  void _handleDiscoverResp(Uint8List buf, InternetAddress from) {
    final DiscoverResp resp;
    try {
      resp = decodeDiscoverResp(buf);
    } catch (_) {
      return; // malformed; ignore rather than crash the socket handler
    }

    final serverId = hex(resp.serverId);
    final pc = DiscoveredPc(
      address: from,
      inputPort: resp.inputPort,
      serverId: serverId,
      hostname: resp.hostname,
      backend: resp.backend,
      alreadyPaired: resp.alreadyPaired,
      pairingMode: resp.pairingMode,
    );

    discovered.removeWhere((d) => d.serverId == serverId);
    discovered.add(pc);
    notifyListeners();
  }

  // --- pairing --------------------------------------------------------------

  /// Runs the full X25519 + code exchange (pairing v2: commit, then reveal).
  /// Returns null on success, or a human-readable reason.
  Future<String?> pair(DiscoveredPc pc, String code) async {
    final sock = await _ensureSocket();
    final deviceId = _data.deviceIdBytes;

    final keys = await PairingKeys.generate();
    sock.send(
      encodePairReq(
        deviceId: deviceId,
        clientPub: keys.publicKey,
        name: _data.deviceName,
      ),
      pc.address,
      kDiscoveryPort,
    );

    final raw = await _awaitReply(Msg.pairResp, const Duration(seconds: 5));
    if (raw == null) return 'The PC did not answer the pairing request.';

    final PairResp resp;
    try {
      resp = decodePairResp(raw);
    } catch (e) {
      return 'The PC sent a malformed pairing reply ($e).';
    }

    switch (resp.status) {
      case PairStatus.notInPairingMode:
        return 'The PC is not in pairing mode. Click "Pair a phone" on it first.';
      case PairStatus.codeMismatch:
      case PairStatus.rejected:
        return 'The PC refused pairing.';
      case PairStatus.ok:
        break;
    }
    // PhonePad 1.0 pairs in a way that gives the code away to the phone; never
    // continue with it, or a rogue "PC" could simply ask for the old scheme.
    if (!resp.commitScheme) {
      return 'This PC has an older PhonePad. Update the companion to 1.1 or '
          'newer, then pair again.';
    }

    final secrets = await keys.agree(resp.serverPub, deviceId);
    final clientConfirm =
        secrets.clientConfirm(keys.publicKey, resp.serverPub, code);
    final clientNonce = randomBytes(16);

    sock.send(
      encodePairCommit(
        deviceId: deviceId,
        clientCommit: clientCommitment(clientConfirm, clientNonce),
      ),
      pc.address,
      kDiscoveryPort,
    );

    final revealRaw =
        await _awaitReply(Msg.pairReveal, const Duration(seconds: 5));
    if (revealRaw == null) return 'The PC did not answer the pairing request.';

    final PairReveal reveal;
    try {
      reveal = decodePairReveal(revealRaw);
    } catch (e) {
      return 'The PC sent a malformed pairing reply ($e).';
    }
    switch (reveal.status) {
      case PairStatus.ok:
        break;
      case PairStatus.notInPairingMode:
        return 'Pairing has stopped on the PC. Click "Pair a phone" on it '
            'and try again.';
      case PairStatus.codeMismatch:
      case PairStatus.rejected:
        // Another attempt used the code first; the PC has moved on to a new one.
        return 'Something else tried to pair at the same moment. Enter the '
            'new code shown on the PC.';
    }

    // Verify the PC knew the code, with a proof it committed to before it saw
    // anything of ours. This is what stops another device on the network from
    // impersonating the PC, and only after it may our own proof leave.
    final valid = serverRevealIsValid(
      serverCommit: resp.serverCommit,
      revealedConfirm: reveal.serverConfirm,
      revealedNonce: reveal.serverNonce,
      expectedConfirm:
          secrets.serverConfirm(keys.publicKey, resp.serverPub, code),
    );
    if (!valid) {
      return 'Wrong code. The PC is now showing a new one; enter that.';
    }

    sock.send(
      encodePairConfirm(
        deviceId: deviceId,
        clientConfirm: clientConfirm,
        clientNonce: clientNonce,
      ),
      pc.address,
      kDiscoveryPort,
    );

    final resultRaw = await _awaitReply(Msg.pairResult, const Duration(seconds: 5));
    if (resultRaw == null) return 'The PC did not confirm pairing.';

    final PairStatus status;
    try {
      status = decodePairResult(resultRaw);
    } catch (e) {
      return 'The PC sent a malformed pairing result ($e).';
    }
    if (status != PairStatus.ok) {
      return status == PairStatus.codeMismatch
          ? 'Wrong code. The PC is now showing a new one; enter that.'
          : 'The PC refused pairing.';
    }

    _data.upsertPc(PairedPc(
      serverId: pc.serverId,
      hostname: pc.hostname,
      token: hex(secrets.token),
      lastAddress: pc.address.address,
      lastInputPort: pc.inputPort,
    ));
    await _store.save();
    notifyListeners();
    return null;
  }

  // --- session --------------------------------------------------------------

  Future<bool> connect(DiscoveredPc pc) async {
    final paired = _data.pcById(pc.serverId);
    if (paired == null) {
      _setPhase(ConnectionPhase.failed, error: 'This PC is not paired yet.');
      return false;
    }
    paired.lastAddress = pc.address.address;
    paired.lastInputPort = pc.inputPort;
    _pc = paired;
    _pcAddress = pc.address;
    _inputPort = pc.inputPort;
    await _store.save();
    return _openSession(announce: true);
  }

  Future<bool> _openSession({bool announce = false}) async {
    final pc = _pc;
    final addr = _pcAddress;
    final token = pc?.tokenBytes;
    if (pc == null || addr == null || token == null) {
      _setPhase(ConnectionPhase.failed, error: 'No paired PC selected.');
      return false;
    }

    if (announce) _setPhase(ConnectionPhase.connecting);
    final sock = await _ensureSocket();

    final clientNonce = randomBytes(16);
    sock.send(
      encodeSessionReq(
        deviceId: _data.deviceIdBytes,
        clientNonce: clientNonce,
        token: token,
      ),
      addr,
      _inputPort,
    );

    final raw = await _awaitReply(Msg.sessionResp, const Duration(seconds: 2));
    if (raw == null) {
      _setPhase(
        announce ? ConnectionPhase.failed : ConnectionPhase.reconnecting,
        error: 'The PC did not answer. Is the companion running?',
      );
      return false;
    }

    final SessionResp resp;
    try {
      resp = decodeSessionResp(raw, token);
    } catch (e) {
      // A failed MAC here almost always means the PC forgot this phone.
      _setPhase(ConnectionPhase.failed,
          error: 'The PC no longer recognises this phone. Pair again.');
      return false;
    }

    if (resp.status != SessionStatus.ok) {
      final reason = switch (resp.status) {
        SessionStatus.unknownDevice => 'The PC has forgotten this phone. Pair again.',
        SessionStatus.noBackend =>
          'The PC has no virtual controller available. Check ViGEmBus.',
        SessionStatus.serverBusy => 'Another phone is already connected.',
        _ => 'The PC refused the connection (${resp.status.name}).',
      };
      _setPhase(ConnectionPhase.failed, error: reason);
      return false;
    }

    _sessionId = resp.sessionId;
    _encoder = InputEncoder(
      deriveSessionKey(token, clientNonce, resp.serverNonce),
    );
    _seq = 0;
    // The PC starts each session's control counter at zero, so restart ours in
    // step rather than relying on it happening to stay ahead.
    _controlSeq = 0;
    _lastEchoMs = -1;
    _lastFeedbackAt = DateTime.now();
    _reconnectAttempts = 0;
    _setPhase(ConnectionPhase.connected);
    _startPumps();
    return true;
  }

  // --- hot path -------------------------------------------------------------

  void _startPumps() {
    _sendTimer?.cancel();
    _housekeepingTimer?.cancel();

    final hz = _data.settings.sendRateHz.clamp(60, 250);
    final periodMs = (1000 / hz).round().clamp(4, 16);

    // Unconditional repeat. Any single lost packet is corrected within one
    // period, which is why this can be UDP with no retransmission.
    _sendTimer = Timer.periodic(Duration(milliseconds: periodMs), (_) => _send());
    _housekeepingTimer =
        Timer.periodic(const Duration(milliseconds: 250), (_) => _housekeeping());
  }

  void _stopPumps() {
    _sendTimer?.cancel();
    _sendTimer = null;
    _housekeepingTimer?.cancel();
    _housekeepingTimer = null;
  }

  int _controlSeq = 0;

  /// Ask the PC to do something out of band — re-attach the virtual pad, or
  /// release every control.
  ///
  /// Sent a few times because a single lost datagram would otherwise make the
  /// button look broken. The PC only acts on a strictly increasing sequence, so
  /// the repeats are ignored rather than applied several times over.
  Future<void> sendControl(ControlCommand command) async {
    final sock = _socket;
    final enc = _encoder;
    final addr = _pcAddress;
    if (sock == null || enc == null || addr == null || _sessionId == 0) return;

    _controlSeq++;
    final bytes = encodeControl(
      sessionId: _sessionId,
      controlSeq: _controlSeq,
      command: command,
      sessionKey: enc.sessionKey,
    );

    for (var attempt = 0; attempt < 3; attempt++) {
      try {
        sock.send(bytes, addr, _inputPort);
      } catch (e) {
        debugPrint('PhonePad: control send failed: $e');
      }
      await Future<void>.delayed(const Duration(milliseconds: 40));
    }

    if (command == ControlCommand.releaseAll) {
      state.reset();
    }
    notifyListeners();
  }

  /// Called straight from the touch handler. Sending on change as well as on
  /// the timer is what removes the up-to-one-period wait from a button press.
  void sendNow() {
    if (_phase != ConnectionPhase.connected) return;
    _send();
  }

  void _send() {
    final sock = _socket;
    final enc = _encoder;
    final addr = _pcAddress;
    if (sock == null || enc == null || addr == null) return;

    _seq++;
    final bytes = enc.encode(
      sessionId: _sessionId,
      seq: _seq,
      clientTimeMs: _clock.elapsedMilliseconds & 0xFFFFFFFF,
      rttUs: _rttUs,
      flags: 0,
      state: state,
    );

    try {
      sock.send(bytes, addr, _inputPort);
      _sentThisSecond++;
    } catch (e) {
      // A send failure here is usually the Wi-Fi dropping. Housekeeping will
      // notice the missing feedback and start reconnecting; don't spam.
      debugPrint('PhonePad: send failed: $e');
    }
  }

  void _handleFeedback(Uint8List buf) {
    final enc = _encoder;
    if (enc == null) return;

    final FeedbackPacket fb;
    try {
      fb = decodeFeedback(buf, enc.sessionKey);
    } catch (_) {
      return; // forged or from a stale session
    }
    if (fb.sessionId != _sessionId) return;

    _feedbackThisSecond++;
    _lastFeedbackAt = DateTime.now();
    stats.pcAcceptedPps = fb.acceptedPps;
    stats.pcLossPermille = fb.lossPermille;
    stats.rumbleLarge = fb.rumbleLarge;
    stats.rumbleSmall = fb.rumbleSmall;
    onRumble?.call(fb.rumbleLarge, fb.rumbleSmall);

    // Only a *new* echo says anything about the current round trip. Measuring
    // against a stale one makes RTT climb forever after packets stop, which
    // would be a fabricated number rather than a reading.
    if (fb.echoClientTimeMs != _lastEchoMs) {
      _lastEchoMs = fb.echoClientTimeMs;
      final now = _clock.elapsedMilliseconds & 0xFFFFFFFF;
      final sample = ((now - fb.echoClientTimeMs) * 1000).clamp(0, 2000000);
      if (_smoothedRttUs == 0) {
        _smoothedRttUs = sample.toDouble();
      } else {
        _jitterUs += ((sample - _smoothedRttUs).abs() - _jitterUs) / 8.0;
        _smoothedRttUs += (sample - _smoothedRttUs) / 8.0;
      }
      _rttUs = _smoothedRttUs.round();
      stats.rttUs = _rttUs;
      stats.jitterUs = _jitterUs.round();
    }

    if (_phase == ConnectionPhase.reconnecting) {
      _reconnectAttempts = 0;
      _setPhase(ConnectionPhase.connected);
    }
  }

  void _housekeeping() {
    final now = DateTime.now();

    final windowElapsed = now.difference(_rateWindow);
    if (windowElapsed >= const Duration(seconds: 1)) {
      _rateWindow = now;
      // Divide by the window that actually elapsed. This check only runs on the
      // 250 ms housekeeping tick, so the window is really 1.0-1.25 s; treating
      // it as exactly one second over-reported the rate by up to 25%.
      final seconds = windowElapsed.inMicroseconds / 1e6;
      stats.sentPps = (_sentThisSecond / seconds).round();
      stats.feedbackPps = (_feedbackThisSecond / seconds).round();
      stats.seq = _seq;
      stats.lastFeedback = _lastFeedbackAt;
      _sentThisSecond = 0;
      _feedbackThisSecond = 0;
      notifyListeners();
    }

    final last = _lastFeedbackAt;
    if (last == null) return;
    final silence = now.difference(last);

    if (silence > _feedbackTimeout && _phase == ConnectionPhase.connected) {
      // Release everything locally too. The PC's watchdog is the real
      // guarantee, but there is no reason for the phone to keep believing a
      // button is held.
      state.reset();
      _setPhase(ConnectionPhase.reconnecting,
          error: 'Lost contact with the PC — retrying.');
    }

    if (_phase == ConnectionPhase.reconnecting &&
        silence > _feedbackTimeout + _reconnectInterval) {
      _lastFeedbackAt = now; // rate-limit the retries
      // From the second attempt on, start from a fresh socket. Measured on a
      // Galaxy S24 Ultra (Android 17) sharing its data over USB: when the
      // tethering interface was torn down and re-created mid-session, the
      // existing socket kept *sending* (the PC saw every session request) but
      // never received another datagram, so the phone sat on "reconnecting"
      // indefinitely while a newly bound socket connected at once. A rebind is
      // cheap and the PC follows the new port, because each session request
      // opens a fresh session from wherever it came from.
      if (_reconnectAttempts++ > 0) _rebindSocket();
      unawaited(_openSession());
    }
  }

  // --- teardown -------------------------------------------------------------

  Future<void> disconnect() async {
    final sock = _socket;
    final enc = _encoder;
    final addr = _pcAddress;
    if (sock != null && enc != null && addr != null && _sessionId != 0) {
      try {
        sock.send(encodeBye(_sessionId, enc.sessionKey), addr, _inputPort);
      } catch (_) {
        /* the watchdog covers us either way */
      }
    }
    _stopPumps();
    state.reset();
    _encoder = null;
    _sessionId = 0;
    _rttUs = 0;
    _smoothedRttUs = 0;
    _jitterUs = 0;
    _setPhase(ConnectionPhase.idle);
  }

  @override
  void dispose() {
    _stopPumps();
    _socket?.close();
    _socket = null;
    super.dispose();
  }
}
