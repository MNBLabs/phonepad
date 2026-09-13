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
| `0x07` | PAIR_CONFIRM | phone → PC |
| `0x08` | PAIR_RESULT | PC → phone |
| `0x05` | SESSION_REQ | phone → PC :47801 |
| `0x06` | SESSION_RESP | PC → phone |
| `0x10` | INPUT | phone → PC :47801 |
| `0x11` | FEEDBACK | PC → phone |
| `0x12` | BYE | phone → PC |

---

## Discovery

The phone broadcasts to **both** `255.255.255.255` and the subnet-directed broadcast
address (the subnet broadcast address, e.g. `192.168.x.255`, computed from the Kotlin link-info call) because some access
points drop the all-ones address. The PC replies **unicast to the source address**.

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

## Pairing — X25519 + 6-digit code

The code is displayed by the companion and typed on the phone. It authenticates the
Diffie–Hellman exchange, so a device sniffing the LAN cannot man-in-the-middle it.

```
phone                                             PC
  |-- PAIR_REQ  deviceId, clientPub ------------->|   (pairing mode must be active)
  |<-- PAIR_RESP status, serverPub, srvConfirm ---|
  |   user types the 6-digit code                 |
  |   verify srvConfirm  -> proves PC knows code  |
  |-- PAIR_CONFIRM deviceId, cliConfirm --------->|   proves phone knows code
  |<-- PAIR_RESULT status ------------------------|
```

```
shared     = X25519(ourPriv, theirPub)
k          = HKDF-SHA256(shared, salt = "phonepad-v1", info = "pair")
srvConfirm = HMAC-SHA256(k, "pp-srv" || clientPub || serverPub || codeAscii)
cliConfirm = HMAC-SHA256(k, "pp-cli" || clientPub || serverPub || codeAscii)
token      = HKDF-SHA256(shared, salt = "phonepad-v1", info = "token" || deviceId)
```

**The 32-byte `token` is never transmitted** — both sides derive it independently and
persist it. It is the long-term credential for that phone/PC pair.

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
