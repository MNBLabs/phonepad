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

Six digits are only a secret if nobody can test guesses against them offline.
Since 1.1.0 each side **commits** to its proof of the code before the other
reveals one (pairing v2, `protocol/PROTOCOL.md`), and the PC retires the code the
moment it reveals a proof, so a device that does not know the code gets one
guess per code, and at most five attempts per pairing window:

- A device pretending to be a phone learns nothing from the PC's reply, and a
  proof it extracts by committing a guess is for a code that has already been
  replaced on screen.
- A device pretending to be the PC cannot make the phone reveal anything: the
  phone checks the PC's proof, against a commitment made before the PC could
  have known the code, before it sends its own.
- Phones and companions on 1.0 cannot pair with 1.1 (each refuses the other by
  name and asks for an update). There is no fallback, so the weaker scheme
  cannot be requested. Phones already paired keep working: sessions are
  unchanged.

### Fixed in 1.1.0: the pairing code could be recovered offline

In 1.0.0 the PC's first pairing reply carried an HMAC over the code, keyed by a
secret the requester itself had a share in. Any device that could reach UDP
47800 while the pairing window was open could send one pairing request, try all
million codes against the reply in well under a second, and pair as a trusted
phone. This was found while reviewing a proposal to open PhonePad's ports on
public networks, and confirmed against the 1.0.0 companion with a test that
recovered the code in 174 ms; the same test now runs against 1.1.0 and fails
to find anything. If you paired a phone on a network you do not control while
running 1.0.0, check **Trusted phones** in the companion and forget anything you
do not recognise.

**What it does not defend against.** Someone who can already run code on your
PC, or who can read your PC's config file, has your token and can drive the
controller. That is the same position as someone holding a physical pad. There
is no attempt to defend against a compromised host.

**What it assumes.** That your local network is where you think it is. Discovery
is a broadcast on each local link (the Wi-Fi, or the hotspot or USB tethering
the phone is providing), and the pairing window is open for 120 seconds after
you click "Pair a phone". During that window, someone who can see the six-digit
code on your screen can pair. The window closes on the first successful pairing,
or after five attempts.

**Ports.** The companion binds UDP 47800 (discovery) and 47801 (input). The rule
it can add is scoped to private and domain networks. It **never** opens the ports
on public networks in general: that would include every café, hotel and airport
network the PC joins. When a network the user owns (a phone's hotspot or USB
tethering) is public, the companion offers to mark *that one network* private,
with Windows' own consent prompt, and nothing else.

**Outbound connections.** The controller traffic never leaves the local network,
and there is no telemetry. Two things do reach the internet, both described in
the [privacy policy](https://phonepad.dynshift.com/privacy/): the companion
fetches a small, strictly validated JSON file for its sponsor strip from
`phonepad.dynshift.com` at most twice a day, and the official Android build
shows one ad through Google's Mobile Ads SDK on its setup screens. Builds from
source carry no ad unit and never start the ad SDK.

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
