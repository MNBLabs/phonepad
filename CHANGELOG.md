# Changelog

Notable changes, newest first. Dates are ISO. Versions follow
[semantic versioning](https://semver.org/): the wire protocol version is
separate and is stated per release when it changes.

## [1.1.0] — 2026-11-09

Update **both** the app and the companion. A phone already paired with a PC
keeps working across the update; pairing a new phone needs 1.1 on both sides.

### Security
- **Pairing could be broken by anyone on the network while the pairing window
  was open.** In 1.0.0 the PC's first pairing reply let the device asking for
  it recover the six-digit code offline in a fraction of a second, and pair as
  a trusted phone. Pairing now commits before it reveals, on both sides, so a
  device without the code gets one guess per code; every attempt replaces the
  code on screen, and pairing stops after five. Details in `SECURITY.md`.
  Wire protocol: pairing v2 (new `PAIR_COMMIT` and `PAIR_REVEAL` messages);
  sessions and input are unchanged.

### Added
- **Phone hotspot and USB tethering** (#2). With no Wi-Fi router, connect the
  PC to the phone's hotspot or turn on USB tethering, and the phone finds the
  PC. It used to search only the network Android uses for the internet, which
  is mobile data when the phone is sharing it.
- **The companion tells you when Windows is blocking PhonePad**, on which
  network, and why. For your own hotspot or tethering it offers to mark that one
  network private, with one administrator prompt. It never opens PhonePad's
  ports on public networks in general.
- **Ads, kept out of the way.** The phone's home, layouts and settings screens
  have one ad strip at the bottom: never on the controller, the editor or the
  first-run pages, never over anything, and not at all where there is no room
  for it, such as a phone held sideways. Consent is asked where the law
  requires it, and can be changed in Settings. The companion has one sponsor
  strip. When there is no ad to show, both strips show PhonePad's own message
  instead of an empty box.
- The companion shows its version in the title bar, which networks it is on and
  how Windows treats each, and how many discovery probes it has heard, which
  tells a "phone can't find the PC" problem on the network apart from one in
  the reply.

### Fixed
- After USB tethering was switched off and on mid-session, the phone could sit
  on "reconnecting" until the app was restarted. It now starts a fresh socket
  when a reconnect attempt goes unanswered.
- The phone labelled the mobile-data address "Wi-Fi address". It now lists each
  local network by what it is.
- Two notices in the companion were shown with long runs of spaces in them.

### Not in this release
- Several phones as separate controllers. It fits the architecture and is
  planned for 1.2; `docs/ADR-002-multiplayer.md` explains why it is not here.

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
