# Brand assets

Exported from the Figma file (page *Logo*: MonoChrome, MonoChrome-invert,
Color). `docs/design.md` explains how they are used.

- `mark.svg`: the mark, stroked in `currentColor`. The path data is embedded
  verbatim in the app, the companion and the site; change it here first.
- `src/`: the raw Figma exports the rest is derived from.
- `icon/`: the app icon layers. `adaptive-fg-1024.png` is the designed tile in
  the 72 dp window of a 108 dp adaptive icon; `adaptive-bg-1024.png` is its
  edge continued outward; `legacy-512.png` is the tile with rounded corners for
  launchers that do not mask.
- `fonts/`: Bricolage Grotesque (OFL), the 12pt and 96pt optical sizes.
