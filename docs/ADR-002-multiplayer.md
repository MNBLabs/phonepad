# ADR-002 — Several phones, several controllers

**Status:** Accepted (deferred to 1.2) · **Date:** 2026-11-09

## Context

People have asked for local multiplayer: two to four phones on one PC, each its
own controller, for couch co-op and party games. This records whether the
current architecture can do it, what it would take, and why it is not in 1.1.

## What already supports it

| Piece | State |
|---|---|
| Pairing | The PC already keeps a list of trusted phones, each with its own token. Nothing to change. |
| Wire protocol | Every input packet carries a `sessionId` and is authenticated with that session's key, so the PC can tell streams apart without new fields. `SESSION_RESP` already has a `serverBusy` status for refusing a phone. `FEEDBACK` has two reserved bytes, enough to tell a phone which player it is. |
| ViGEmBus | One client can plug in several Xbox 360 targets. Windows assigns each an XInput slot (four exist); the W3C Gamepad API, which browsers and Xbox Cloud Gaming read, sees all of them. |
| Load | Four phones at 250 Hz is 1,000 packets a second of 40 bytes each. The input thread handles that without effort. |

So it is feasible without a protocol break, and without touching pairing.

## What has to change

All of it is in `pp-core/src/server.rs`, the part of the system with the
strictest invariants.

1. **Sessions become a map.** `Option<Session>` becomes up to four sessions
   keyed by session id, each owning its own `Box<dyn VirtualPad>`. Today a new
   session from any paired phone replaces the current one; with several, a
   phone may only replace *its own* session, and a fifth phone gets
   `serverBusy`.
2. **The watchdog becomes per session.** The guarantee that a dead connection
   releases every control within 120 ms is the most important safety property
   PhonePad has. It must hold for each pad independently: one phone dropping
   must neutralise exactly its pad and nobody else's. This is where the work
   and the risk are, and it is tested against a virtual clock today; those
   tests would be extended per session before anything else changes.
3. **Player identity.** A phone is told its player number in `FEEDBACK` and
   shows it, the way a pad's ring light does. The number follows the XInput
   slot Windows actually assigned, read back from ViGEm, not an order PhonePad
   invents; otherwise "player 2" on the phone and in the game could disagree.
4. **Re-attach and rumble per pad.** Re-attach (for browsers that bind pads on
   arrival) currently unplugs the one pad; it would re-plug all of them in slot
   order, so slots do not shuffle. Rumble is read per target and goes back to
   the phone that owns it.
5. **The companion window** lists connected players, with a per-player
   disconnect, instead of one "Phone" row.
6. **Phone UI.** The controller shows its player number; nothing else changes.

## Why not in 1.1

- **It cannot be validated honestly yet.** Development and acceptance testing
  happen on one phone. `pp-fakephone` can drive extra sessions for automated
  tests, but whether two people can play, which slot a game picks, and how a
  cloud stream treats a second pad are questions for real hardware.
- **1.1 already changes the pairing exchange** (a security fix) and discovery.
  Landing a rework of the session and watchdog code in the same release would
  put three risky changes into one set of release notes, and make any
  regression harder to attribute.
- A half-done version, for example two phones without per-session watchdogs,
  is worse than none: a stuck button on player 2 is exactly the failure the
  design exists to make impossible.

## Decision

Planned for **1.2**, built in this order: per-session watchdog with tests,
session map and `serverBusy`, player number in `FEEDBACK`, re-attach and rumble
per pad, then the two UIs. Prerequisite: a second Android phone in the test
setup, and the acceptance checklist extended with two-player items before the
release, not after.

Pricing follows the rule in `ROADMAP.md`: what makes the controller work is
free. A second controller is the controller working for a second person.
