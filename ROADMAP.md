# Roadmap

No dates. This is a side project, and dates on a side project are fiction.

Things are listed in roughly the order they are likely to happen, and something
being absent is not a rejection — open a discussion.

## Next

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
- **A second phone**, for local two-player.
- **A successor to ViGEmBus.** It is archived and works well, and the backend
  already sits behind a trait so replacing it touches one file.

## Not planned

- **An account system, or any server.** PhonePad talks to your PC over your own
  network and contacts nothing else. That is a feature.
- **Putting the controller behind a paywall.** The free app is the product. If
  paid extras ever appear they will be additions — themes, advanced tuning,
  motion — not a tax on what already works.
- **iOS.** Not for lack of interest; there is no way to ship this on iOS that
  works as well, and doing it badly is worse than not doing it.
