# Acceptance checklist

Status as of 2026-11-09 (1.1.0); the 1.0.0 results below still stand. "Verified" means it was actually exercised on the real
S24 Ultra and the real PC and the result observed — not that the code looks
right.

## Automated

| Suite | Count | Status |
|---|---|---|
| `cargo test --workspace` | 95 | pass |
| `flutter test` | 99 | pass |

Notable coverage, because line counts say little:

- **Protocol conformance.** Both codecs are asserted against the same
  `protocol/vectors.json`. A byte-level divergence between Rust and Dart fails a
  test rather than showing up as unexplained "bad MAC" counters at runtime.
- **Pairing crypto conformance.** Deterministic X25519 scalars produce a fixed
  public key, token, confirmation tags and session key that both sides must
  reproduce exactly. Without this, a clamping or HKDF mismatch would surface
  only as a mysterious "wrong code" on a real device.
- **Security negatives.** Forged MAC, wrong key, replayed burst, reserved bit
  set, `i16::MIN` axis, truncation at *every* length, MITM with a substituted
  public key — all rejected.
- **Watchdog.** Neutralisation and session drop tested against a live core over
  real sockets, not mocked.
- **Multi-touch.** Including the classic bug where two fingers on one button and
  releasing one wrongly clears it.

- **Pairing attacks (1.1.0).** The loopback suite plays the attacker against a
  live core: the 1.0 offline brute force finds nothing in a v2 reply; a code
  learned by committing garbage has already been retired; a reveal cancels
  every other attempt in flight; pairing stops after five attempts; a 1.0
  request is refused by name. Before the fix the same brute force recovered the
  code from a 1.0.0 core in 174 ms.
- **Discovery targets (1.1.0).** Which addresses a probe goes to, tested against
  the exact links the S24 Ultra reported while tethering (mobile data on a
  private 10.x address as the default network).
- **Firewall verdicts (1.1.0).** Block rules beating allow rules, rules scoped
  to one adapter, every port required, read back through a real `VARIANT`.
- **Ad strip (1.1.0).** House placement without an ad unit, never under the
  content, clear of the gesture area, absent in landscape on a phone.

## Manual — verified for 1.1.0

On the Galaxy S24 Ultra (Android 17) and the Windows 11 PC, which has no Wi-Fi
adapter, so tethering was exercised over USB; the Wi-Fi hotspot shares the
same code path on the phone and was not run with a Wi-Fi client.

| # | Check | Result |
|---|---|---|
| 1 | Issue #2 reproduced with the shipped 1.0.0 app and companion | USB tethering, phone Wi-Fi off: "No PC found" |
| 2 | Phone finds the PC over USB tethering, broadcast only | Found in about 1 s with no stored address; link shown as "USB tethering" |
| 3 | Session over the tether | Connected, 3.0 to 3.8 ms round trip |
| 4 | Input reaches Windows over the tether | XInput slot 0: A pressed `0x1000`, released `0x0000` |
| 5 | Tethering switched off and on mid-session | Reconnected by itself on a fresh socket (1.0.0 sat on "reconnecting") |
| 6 | Windows blocking PhonePad on a public network | A fresh install answered "Private only" got public block rules; the companion flagged the tether network, offered **Make private**; after the UAC prompt the network was private, the private rule present, and the phone connected. The home LAN was untouched |
| 7 | USB tethering gets a new network identity per session | The phone's tethering MAC changed between sessions; Windows named the network anew; the companion says so |
| 8 | Pairing v2, right code | Paired and connected on the shipping builds |
| 9 | Pairing v2, wrong code | Phone refused to reveal its proof and said so; PC replaced the code and named the attempt |
| 10 | 1.0.0 phone → 1.1.0 companion | Refused; companion: "has an older PhonePad that pairs insecurely" |
| 11 | 1.1.0 phone → 1.0.0 companion | Refused; phone: "This PC has an older PhonePad. Update the companion" |
| 12 | A phone paired under 1.0 against the 1.1 companion | Connects; sessions unchanged |
| 13 | Ad strip with Google's test unit | Banner on home, layouts and settings; never on the controller; clear of the gesture hint; none in landscape |
| 14 | Release build without an ad unit | House placement in the strip; no ad SDK log output |
| 15 | Companion sponsor strip, site file absent | House placement shown; nothing blocked |

## Manual — verified for 1.0.0

| # | Check | Result |
|---|---|---|
| 1 | Windows detects a real game controller | `joy.cpl` shows Xbox 360 Controller; XInput slot 0 |
| 2 | Every button, axis and trigger works | 26/26 round-tripped through XInput |
| 3 | Guide button reaches Windows | Verified via `XInputGetStateEx` |
| 4 | Phone finds the PC automatically | PC found by hostname in ~1 s over Wi-Fi |
| 5 | Pairing with a six-digit code | Paired; token persisted both sides |
| 6 | Wrong code is refused | Covered by test + companion logs the attempt |
| 7 | Reconnect without re-pairing | Tapping the PC connects straight through |
| 8 | Real touch drives the pad | Observed A, B, X, Y, LB, RB, D-pad, triggers 255, stick −29063 |
| 9 | Multi-touch, no cross-talk | Sticks and buttons independent under test |
| 10 | Portrait layout | Renders correctly, all 17 controls |
| 11 | Landscape layout | Renders correctly after the rotation fix below |
| 12 | Rotation mid-session | Layout swaps, connection survives (256 pps throughout) |
| 13 | **App killed with a button held** | Released in **131 ms**; no stuck input |
| 14 | Session dropped after silence | 2.0 s, as configured |
| 15 | 120 Hz display | Confirmed `120 Hz (max 120)` on device |
| 16 | Release APK is properly signed | `CN=PhonePad`, APK Signature Scheme v2 |
| 17 | Release build runs clean | No `FATAL`/`E/flutter` in logcat; 4.9 ms RTT |
| 18 | Diagnostics show real numbers | RTT, jitter, loss, pps all live and measured |
| 19 | **Xbox Cloud Gaming, real gameplay input** | Xbox PC app, Beast of Reincarnation — A registered and the game accepted the controller, after re-attach post-launch |
| 20 | **Racing game, sustained play** | Forza Horizon 5 over Xbox Cloud Gaming, about an hour: proportional steering from the first millimetre, analogue triggers feathered by sliding, rumble on the phone. Learning curve 10 to 15 minutes for a first-time controller user. |
| 20 | Re-attach triggered from the phone | PC pad interfaces went 2 → 0 → 2 from a tap on the phone |

### Bugs this checklist actually caught

Worth recording, because they would all have shipped:

1. **Rotation showed a stale frame.** `TouchRouter.updateGeometry` only notified
   its listeners when fingers were down, so rotating an idle screen changed the
   geometry without repainting — the landscape layout rendered as portrait specs
   at the old width. Fixed, with a regression test.
2. **Square stick gate capped diagonals at 23170** instead of 32767. The
   deadzone rescale re-projected onto the unit circle, which is correct for a
   circular gate and wrong for a square one. Fixed by measuring a square gate
   with the max-norm.
3. **Pairing blocked on stale discovery data.** The app refused to show the code
   dialog based on a `pairingMode` flag captured at discovery time, stranding
   anyone who opened the pairing window afterwards. Now it asks the PC, whose
   answer is never stale.
4. **Sender ran at 65 Hz instead of 250 Hz** (test tool) — Windows scheduler
   tick quantising socket timeouts. See `benchmarks.md`.
5. **RTT climbed forever after packets stopped**, because a stale echo was being
   re-measured against a moving clock. Now only a fresh echo updates it.
6. **Packet rate was overstated by up to 25%.** Both sides published a count
   whenever "more than a second" had passed, but that check runs on a periodic
   tick, so the window was really 1.0–1.25 s. It surfaced as the phone claiming
   *sending 312/sec* against *PC accepting 259/sec*, which looked like 17% loss
   and was neither. See `benchmarks.md`.
7. **Parallel test flake.** `free_port()` closed its probe socket before `Core`
   rebound it, so two harnesses could be handed the same port and one test would
   see another's packets. Ports now come from a process-wide counter.

### Root cause: cloud gaming ignored the controller

Worth writing down, because the symptom points away from the real cause.

**Symptom.** Xbox app menus and the Guide button responded to the pad; the
streamed game ignored every input.

**Not the cause**, all checked and ruled out: XInput (26/26), duplicate or
phantom gamepads (exactly one device present), device identity (genuine
`VID_045E&PID_028E` with the `IG_` marker), packet loss (0.0%), or anything in
the PhonePad pipeline.

**Actual cause.** A cloud client binds a controller to a streaming session when
it observes the controller *arrive*. The client's own UI reads whatever pads
exist and so behaves normally, but a pad that predates the stream never
generates an arrival event, leaving the session with nothing bound. The split
between "client UI works, game does not" is the signature.

**Fix.** `Command::ReattachPad` unplugs and replugs the ViGEm target, producing
a genuine arrival. Reachable from the companion window and — because the moment
you need it is mid-game behind a fullscreen window — from the phone, over an
authenticated CONTROL message.

## Manual — not yet done

Stated plainly rather than quietly skipped:

| Check | Why it is still open |
|---|---|
| A locally installed (non-streamed) game | Only cloud gaming has been played through |
| Wi-Fi genuinely dropping (router off, airplane mode) | Only simulated by killing the app so far. The watchdog path is identical, but the reconnect-after-real-outage journey has not been walked |
| Screen off / on mid-session | Not exercised |
| Long-session battery drain | Not measured |
| Second phone connecting while one is active | Single-session by design; the replacement path has a unit test but no hardware run |

## How to re-run

```powershell
cd companion; cargo test --workspace
cd ..\phone;  flutter test

# Hardware
dist\pp-vigem-probe.exe                                   # gate 1-3
dist\pp-headless.exe --pair                               # then pair the phone
dist\pp-vigem-probe.exe --watch --seconds 15              # touch the phone, watch XInput
```
