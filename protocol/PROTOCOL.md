# PhonePad wire protocol v1

Single source of truth for both implementations:

- Rust — `companion/crates/pp-protocol/`
- Dart — `phone/lib/core/protocol/`

Both are asserted against the shared fixtures in `protocol/vectors.json`, so the two
codecs cannot silently drift apart.

## Conventions

- **All integers little-endian.** All packets fixed-layout; no varints, no JSON.
- Transport is UDP. **Discovery port 47800**, **input port 47801** (both configurable).
- Every packet starts with the same 4-byte header:

| off | size | field | |
|---|---|---|---|
| 0 | 1 | `magic` | `0x50` (`'P'`) |
| 1 | 1 | `version` | `1` |
| 2 | 1 | `type` | see below |
| 3 | 1 | `flags` | type-specific |

Anything that fails the magic/version/length check is rejected and **counted** — never
silently dropped. Counters surface in the companion diagnostics pane.

## Message types

| code | name | direction |
|---|---|---|
| `0x01` | DISCOVER_REQ | phone → broadcast :47800 |
| `0x02` | DISCOVER_RESP | PC → phone (unicast) |
| `0x03` | PAIR_REQ | phone → PC :47800 |
| `0x04` | PAIR_RESP | PC → phone |
| `0x09` | PAIR_COMMIT | phone → PC |
| `0x0A` | PAIR_REVEAL | PC → phone |
| `0x07` | PAIR_CONFIRM | phone → PC |
| `0x08` | PAIR_RESULT | PC → phone |
| `0x05` | SESSION_REQ | phone → PC :47801 |
| `0x06` | SESSION_RESP | PC → phone |
| `0x10` | INPUT | phone → PC :47801 |
| `0x11` | FEEDBACK | PC → phone |
| `0x12` | BYE | phone → PC |

---

## Discovery

The phone sends the probe to:

- the **subnet-directed broadcast address of every local IPv4 link**: its Wi-Fi,
  and any network it is providing itself (hotspot, USB or Bluetooth tethering).
  The Kotlin side enumerates interfaces and leaves out cellular and VPN links by
  transport, not by address, because carriers commonly use private 10.x
  addresses;
- `255.255.255.255`, **only** when the phone's default network is Wi-Fi or
  Ethernet. Otherwise the kernel routes it over mobile data;
- the last address of each paired PC, **only** when it lies inside one of those
  links, in case broadcast is filtered.

Some access points drop the all-ones address but forward subnet broadcasts,
which is why both are sent. Tethering is why every link is: when the phone
shares its connection, the PC sits on a tethering interface that is never
Android's default network, and the kernel keeps a `broadcast <subnet>.255` route
for each interface in its local table, so a subnet-directed probe leaves on the
link it names. The PC replies **unicast to the source address**.

This direction matters: the phone therefore only ever needs to *receive a unicast*, which
sidesteps Android's multicast-lock flakiness entirely.

**DISCOVER_REQ**

| off | size | field |
|---|---|---|
| 0 | 4 | header |
| 4 | 16 | `deviceId` — random, generated once, persisted |
| 20 | 4 | `nonce` |
| 24 | 1 | `nameLen` |
| 25 | n | `name` (UTF-8, ≤ 64 bytes) |

**DISCOVER_RESP**

| off | size | field |
|---|---|---|
| 0 | 4 | header |
| 4 | 4 | `nonce` (echoed) |
| 8 | 16 | `serverId` |
| 24 | 2 | `inputPort` |
| 26 | 1 | `state` — bit0 `alreadyPaired`, bit1 `pairingModeActive` |
| 27 | 1 | `hostLen` |
| 28 | n | `hostname` |
| .. | 1 | `backendLen` |
| .. | m | `backend`, e.g. `Xbox 360 (ViGEm 1.16.112)` |

---

## Pairing — X25519 + 6-digit code, commit then reveal (pairing v2)

The code is displayed by the companion and typed on the phone. It authenticates the
Diffie–Hellman exchange, so a device on the network can neither man-in-the-middle it
nor pair without the code.

Six digits are a million possibilities, which is nothing to try offline. So no
message ever lets a peer test guesses at leisure: each side **commits** to its proof
of the code before the other **reveals** one, and the PC retires the code the moment
it reveals.

```
phone                                                     PC
  |-- PAIR_REQ     deviceId, clientPub ------------------->|  pairing mode must be on
  |<-- PAIR_RESP   status, serverPub, H(srvConfirm, rs) ---|  a commitment, not the proof
  |   user has typed the code                              |
  |-- PAIR_COMMIT  deviceId, H(cliConfirm, rc) ----------->|  phone commits first
  |<-- PAIR_REVEAL status, srvConfirm, rs -----------------|  PC opens; code is retired
  |   check H(srvConfirm, rs) and srvConfirm               |
  |-- PAIR_CONFIRM deviceId, cliConfirm, rc -------------->|  phone opens only if both pass
  |<-- PAIR_RESULT status ---------------------------------|
```

```
shared     = X25519(ourPriv, theirPub)
k          = HKDF-SHA256(shared, salt = "phonepad-v1", info = "pair")
srvConfirm = HMAC-SHA256(k, "pp-srv" || clientPub || serverPub || codeAscii)
cliConfirm = HMAC-SHA256(k, "pp-cli" || clientPub || serverPub || codeAscii)
srvCommit  = SHA-256("pp-commit-srv" || srvConfirm || rs)      rs, rc: 16 random bytes
cliCommit  = SHA-256("pp-commit-cli" || cliConfirm || rc)
token      = HKDF-SHA256(shared, salt = "phonepad-v1", info = "token" || deviceId)
```

**The 32-byte `token` is never transmitted** — both sides derive it independently and
persist it. It is the long-term credential for that phone/PC pair.

### Rules that make it hold

- **The PC retires the code on every reveal.** As soon as it sends PAIR_REVEAL it
  shows a new code and discards every other attempt in flight. A peer that commits a
  guess learns the old code from the reveal (it holds `k`), but that code no longer
  pairs anything.
- **The phone reveals last.** It sends `cliConfirm` only after `srvConfirm` opens the
  commitment from PAIR_RESP *and* matches the code the user typed. A rogue "PC" had
  to commit before it could have known the code, so it cannot pass; the phone stops
  and reveals nothing.
- **At most five reveals per pairing window**, then pairing mode closes. A device
  without the code succeeds with probability at most 5 in a million per click of
  "Pair a phone".
- **No fallback.** The v2 forms of PAIR_REQ, PAIR_RESP and PAIR_CONFIRM set header
  flag bit 0. A 1.1 companion answers a request without it with `Rejected` and
  nothing derived from the code; a 1.1 phone refuses a reply without it. Either side
  says which one needs updating.
- At most eight attempts are kept in flight; the device id keying them is chosen by
  whoever sends the request.

### Pairing v1 (1.0.0) and why it was replaced

In v1, PAIR_RESP carried `srvConfirm` itself. Any device could send one PAIR_REQ with
a key pair of its own, compute `k`, and test all million codes against the reply
offline in well under a second, then send a valid PAIR_CONFIRM and be paired. The
loopback tests in `pp-core/tests/loopback.rs` now play that attacker against the
companion.

### Layouts

**PAIR_REQ** — header (flags bit 0 = v2), `deviceId` (16), `clientPub` (32), `nameLen` (1), `name` (n)

**PAIR_RESP** — 69 bytes: header (flags bit 0 = v2), `status` (1), `serverPub` (32), `srvCommit` (32)

**PAIR_COMMIT** — 52 bytes: header, `deviceId` (16), `cliCommit` (32)

**PAIR_REVEAL** — 53 bytes: header, `status` (1), `srvConfirm` (32), `rs` (16)

**PAIR_CONFIRM** — 68 bytes: header (flags bit 0 = v2), `deviceId` (16), `cliConfirm` (32), `rc` (16)

**PAIR_RESULT** — 5 bytes: header, `status` (1)

`status`: `0` ok · `1` not in pairing mode · `2` code mismatch · `3` rejected

---

## Session establishment

**SESSION_REQ** — header, `deviceId` (16), `clientNonce` (16), `mac` (8) = 44 bytes
**SESSION_RESP** — header, `status` (1), `sessionId` (4), `serverNonce` (16), `mac` (8) = 33 bytes

`status`: `0` ok · `1` unknown device · `2` bad MAC · `3` server busy · `4` no backend

```
sessionKey = HKDF-SHA256(token, salt = clientNonce || serverNonce, info = "session")
```

A fresh `sessionKey` per session means a captured stream cannot be replayed into a later
session even if sequence numbers restart.

---

## INPUT — 40 bytes, the hot path

| off | size | field | |
|---|---|---|---|
| 0 | 4 | header | `flags` bit0 = gyro contributing |
| 4 | 4 | `sessionId` | |
| 8 | 4 | `seq` | starts at 1, monotonic |
| 12 | 4 | `clientTimeMs` | phone monotonic clock; the PC only ever echoes it back |
| 16 | 2 | `buttons` | bitmask, below |
| 18 | 2 | `lx` | i16, −32767..32767 |
| 20 | 2 | `ly` | i16, **up is positive** (XInput convention) |
| 22 | 2 | `rx` | i16 |
| 24 | 2 | `ry` | i16 |
| 26 | 1 | `lt` | u8 0..255 |
| 27 | 1 | `rt` | u8 0..255 |
| 28 | 4 | `rttUs` | round-trip time in µs as last measured **by the phone** from FEEDBACK echoes; `0` = not yet known. Lets the companion display real end-to-end latency without the two devices needing a shared clock. |
| 32 | 8 | `mac` | `HMAC-SHA256(sessionKey, bytes[0..32])[0..8]` |

Button bits are **deliberately identical to XInput's `wButtons`**, so the companion never
has to translate — it copies the field straight through.

| bit | control | | bit | control |
|---|---|---|---|---|
| `0x0001` | D-pad up | | `0x0100` | LB |
| `0x0002` | D-pad down | | `0x0200` | RB |
| `0x0004` | D-pad left | | `0x0400` | Guide |
| `0x0008` | D-pad right | | `0x0800` | *reserved* |
| `0x0010` | Start / Menu | | `0x1000` | A |
| `0x0020` | Back / View | | `0x2000` | B |
| `0x0040` | LS press | | `0x4000` | X |
| `0x0080` | RS press | | `0x8000` | Y |

### Send policy

Send **on every state change**, plus an unconditional repeat at 120 Hz. At 40 bytes that is
~10 KB/s at 250 Hz — negligible. The repeat is what makes packet loss a non-event: any
single dropped packet is corrected within ~8 ms without retransmission, which is why this
is UDP and not TCP. A retransmitted *stale* input would be worse than a lost one.

### Validation on receipt

1. length, magic, version
2. known `sessionId`
3. MAC over `bytes[0..32]`
4. replay: `seq` against a 64-entry sliding bitmap window

Failures increment a named counter and are visible in diagnostics.

---

## FEEDBACK — 28 bytes, PC → phone at 20 Hz

| off | size | field |
|---|---|---|
| 0 | 4 | header |
| 4 | 4 | `sessionId` |
| 8 | 4 | `echoClientTimeMs` — from the most recent accepted INPUT, gives the phone its RTT |
| 12 | 1 | `rumbleLarge` |
| 13 | 1 | `rumbleSmall` |
| 14 | 2 | `acceptedPps` |
| 16 | 2 | `lossPermille` |
| 18 | 2 | reserved |
| 20 | 8 | `mac` over `bytes[0..20]` |

## BYE — 16 bytes

Header, `sessionId` (4), `mac` (8). A courtesy for clean disconnects; the watchdog is the
guarantee, not this.

---

## Failsafe

| condition | action |
|---|---|
| no valid INPUT for **120 ms** | push all-zero neutral state to the virtual pad |
| no valid INPUT for **2 s** | drop the session; phone must re-establish |
| companion process exits | target unplugged (also enforced by ViGEm on handle close) |
| backend error | neutral state, session dropped, error surfaced in the UI log |

Neutral means every button released, both sticks centred, both triggers zero. A held
control from a dead connection is the worst failure this system can have, so it is enforced
in `pp-core` and covered by unit tests against a virtual clock rather than left to chance.
