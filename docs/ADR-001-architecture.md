# ADR-001 — PhonePad architecture

**Status:** Accepted · **Date:** 2026-08-15

## Context

Turn a Galaxy S24 Ultra into a low-latency wireless Xbox-compatible game controller for
a Windows 11 PC. The PC has **no Bluetooth**, which removes Android's `BluetoothHidDevice`
path entirely. Input must travel over the local Wi-Fi network and be injected into Windows
as a device the OS and games treat as real hardware.

## Environment as measured (2026-08-15)

| Component | Version |
|---|---|
| Flutter / Dart | 3.44.4 stable / 3.12.2 |
| Android SDK | platforms 28–36, build-tools 34.0.0–36.1.0-rc1, NDK 26–29 |
| Rust | 1.96.0 `x86_64-pc-windows-msvc` |
| MSVC / Windows SDK | VS Build Tools 2019 14.29.30133 / 10.0.19041 |
| .NET | **not installed** |
| ViGEmBus | 1.16.112.0, status OK |
| Phone | Galaxy S24 Ultra, Android 16 (API 36), arm64-v8a |

## Decision 1 — Windows virtual controller: ViGEmBus via `vigem-client` (Rust)

### Options considered

| Option | Verdict |
|---|---|
| **Virtual HID Framework (VHF) / HID source driver** | Technically the "most correct" modern answer, and what Microsoft documents. But it requires authoring a KMDF driver and either an EV-signed WHQL submission or putting the machine into test-signing mode. Unusable for a product the user wants to play games with today. |
| **GameInput** | A *consumer* API. It reads devices; it provides no supported way to publish a virtual one. Not applicable. |
| **vJoy** | Presents a DirectInput-only device. Xbox Cloud Gaming and most modern titles read XInput or the W3C Gamepad API, so a vJoy stick is invisible to them. Rejected. |
| **ViGEmBus + Xbox 360 target** | Chosen. |

### Why ViGEmBus despite being EOL

Nefarius archived the project on 2023-11-02 (trademark conflict with ViGEM GmbH); 1.22.0 is
the final release. That is a genuine risk and is recorded as such. It is still the right
choice because it is the only mechanism that produces a **real XUSB device node** without
driver signing. That single device is simultaneously visible to `joy.cpl`, XInput,
DirectInput, GameInput, and the **W3C Gamepad API** — the last being exactly what Xbox
Cloud Gaming reads in the browser. It is the same mechanism Parsec and DS4Windows ship.

**Mitigation:** the backend sits behind a `VirtualPad` trait in `pp-core`. Swapping in a
successor (VirtualPad/Shibari, or a signed driver later) touches one file.

### Verified, not assumed

`pp-vigem-probe` plugs in a target, writes known states, and reads them back through
Windows' own XInput API. Result on 2026-08-15 against ViGEmBus **1.16.112**:

```
26 / 26 checks passed
write -> observable via XInput: avg 0.09 ms, max 0.53 ms
```

All 15 buttons, 5 trigger levels, 6 stick positions round-tripped exactly. The virtual pad
adds effectively nothing to the latency budget.

One wrinkle worth recording: the public `XInputGetState` **masks out the Guide button**
(0x0400). Only the undocumented `XInputGetStateEx` (xinput1_4.dll, ordinal 100) reports it.
This is a Windows API behaviour, not a ViGEm limitation — Guide is delivered correctly. The
probe resolves the Ex entry point dynamically so it can verify Guide honestly, and degrades
to skipping that one check if the export is missing.

1.22.0 is still recommended for better Windows 11 23H2+ behaviour, but is **not** a blocker.

## Decision 2 — Companion in Rust with `egui`, single self-contained .exe

No .NET runtime exists on the target machine, so a .NET companion would impose an install.
Rust produces a dependency-free binary and lets the UDP receiver run on a plain OS thread
with no async runtime in the hot path. `egui` keeps the diagnostics UI lightweight.

The receiver, session manager, watchdog and pad backend live in `pp-core` and never block
on the UI — the UI is a reader of shared state, exactly as §14 of the brief requires.

## Decision 3 — Phone: Flutter for UI, Kotlin only where it earns its place

The brief suggests a "native Android transport". **This is challenged deliberately** (§26
invites it). Routing every input packet through a Flutter↔Kotlin method channel *adds* a
hop and per-packet allocation to the hot path; it does not remove one. The lower-latency
arrangement is:

```
Listener (raw pointer events, no gesture arena)
   -> ControllerState engine (plain Dart, no widget rebuilds)
   -> RawDatagramSocket.send        <- one syscall, same isolate
```

Rendering is a single `CustomPaint` under a `RepaintBoundary` driven by a `Listenable`, so
touch never triggers a widget rebuild or layout pass.

Kotlin is used for what Dart genuinely cannot reach:

- 120 Hz preferred display mode
- `WIFI_MODE_FULL_LOW_LATENCY` Wi-Fi lock (a real, measurable latency win on Samsung)
- `setSystemGestureExclusionRects` (stops the back-swipe firing mid-game)
- `VibratorManager` haptics
- Wi-Fi link info (IPv4 + prefix) to compute the subnet broadcast address
- Foreground service so Doze cannot kill an active session

This decision is **measured at M4, not assumed**. `InputTransport` is an interface; if the
Dart path shows unacceptable jitter, a Kotlin sender drops in behind it without redesign.

## Decision 4 — Transport: compact binary UDP, not JSON, not TCP

40-byte fixed-layout packets at up to 250 Hz (~10 KB/s). TCP's head-of-line blocking is
actively harmful here: a retransmitted stale input is worse than a dropped one. Loss is
handled by redundancy instead — state is re-sent at 120 Hz regardless of change, so any
single lost packet self-heals within ~8 ms.

See `protocol/PROTOCOL.md` for the wire format.

## Decision 5 — Security: X25519 pairing, per-packet MAC

An unauthenticated UDP socket that injects controller input is a genuine hazard on a shared
LAN. Pairing performs X25519 ECDH authenticated by a 6-digit code shown in the companion
UI, yielding a persisted 32-byte token. Every input packet carries a truncated HMAC-SHA256
over its contents plus a sliding-window sequence check, so packets cannot be forged or
replayed. Rejected packets are counted and surfaced in diagnostics, never silently dropped.

## Decision 6 — Failsafe

A stuck stick or held trigger from a dead connection is the worst possible failure mode.
The companion runs a watchdog: **no valid packet for 120 ms → all-zero neutral state pushed
to the pad**; 2 s → session dropped. Process exit unplugs the target. This is enforced in
`pp-core` and covered by unit tests against a virtual clock.

## Risks

| Risk | Mitigation |
|---|---|
| ViGEmBus is EOL | `VirtualPad` trait; driver version detected and displayed; loud actionable error if absent |
| Windows Firewall blocks the UDP bind | Detected and explained in-UI, with an optional elevated `netsh advfirewall` helper |
| Samsung battery optimisation kills the session | Foreground service + Wi-Fi lock; app warns if it detects itself restricted |
| AP client isolation / broadcast filtering breaks discovery | Failure surfaced, not swallowed; manual-IP and QR pairing fallbacks |
| build-tools 36.1.0-rc1 is a release candidate | Pin to stable 36.0.0 if it misbehaves |
| Dart hot-path jitter | Measured at M4; Kotlin sender swappable behind `InputTransport` |
