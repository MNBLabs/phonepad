# Roadmap

No dates. This is a side project, and dates on a side project are fiction.

Things are listed in roughly the order they are likely to happen, and something
being absent is not a rejection — open a discussion.

## Next

- **Several phones, several controllers.** Local multiplayer with up to four
  phones on one PC, each its own pad with its own player number. Designed in
  [`docs/ADR-002-multiplayer.md`](docs/ADR-002-multiplayer.md), planned for
  1.2.
- **Per-game tuning profiles.** The right amount of deadzone compensation
  depends on the game, and nothing on the PC can measure it. A short calibration
  — ramp the output until the game just starts responding — plus a way to save
  the result per game.
- **Sharing layouts and profiles.** A versioned, portable export format so a
  layout that works for a specific game can be handed to someone else. Data
  only; see `protocol/LAYOUT_FORMAT.md`.
- **Run the companion at login**, so a working setup survives a reboot without
  being started by hand.

## Later

- **Gyro aiming.** Tilt to nudge the right stick. The protocol reserves a flag;
  nothing else exists yet.
- **A successor to ViGEmBus.** It is archived and works well, and the backend
  already sits behind a trait so replacing it touches one file.

## Not planned

- **An account system, or any server.** The controller talks to your PC over
  your own network and nothing else, and there is no account to make. That is a
  feature. (The ad and sponsor strips on the setup screens are the only things
  that reach the internet; the privacy policy says exactly what they send.)
- **Putting the controller behind a paywall.** The free app is the product. If
  paid extras ever appear they will be additions — themes, advanced tuning,
  motion — not a tax on what already works. Connecting matters most of all:
  hotspot and USB tethering are free, and will stay free, because a controller
  that cannot reach the PC is not a controller.
- **iOS.** Not for lack of interest; there is no way to ship this on iOS that
  works as well, and doing it badly is worse than not doing it.
