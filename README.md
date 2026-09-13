<div align="center">

<img src="brand/icon/legacy-512.png" width="96" height="96" alt="">

# PhonePad

**No controller? Use your phone.**

Turns an Android phone into a wireless Xbox-compatible controller for Windows,
over ordinary Wi-Fi. No Bluetooth. No cable. Nothing to buy.

[Download](https://github.com/MNBLabs/phonepad/releases/latest) ·
[Setup guide](https://phonepad.dynshift.com/setup/) ·
[Troubleshooting](https://phonepad.dynshift.com/troubleshooting/)

</div>

---

Windows sees a real Xbox 360 pad on an XInput slot, so anything that takes a
controller takes this: Steam, Game Pass, emulators, browser games, and **Xbox
Cloud Gaming**.

Measured on the development hardware — a Galaxy S24 Ultra and a Windows 11 PC on
the same home Wi-Fi:

| | |
|---|---|
| Round trip, phone → PC → phone | **4.9–8.0 ms**, typically ~5.2 ms |
| Packet rate | 249/sec |
| Packet loss | 0.0% across 42,805 consecutive packets |
| Jitter | 0.1–0.6 ms |

Those are real numbers from [`docs/benchmarks.md`](docs/benchmarks.md), not
targets.

## Setting it up

### 1. On the PC

Install the [ViGEmBus driver](https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0)
once — it is what lets a program create a virtual controller Windows can see.

Then run **`PhonePad-Companion.exe`** from the
[latest release](https://github.com/MNBLabs/phonepad/releases/latest). It is
self-contained: no .NET, no installer. Windows will ask to allow it through the
firewall — say yes for **private networks**. (If you miss the prompt, there is an
"Add firewall rule" button under Advanced.)

You should see **Waiting for your phone**, and a six-digit pairing code.

### 2. On the phone

Install **`PhonePad-arm64.apk`** from the same release. Android will ask you to
allow installing from an unknown source; that is normal for an app not
distributed through the Play Store.

Open it. Your PC appears within a second or two. Tap it, type the six digits, and
you are on the controller.

After that first pairing, opening the app and tapping your PC connects straight
through — no code, no IP addresses.

### 3. Check Windows sees it

Run `joy.cpl`. "Xbox 360 Controller for Windows" should be listed, and its
buttons and axes should light up as you touch the phone.

## Xbox Cloud Gaming

It works, with one thing to know.

**Launch the game first, then hold the XBOX button on the phone for a second.**

This is not a workaround for a bug. A cloud gaming client binds a controller to a
streaming session at the moment it observes one *arrive*. A pad that already
existed when the stream started produces no arrival event, so the session ends up
with no controller bound — even though the client can read the pad perfectly
well. The giveaway is very specific: **the Xbox app's own menus and the Guide
button respond, but the game itself ignores everything.**

Holding XBOX unplugs the virtual pad and plugs it back in, which produces a real
arrival event without disturbing your connection.

The companion can also do this for you — tick **"Do this automatically when a
cloud game starts"**. It is off by default because it works by noticing which
programs are running, which is a guess, and a wrong guess unplugs a controller
somebody may be using.

## Customising the controller

Nothing about the on-screen pad is hardcoded. **Layouts → Edit** gives you drag to
move, resize, rotate, opacity, shape, label, show/hide, duplicate, delete,
snap-to-grid and undo, plus per-control tuning.

Each layout stores a separate arrangement for **landscape and portrait** and
switches automatically when you rotate. Four presets ship:

- **Standard** — a normal pad
- **Racing** — larger left stick, finer steering near centre, taller analogue
  triggers, no D-pad
- **Precision** — smaller, quicker right stick for aiming
- **Southpaw** — sticks and D-pad swapped

### About the sticks

A physical stick needs an inner deadzone, because it has spring slop and sensor
drift to hide. A touchscreen has neither — your thumb is exactly where you put it
— and the game at the far end is already applying a deadzone of its own on the
same assumption.

PhonePad used to add one on top of that. The result was that roughly a quarter of
your thumb's travel did nothing at all, and then the response arrived all at
once. Steering felt like it was stuck, and then snapped.

It now does the opposite: the moment your thumb leaves centre, the output starts
just past the game's own dead band, and the rest of your travel is spent on the
range the game actually responds to. The before-and-after curves are in
[`docs/benchmarks.md`](docs/benchmarks.md).

If a game still feels twitchy or numb, the amount of compensation is adjustable
per control in the layout editor.

## How it works

```
Touch  ->  TouchRouter  ->  ControllerState  ->  UDP (40 bytes)
                                                      |
                                                   Wi-Fi
                                                      v
                              pp-core  ->  verify  ->  ViGEmBus  ->  XInput
```

- **Binary UDP, 40 bytes, up to 250 Hz** — about 10 KB/s.
- **Send on change plus an unconditional repeat**, so a lost packet self-heals in
  a few milliseconds without retransmission. That is why this is UDP: a resent
  *stale* input would be worse than a dropped one.
- **X25519 pairing** authenticated by the six-digit code, and every packet
  carries a truncated HMAC plus a replay-window check. Nothing else on your
  network can inject controller input. See [SECURITY.md](SECURITY.md).
- **Watchdog**: no valid packet for 120 ms and every control snaps back to
  neutral; 2 s and the session is dropped. Measured on real hardware — killing
  the app mid-button-press released it in 131 ms.
- **Nothing leaves your network.** No account, no server, no telemetry.

Full reasoning, including why ViGEmBus was chosen over VHF, GameInput and vJoy,
is in [`docs/ADR-001-architecture.md`](docs/ADR-001-architecture.md).

## Building from source

```powershell
cd companion
cargo test --workspace          # 71 tests
cargo build --release --workspace

cd ..\phone
flutter test                    # 84 tests
flutter build apk --release --target-platform android-arm64
```

Release signing uses `phone/android/key.properties`, which is gitignored along
with the keystore. Without it the release build still compiles but is signed with
the debug key and logs a warning — deliberately not something you could
distribute by accident.

[`docs/engineering.md`](docs/engineering.md) has the architecture notes, the
invariants worth knowing, and the pitfalls already paid for.
[`docs/design.md`](docs/design.md) has the visual identity.

## Known limitations

- **A phone has one motor where a pad has two.** Game rumble reaches the phone,
  but strong and weak are combined into a single vibration.
- **Gyro aiming is not implemented.** The protocol reserves a flag; nothing else
  about it exists yet.
- **One phone at a time.**
- **ViGEmBus is end-of-life** (archived 2023). It works well and is what Parsec
  and DS4Windows use, but the backend sits behind a trait so a successor can
  replace it without touching anything else.

## Contributing

Bug reports and small fixes are the most useful things you can send. See
[CONTRIBUTING.md](CONTRIBUTING.md) — note that this repository is the *released*
source and merged changes are copied back into the working tree, and that commits
need a `-s` sign-off.

## Licence

[GPL-3.0-or-later](LICENSE). PhonePad is free software and stays that way: if you
distribute a modified version, you have to share the source.

The wire protocol under [`protocol/`](protocol/) is Apache-2.0 instead, so anyone
can write a compatible client or companion without their work becoming subject to
the GPL.

"PhonePad" and "DynShift" are trademarks and are not covered by either licence —
fork the code freely, but give your fork its own name. See
[TRADEMARK.md](TRADEMARK.md).

## Supporting it

PhonePad is free and open source, and the controller is not going behind a
paywall. If it saved you buying a controller, you can
[support development](https://phonepad.dynshift.com/support/).

---

<div align="center">
<sub>PhonePad by <b>DynShift</b> · not affiliated with Microsoft</sub>
</div>
