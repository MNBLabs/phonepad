# Engineering notes

The architecture, the invariants worth knowing, and the things that have
already bitten someone. Read this before changing anything in `phone/lib/ui`,
`companion/crates/pp-core` or `protocol/`.

## Layout

```
phone/       Flutter app (its own android/ inside, Kotlin for the platform channel)
companion/   Rust workspace, Windows only
protocol/    Wire spec + vectors.json, the fixture both codecs are tested against
site/        Astro static site, deployed to GitHub Pages
brand/       The mark, the icon layers and the fonts; see docs/design.md
tools/       Measurement scripts and the release sync
docs/        Design, ADR, benchmarks, acceptance checklist
```

Crates: `pp-protocol` (codec, MAC, pairing, replay, no I/O), `pp-core` (sockets,
sessions, watchdog, backends, no UI), `pp-companion` (the egui window),
`pp-fakephone` and `pp-vigem-probe` (test harnesses).

## Commands

```powershell
cd companion
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build --release --workspace

cd ..\phone
flutter test
dart analyze
flutter build apk --release --target-platform android-arm64

# Regenerating the shared vectors. Note the cwd: the path is relative.
cd companion
cargo run -p pp-protocol --example gen_vectors > ..\protocol\vectors.json
```

CI runs clippy with `-D warnings` on a newer toolchain than most machines
have. Run that exact command locally before pushing; plain `cargo clippy`
passes things CI rejects.

## The device

A phone reachable over both USB and Wi-Fi debugging shows up as two adb
transports, and a bare `adb shell` fails with "more than one device". Use
`-s <serial>`; `adb devices -l` is the source of truth and the Wi-Fi address
changes with the DHCP lease.

`sendevent` is blocked by SELinux on Samsung devices, so synthetic multi-touch
is not possible: `adb shell input` is single-pointer only, and multi-touch has
to be tested with `flutter test` (see `phone/test/controller_surface_test.dart`)
or by hand. `getevent -lt /dev/input/eventN` reads the real touchscreen, which
is how a reported touch bug gets confirmed rather than guessed at.

## Invariants

**Protocol.** Never emit `i16::MIN` for an axis: it has no positive
counterpart and breaks symmetric scaling, and the decoder rejects it. The MAC
covers a specific byte range per packet type; changing a field's position
changes what is authenticated. Input and control messages have **separate**
sequence counters, so neither stream can invalidate the other's replay state.
`protocol/vectors.json` is generated from fixed keys and is byte-stable; if it
changes, both codecs must change with it, and CI checks that it is not stale.

**Touch.** `TouchRouter` rebuilds the whole controller state from the set of
live pointers on every event. It never toggles incrementally. That is what makes
a stuck button structurally impossible, and it is worth preserving.

A dropped connection must **not** clear the pointer map. The fingers are still
on the glass; forgetting them leaves them dead until they are lifted. Nothing is
sent while disconnected and the PC watchdog releases within 120 ms, so nothing
can stick.

**Stick maths.** Exact neutral must return exactly zero *before* the
anti-deadzone floor is applied. Applying the floor unconditionally makes every
game read the pad as permanently deflected. There is a test for this; do not
delete it.

**The input thread must not sleep.** It drains the socket and ticks the
watchdog. Re-attach needs a 400 ms gap in the pad, and that gap is a deadline
the loop honours, not a `thread::sleep`.

**The companion window is a reader.** It never sits in the input path. If the UI
froze completely the controller would keep working. Keep it that way.

**Identity.** `docs/design.md` is the source of truth for colour, type and
shape. The same values live in `phone/lib/ui/theme.dart`,
`companion/crates/pp-companion/src/brand.rs` and `site/src/styles/global.css`
by hand; when one changes, all three change. The mark is never redrawn: the
path data in `brand/mark.svg` is embedded verbatim in each.

## Running the tests on Windows

`cargo test --workspace` runs `pp-core/tests/loopback.rs`, which binds real UDP
sockets. Windows Firewall prompts the first time each freshly built test binary
does that, so it reappears after most code changes.

**Decline it.** Windows Firewall does not filter loopback, so the tests pass
either way, and allowing it adds a rule for a throwaway binary that will never
be run again.

## Pitfalls already paid for

- Windows rounds `SO_RCVTIMEO` up to the ~15.6 ms scheduler tick, which caps a
  blocking sender at ~65 Hz however fast you ask it to go. `pp-fakephone` uses a
  non-blocking socket and `yield_now` because of this.
- A rate published on a periodic tick and divided by an assumed exactly-one
  second reads high; divide by the elapsed window.
- Re-measuring RTT against a *stale* echo makes it climb forever once sending
  stops.
- `XInputGetState` masks the Guide button. Only `XInputGetStateEx`
  (xinput1_4.dll, ordinal 100) reports it.
- Rumble is a driver *notification*, not a pollable value. It needs the
  `unstable_xtarget_notification` feature on `vigem-client`.
- egui applies no OpenType features and its bundled font has no U+25CF. The
  companion's UI fonts have tabular figures baked into the cmap for that
  reason, and bullets are painted circles.
- The default `flutter_test` surface is 800x600. A widget wider than that has
  its right-hand side off-screen and never hit-tested, which reads as a
  multi-touch failure that is really a viewport.
- Samsung's launcher does not parallax adaptive icons, but others do: the icon's
  foreground layer is the whole designed tile and the background is only its
  edge continued outward, so any parallax reveals more field rather than a
  seam.

## Conventions

Comments explain **why**. A comment that restates the line below it is noise; a
comment that records the failure which motivated the code is the most valuable
thing in the file. Several here name a specific bug; keep that habit.

Commits: a subject line that says what changed, then prose explaining why, in
paragraphs. No trailers.
