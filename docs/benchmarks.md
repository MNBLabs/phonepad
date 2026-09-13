# Measured performance

All figures below were measured on the actual hardware on 2026-08-15. Nothing
here is a target, an estimate or a round number chosen because it looked good.
Where something was not measured, it says so.

## Hardware

| | |
|---|---|
| Phone | Samsung Galaxy S24 Ultra, Android 16, arm64 |
| PC | Windows 11 Pro 23H2 |
| Driver | ViGEmBus 1.16.112 |
| Network | 2.4/5 GHz Wi-Fi, phone and PC on the same `192.168.x.x` subnet |

## 1. Virtual controller, in isolation

`pp-vigem-probe` writes a known state to the ViGEm target and polls Windows'
own XInput API until it reads the same state back.

```
26 / 26 checks passed
write -> observable via XInput: avg 0.09 ms, max 0.53 ms
```

15 buttons, 5 trigger levels and 6 stick positions all round-tripped exactly.
**The virtual pad contributes essentially nothing to the latency budget.**

Note: the public `XInputGetState` masks out the Guide button. Only
`XInputGetStateEx` (xinput1_4.dll, ordinal 100) reports it. That is a Windows
API behaviour, not a ViGEm limitation — Guide is delivered correctly, and the
probe resolves the Ex entry point dynamically so it can verify it honestly.

## 2. Network path, synthetic sender

`pp-fakephone` over loopback at 250 Hz, backend = real ViGEm:

```
sent 250/s  ·  accepted 1500/1500  ·  loss 0.0 %  ·  jitter 0.0 ms
```

XInput independently observed 454 distinct state changes, a left-stick X range
of −29999..30000, triggers reaching 255, and 7 distinct button patterns — so the
whole chain, not just the socket, was verified.

### A real finding from this run

The first attempt reported **65 Hz when 250 Hz was requested.** The cause was
not the protocol: Windows rounds `SO_RCVTIMEO` up to the ~15.6 ms scheduler
tick, so a "1 ms" socket read timeout actually costs 15 ms per drain and caps
the send loop. `thread::sleep` is quantised the same way. Switching the sender
to a non-blocking socket with `yield_now` pacing produced an exact 250 Hz.

This matters beyond the test tool: **any sleep-based pacing on Windows is coarse.**
The companion's receive path is unaffected because it blocks on `recv_from`,
which wakes the instant a packet lands.

## 3. End to end, real phone over real Wi-Fi

Debug build, portrait, idle apart from the controller:

```
CONNECTED | Galaxy S24 Ultra | 251 pps | loss 0.0% | jitter 0.2 ms | rtt 5.9 ms
CONNECTED | Galaxy S24 Ultra | 256 pps | loss 0.0% | jitter 0.2 ms | rtt 6.0 ms
CONNECTED | Galaxy S24 Ultra | 256 pps | loss 0.0% | jitter 0.2 ms | rtt 8.0 ms
```

Release build:

```
CONNECTED | Galaxy S24 Ultra | 259 pps | loss 0.0% | jitter 0.2 ms | rtt 5.4 ms
CONNECTED | Galaxy S24 Ultra | 260 pps | loss 0.0% | jitter 0.1 ms | rtt 5.2 ms
CONNECTED | Galaxy S24 Ultra | 260 pps | loss 0.0% | jitter 0.1 ms | rtt 4.9 ms
```

| Metric | Result |
|---|---|
| Round trip (phone → PC → phone) | **4.9 – 8.0 ms**, typically ~5.2 ms |
| Packet rate | **249 /sec**, matching the configured target |
| Packet loss | **0.0 %** across 42,805 consecutive packets |
| Jitter | **0.1 – 0.6 ms** |
| Rejected packets | **0** — no bad MACs, no replays, no malformed |

### Correction: the rate figures above were originally overstated

The first version of this document reported "250 – 260 /sec", and the phone's
own panel once showed *sending 312/sec* against *PC accepting 259/sec*. Both
were a measurement bug, not real traffic and not real loss.

Each side accumulated a count and published it whenever more than a second had
passed — but the check only runs on a periodic tick (250 ms on the phone, 50 ms
on the PC). The window was therefore 1.0–1.25 s on the phone and 1.0–1.05 s on
the PC, and dividing by an assumed *exactly* one second inflated the result by
up to 25% and 5% respectively.

Both now divide by the window that actually elapsed. Measured immediately after
the fix, with the phone idle:

```
Sending      249 /sec
PC accepting 249 /sec
```

The two agree, which is the result that should have been visible all along.
Latency, jitter and loss were never affected — they are not derived from this
window.

### What the round-trip figure includes and excludes

It is measured from a timestamp the phone puts in an input packet and the PC
echoes back in its next feedback packet. So it covers phone → PC, packet
validation, the ViGEm update, and PC → phone.

It does **not** include the touchscreen's own sampling and reporting latency, or
the display pipeline at the far end. Those are outside anything this software
can observe, and are not claimed.

One bias worth stating: feedback is sent at 20 Hz, so the echoed packet may be
up to one input period (~4 ms at 250 Hz) old when the feedback goes out. The
figure is therefore a slight over-estimate, not an under-estimate.

## 4. Display

The Kotlin layer requests the fastest display mode at the current resolution.
Confirmed on device via the app's own readout:

```
Display: 120 Hz (max 120)
```

Samsung's adaptive refresh would otherwise settle at 60 Hz for an app it judges
idle, which is exactly wrong for a touch controller.

## 5. Failsafe timing

The single most important number here. A button was held on the phone and the
app was force-stopped mid-press:

```
[12:45:59] WARN  input stalled (131 ms) - controls released
[12:46:01] INFO  session ended: no packets for 2.0s
```

XInput observed exactly `neutral → A → neutral`, with 3 state changes total.
**Nothing stayed held.**

| Event | Configured | Measured |
|---|---|---|
| Controls released after silence | 120 ms | **131 ms** |
| Session dropped after silence | 2 s | **2.0 s** |

The 11 ms overshoot is one iteration of the receive loop's 15 ms read timeout,
which is expected and bounded.

## 6. Stick response

Measured 2026-09-11 on the S24 Ultra, landscape, left stick (`size 0.34` →
radius 245 physical px at density 600). A 2.5 s `adb input swipe` from the stick
centre to the ring edge, captured with `pp-headless --trace-csv` and read by
`tools/stick_transfer.py`.

The complaint this addresses was specific: nothing at the start of the travel,
then suddenly too much — in racing games and when panning a camera.

### The cause was two deadzones, not one

A game applies its own inner deadzone to raw XInput, typically 0.15–0.25,
because it assumes a physical stick with spring slop and sensor drift to hide.
PhonePad was applying 0.12 of its own on top. The table below composes both, so
the right-hand columns are what the player actually feels. `game` assumes an
inner deadzone of 0.15, rescaled — the common case.

| Thumb travel | old sends | old, in game | new sends | new, in game |
|---:|---:|---:|---:|---:|
| 5%   | 0.0%  | **0.0%** | 16.0% | 1.2% |
| 10%  | 0.0%  | **0.0%** | 18.7% | 4.3% |
| 15%  | 3.4%  | **0.0%** | 21.9% | 8.1% |
| 20%  | 9.1%  | **0.0%** | 25.5% | 12.3% |
| 25%  | 14.8% | **0.0%** | 29.4% | 17.0% |
| 30%  | 20.5% | 6.4%     | 33.6% | 21.9% |
| 50%  | 43.2% | 33.2%    | 52.5% | 44.2% |
| 75%  | 71.6% | 66.6%    | 79.7% | 76.2% |
| 100% | 100%  | 100%     | 100%  | 100%  |

**The first quarter of the thumb's travel did nothing at all**, and the response
then climbed at 1.3% of game input per 1% of travel. A thumb on glass does not
slide smoothly — it sticks and breaks free in a 2–3 mm jump, which on a 10.4 mm
radius is ~12% of travel, ~16% of game input arriving at once. That is the whole
of "nothing, then suddenly too much".

### Measured, after

```
 travel   output       %
     0%     4916   15.0%      dead band      0% of travel
    25%     9466   28.9%      largest step   5.6% per 5% of travel
    50%    16239   49.6%      832 samples over 2250 ms
   100%    32767  100.0%
```

Output leaves zero on the first movement past the noise floor and climbs
monotonically with no step larger than 5.6% of full scale per 5% of travel. The
one deliberate discontinuity is the anti-deadzone lift at the noise floor: it is
sized to land just past the game's own dead band, so what the *player* sees
starts from zero.

Composed with a game deadzone of 0.15 the result is mildly progressive — 1.2%,
4.3%, 8.1%, 12.3% at 5/10/15/20% of travel — which is fine control near centre
without losing full lock.

### Triggers were digital

`analogSlide` defaulted to false, so `computeTrigger` never ran and both
triggers sent a flat 255. The default layout had on/off throttle and brake.

After: a 150 px slide up a 288 px trigger produced every value from 122 to 255,
continuously — matching `255 × (1 − 150/288) = 122` exactly. A tap is still a
full press.

### What this measurement does not cover

`adb input` injects synthetic events, which carry none of a real digitiser's
noise. The 0.02 noise floor is therefore set conservatively from the touch
slop a resting thumb is expected to produce, and has **not** been validated
against a real stationary finger. If a resting thumb is seen to flutter the
axis, that constant is the one to raise.

## 7. Rumble, end to end

Measured 2026-09-11. `pp-vigem-probe --rumble` drives `XInputSetState` against
the virtual pad — the same call a game makes — with a triangle ramp over 8 s.
The companion reads it back from ViGEmBus as a driver notification and forwards
it in the FEEDBACK packet; the phone plays it.

Read from the phone's own `dumpsys vibrator_manager`, the motor followed the
PC's descending ramp step for step:

```
amplitude=0.47  0.45  0.43  0.41  0.39  0.37   (dev.phonepad.phonepad)
```

Path confirmed: game → ViGEmBus → companion → Wi-Fi → phone motor.

This corrects an earlier limitation. `vigem-client` 0.1.4 *does* expose the
notification API; it sits behind the `unstable_xtarget_notification` feature
flag, which is why it was previously read as absent.

## Not yet measured

Honest gaps, rather than invented figures:

- **Battery drain** over a long session.
- **CPU usage** on either side. Both appear negligible by observation, but
  "appears negligible" is not a measurement.
- **Sustained frame rate on the phone under heavy touch** — the architecture
  avoids rebuilds on the input path by construction, but this has not been
  profiled with the Flutter timeline.
- **Behaviour on a congested or weak Wi-Fi link.** Every figure above is from a
  healthy network; loss handling is covered by unit tests but not yet by a real
  degraded-link run.
- **Digitiser noise under a stationary finger** — see section 6.
- **Battery and thermals during a long cloud-gaming session.**

Xbox Cloud Gaming *was* subsequently played through end to end; that run is
recorded in `docs/acceptance.md` item 19. This section originally listed it as
untested and was not updated at the time.

## Reproducing

```powershell
cd companion
cargo run --release -p pp-vigem-probe                       # section 1
cargo run --release -p pp-core --bin pp-headless            # then, separately:
cargo run --release -p pp-fakephone -- --rate 250 --seconds 20   # section 2
cargo run --release -p pp-vigem-probe -- --watch --seconds 15    # independent check
```

For section 3, run the companion and connect the phone; the numbers print once
a second in headless mode and appear live in the companion window.
