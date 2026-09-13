# PhonePad visual identity

The source of truth is the Figma file (three frames on the *Logo* page:
MonoChrome, MonoChrome-invert, Color). Everything below is derived from it, and
every surface (the Android app, the Windows companion, the website, the app
icon, the social image) uses these same values. When something here and
something in code disagree, the code is wrong.

## The mark

An eight-petal asterisk: two identical four-petal shapes, one at 0° and one at
45°, drawn as a 10.19 px stroke on a 128 px tile so the arms fill solid; it
spans about 81% of the tile. Each petal tapers to a point at the centre and ends in a rounded tip; the centre is
a small hollow square where the tapers meet.

It is a burst from a point. That is the whole idea of the product: a phone
radiating a controller out to a PC, over the air. The mark is never redrawn,
simplified, or restyled per surface. The path data lives in `brand/mark.svg`
and is embedded verbatim in the app, the companion and the site.

Colour treatments, from the Figma frames:

| Frame | Use |
|---|---|
| MonoChrome (ink on white) | inline on light surfaces, favicon, print |
| MonoChrome-invert (white on ink) | inline on dark surfaces, the controller screen |
| Color (white mark, `overlay`, on the moving gradient) | app icon, hero, social image |

## The light

The Color frame is a gradient shader with three stops:

```
#FFE99E   pale yellow     0.0
#8178FF   periwinkle      0.5
#FF009B   magenta         1.0
```

blurred into each other like light through frosted glass, with the mark in
white at `overlay` blend on top. That field is the product's colour. It is used
**only** where the mark lives: the icon, the hero behind the phone, the first
onboarding page, the social image. Never as a button, never as text, never as a
page background, never animated on an operating surface.

Everything else is built from the periwinkle stop and neutrals tinted with it.

## Palette

Two grounds. **Paper** is for surfaces you set things up on: the website, first
run, home, settings, the editor, the companion window. **Night** is for the
surface you play on: the controller and its pause menu, drawn over somebody's
game, on an OLED, in a dim room. The transition from one to the other is the
moment the phone becomes a controller, and it is meant to be felt.

### Paper

| Token | Value | Role | Contrast on paper |
|---|---|---|---|
| `paper` | `#F7F7FC` | page ground, cool white tinted from the periwinkle | |
| `raised` | `#FFFFFF` | a surface that sits on paper (a tile, a field) | |
| `sunken` | `#EEEDF7` | a surface set into paper (a track, a well) | |
| `line` | `#E2E1EE` | hairlines | |
| `ink` | `#15142B` | primary text, the mark | 16.9 |
| `ink2` | `#5A5878` | secondary text, tinted not grey | 6.3 |
| `ink3` | `#807EA0` | tertiary text, disabled, placeholders | 3.6 |
| `brand` | `#8178FF` | the periwinkle stop itself: decoration, large marks, focus rings | 3.2 |
| `accent` | `#5A4FE6` | the one interactive colour: buttons, links, the active state | 5.3 (white on it 5.7) |
| `accent-deep` | `#4A40D4` | pressed accent | 6.6 |
| `good` | `#177F4A` | connected, paired | 4.7 |
| `warn` | `#935B00` | reconnecting, unpaired, a notice | 5.3 |
| `bad` | `#C43837` | disconnected, an error | 5.0 |

### Night

| Token | Value | Role | Contrast on night |
|---|---|---|---|
| `night` | `#0B0B14` | the controller ground, periwinkle hue at 4% lightness | |
| `night-hi` | `#15152A` | the pause panel | |
| `night-line` | `#24243C` | hairlines | |
| `night-ink` | `#F3F2FF` | text | 17.7 |
| `night-ink2` | `#9B99B8` | secondary text | 7.1 |
| `brand` | `#8178FF` | the accent on night: editor selection, focus | 5.7 |
| `good` | `#4FD38E` | | 10.3 |
| `warn` | `#F2B84B` | | 10.9 |
| `bad` | `#F4635F` | | 6.3 |

The controller itself is white at varying alpha over night. It carries no
colour at all in play; the periwinkle appears only in the editor, on the
control being edited.

## Type

**Bricolage Grotesque** (Ateliertriay, OFL), one family for every surface.
It has an optical-size axis, so the same face is a text face at 12 pt and a
display face at 96 pt, which is what keeps a 15 px settings label and a 64 px
headline reading as one voice.

| Role | Instance | Weight | Tracking |
|---|---|---|---|
| Display (the headline, "No controller? Use your phone.") | 96pt | ExtraBold 800 | -0.03em |
| Title (screen titles, section heads) | 96pt | SemiBold 600 | -0.02em |
| UI, body | 12pt | Regular 400 / Medium 500 | 0 |
| Emphasis, buttons | 12pt | SemiBold 600 | 0 |
| Figures (a pairing code, a latency) | 12pt, `tnum` | Medium 500 | 0.02em |

No monospace anywhere. A latency reading is a number, and Bricolage has
tabular figures.

## Shape

One radius scale, from the tile the mark sits on: **large 24, medium 14,
small 8, control 999 (pill)**. Buttons are pills. Tiles and panels are 14.
Text fields are 8. Nothing is sharp-cornered and nothing is a 16.

The controller has its own geometry (circles, pills and rounded rectangles at
34% of the short side) and does not take these values; it is a physical object,
not a piece of interface. Its d-pad is two crosses stacked, the cardinal one
over a diagonal one, which is the construction of the mark; a diagonal press
lights the diagonal arm.

## Depth

Paper surfaces are separated by hairline or by tone, never both, and never by
shadow except for one case: a floating element (a dialog, the pairing sheet)
gets `0 12px 32px -12px rgba(21,20,43,.28)`, an offset and a soft blur, tinted
from the ink. No glow. No halo. Nothing has a `border-left`.

## Motion

Feedback, not personality. Press feedback on anything pressable: scale 0.97
over 120 ms. Screen transitions and sheets: 180-220 ms, `cubic-bezier(.23,1,.32,1)`.
One authored moment per surface: on the website the light behind the phone
moves, very slowly. Nothing loops on an operating surface. Under `prefers-reduced-motion` the light holds still.

## Copy

The product's own voice, already established: plain, short, a little dry.
"No controller? Use your phone." stays. Buttons name the action ("Find my PC",
"Pair", "Play"). Errors say what is wrong and what to do. No exclamation
marks, no "seamless", no "unleash".

## What is not in the system

No gradient text. No glassmorphism. No cards inside cards. No colour-coded
dots on every row. No neon, no glow, no dark-mode-with-one-accent template.
No illustration that is not the controller. The controller is the product;
draw it, and let it be the picture.
