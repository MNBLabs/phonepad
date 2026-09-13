# Security

## The threat model, stated plainly

PhonePad puts a controller on your PC that is driven over the network. That is
worth being careful about, so here is exactly what it does and does not defend
against.

**What it defends against.** Anything else on your local network injecting
controller input. Every input, feedback, control and session packet carries a
truncated HMAC-SHA256 tag over its own contents, keyed by a session key derived
per session from a long-term token and both sides' nonces. Packets that fail the
tag are counted and dropped. A 64-slot sliding replay window rejects duplicates
and anything too old, so a captured burst cannot be replayed. Sequence numbers
for input and for control messages are tracked separately, so neither stream can
invalidate the other's replay state.

The long-term token is established by an X25519 exchange authenticated with a
six-digit code shown on the PC. The token itself is never transmitted. An
attacker who watches the entire pairing exchange and substitutes their own
public key does not learn the token — there is a test for exactly that.

**What it does not defend against.** Someone who can already run code on your
PC, or who can read your PC's config file, has your token and can drive the
controller. That is the same position as someone holding a physical pad. There
is no attempt to defend against a compromised host.

**What it assumes.** That your local network is where you think it is. Discovery
is a LAN broadcast, and the pairing window is open for 120 seconds after you
click "Pair a phone". During that window, someone on your network who can see
the six-digit code can pair. The window closes on the first successful pairing.

**Ports.** The companion binds UDP 47800 (discovery) and 47801 (input) and adds
a firewall rule scoped to private and domain profiles. It never opens an
outbound connection, never contacts a server, and sends no telemetry.

## Downloaded content

Layouts and profiles are **data, not code**. The import path validates against a
strict schema: every numeric is clamped to the range its editor slider allows,
the control count is capped, labels are length-limited with control characters
stripped, and unknown fields are dropped. There is no mechanism by which a
shared layout can execute anything. If you find one, that is a serious bug and
this is exactly the report we want.

## Reporting a vulnerability

**Please do not open a public issue.**

Use GitHub's private vulnerability reporting on this repository
(Security → Report a vulnerability), which is the fastest route. If that is not
available to you, the maintainer's contact address is in the repository profile.

Please include what you were able to do, the steps to reproduce it, and the
versions of the app and companion. A proof of concept helps a lot.

This is a side project maintained by one person, so response times are
best-effort rather than contractual. Expect an acknowledgement within a few
days. If a fix is warranted it will ship in the next release with credit, unless
you would rather not be named.

## Supported versions

The most recent release. There are no long-term support branches.
