# Contributing to PhonePad

Thanks for looking. Bug reports and small fixes are the most useful things you
can send; please open an issue before a large change so we can agree on the
shape of it first.

## Before you start

**PhonePad is developed in a private repository and published here in
releases.** This repository is the released source, not the working tree, so
its history is a sequence of release commits rather than day-to-day work.

What that means for you in practice:

- Pull requests are reviewed and merged here as normal.
- Merged changes are then copied back into the working repository, **with your
  commit authorship preserved**, and appear in the next release.
- Don't be surprised if the file you edited looks slightly different in the next
  release: it has been through the working tree.

If a PR sits for a while, ping it. This is a side project.

## Development

```
companion/    Rust workspace, the Windows side
phone/        Flutter app, with its own android/ inside
protocol/     Wire-format spec and the fixtures both codecs are tested against
```

```powershell
cd companion
cargo test --workspace
cargo clippy --workspace --all-targets    # must be clean

cd ..\phone
flutter test
dart analyze
```

On Windows, `cargo test` prompts for firewall access each time the loopback
test binary is rebuilt — it binds real UDP sockets. Decline it; Windows does not
filter loopback and the tests pass regardless.

Both codecs are asserted byte-for-byte against `protocol/vectors.json`. If you
change the wire format, regenerate it and expect both test suites to move
together:

```powershell
cd companion
cargo run -p pp-protocol --example gen_vectors > ..\protocol\vectors.json
```

[`docs/engineering.md`](docs/engineering.md) documents the architecture, the
invariants worth knowing, and the things that have already bitten someone.
[`docs/design.md`](docs/design.md) is the source of truth for anything visual.

## Style

Match the surrounding code. The one thing worth stating explicitly: comments
here explain **why**, not what. A comment that restates the line below it is
noise; a comment that records the failure which motivated the code is the most
valuable thing in the file. Several of them name a specific bug — keep that
habit.

## Signing off

Every commit needs a `Signed-off-by` line:

```
git commit -s -m "your message"
```

By signing off you certify the [Developer Certificate of Origin](https://developercertificate.org/)
— that the work is yours to give — **and** you agree to the Contributor Terms
below.

### Contributor Terms

By contributing, you grant DynShift (the "maintainer"):

1. a perpetual, worldwide, non-exclusive, royalty-free, irrevocable licence to
   reproduce, modify, distribute and sublicense your contribution as part of
   PhonePad **under GPL-3.0-or-later**; and

2. the same rights **under other licence terms of the maintainer's choosing**,
   including proprietary terms, for the purpose of distributing PhonePad or
   works derived from it.

You keep the copyright in your contribution. You may use it however you like
elsewhere. Grant 2 exists for one specific reason, stated plainly: PhonePad is
GPL so that nobody can take it closed-source, and the maintainer intends to
offer paid PhonePad builds alongside the free one. A copyright holder is not
bound by their own licence, so the maintainer can do that with their own code —
but not with yours, unless you say so here. Without grant 2, any file you touch
becomes permanently unusable in a paid build, which in practice means such
contributions cannot be accepted.

If you are contributing on behalf of an employer, make sure you have the
authority to grant this.

This is a deliberately lightweight arrangement, chosen over a separate signed
CLA to keep the workflow to one `-s` flag. It is written by an engineer, not a
lawyer. If you need something more formal before contributing, say so in the
issue and we will sort it out.

## Reporting a security problem

Don't open an issue. See [SECURITY.md](SECURITY.md).
