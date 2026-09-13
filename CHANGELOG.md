# Changelog

Notable changes, newest first. Dates are ISO. Versions follow
[semantic versioning](https://semver.org/): the wire protocol version is
separate and is stated per release when it changes.

## [1.0.0] — 2026-09-13

First public release.

### Added
- The companion can be given a name to show on the phone (Advanced →
  Network); it defaults to the computer name.

- Android phone as a wireless Xbox-compatible controller for Windows, over
  Wi-Fi. No Bluetooth, no cable.
- Automatic PC discovery and one-time pairing with a six-digit code.
- Layout editor: move, resize, rotate, opacity, shape, label, show/hide,
  duplicate, delete, snap-to-grid and undo, plus per-control tuning. Separate
  arrangements for landscape and portrait, switched automatically.
- Four presets — Standard, Racing, Precision and Southpaw.
- **Game rumble reaches the phone.** Read back from ViGEmBus as a driver
  notification and played on the phone's motor.
- Automatic re-attach when a cloud gaming client starts, off by default, plus a
  manual re-attach by holding the XBOX button on the phone.
- First-run introduction explaining that a PC component exists.
- Diagnostics on both sides: round trip, jitter, packet rate, loss, and a
  breakdown of why anything was rejected.

### Notes on controller feel

The touch sticks were rebuilt rather than retuned. A physical stick needs an
inner deadzone because it has spring slop and sensor drift; a touchscreen has
neither, and the game at the far end is already applying its own. Stacking ours
on top meant roughly a quarter of the thumb's travel produced nothing at all,
and the response then arrived all at once. Output now starts just past the
game's own dead band and spends the remaining travel on the range the game
actually responds to. Measured before-and-after curves are in
`docs/benchmarks.md`.

Triggers were sending a flat 255 — the default layout had digital throttle and
brake. They are analogue by default now.

### Known limitations

- A phone has one motor where a pad has two, so strong and weak are combined.
- Gyro aiming is not implemented.
- One phone at a time.
- ViGEmBus is end-of-life (archived 2023). It works, and it is what Parsec and
  DS4Windows use, but the backend sits behind a trait so a successor can replace
  it without touching anything else.
