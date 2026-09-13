# PhonePad layout format

Version 1. Licensed Apache-2.0, like the rest of `protocol/`.

A `.phonepad` file is one controller layout: where the controls sit, how big
they are, and how they respond. It is what you hand someone when you have got a
game feeling right and they want the same.

## The rule that matters

**A layout is data. It is never code.**

There is no scripting, no expression language, no macro, no reference to
anything outside the file. Importing a layout can change what your controller
looks like and how it responds, and nothing else. If you find a way to make an
imported layout do anything beyond that, it is a security bug — see
[SECURITY.md](../SECURITY.md).

This constrains what the format will ever grow into, deliberately. Community
content is only worth having if installing it is safe without reading it first.

## Shape

```json
{
  "format": "phonepad.layout",
  "formatVersion": 1,
  "id": "forza-wheel",
  "name": "Forza — wheel feel",
  "author": "someone",
  "notes": "Large left stick, heavy expo. Brake on the left trigger.",
  "landscape": [ /* controls */ ],
  "portrait":  [ /* controls */ ]
}
```

`id`, `author` and `notes` are optional. `landscape` and `portrait` are separate
arrangements of the same layout; the app switches between them when the phone
rotates.

A control:

```json
{
  "id": "lstick",
  "type": "stick",
  "mapping": { "stick": "left" },
  "x": 0.175, "y": 0.68,
  "size": 0.34, "aspect": 1.0,
  "rotation": 0.0, "opacity": 0.7,
  "visible": true, "label": "", "shape": "circle",
  "deadzone": 0.02, "antiDeadzone": 0.15, "saturation": 0.92,
  "sensitivity": 1.0, "responseCurve": 1.3,
  "circularRange": true, "floating": true, "drift": true,
  "analogSlide": false
}
```

### Coordinates

`x` and `y` are the control's **centre**, as a fraction of the safe area.
`size` is a fraction of the **shorter** screen edge, and `aspect` is width over
height.

Splitting them this way is what makes one layout correct across devices: a
control keeps its physical size while the spread between controls follows the
screen. Nothing here is in pixels, so nothing here is tied to one phone.

### Fields

| Field | Type | Range | |
|---|---|---|---|
| `type` | enum | `button` `dpad` `stick` `trigger` | |
| `mapping.buttons` | int | XInput `wButtons` bitmask | for `button` |
| `mapping.stick` | enum | `left` `right` | for `stick` |
| `mapping.trigger` | enum | `left` `right` | for `trigger` |
| `x`, `y` | float | 0.0–1.0 | |
| `size` | float | 0.04–0.6 | |
| `aspect` | float | 0.4–3.5 | |
| `rotation` | float | −0.8–0.8 rad | |
| `opacity` | float | 0.05–1.0 | |
| `shape` | enum | `circle` `roundedRect` `pill` | |
| `deadzone` | float | 0.0–0.6 | movement ignored as noise |
| `antiDeadzone` | float | 0.0–0.5 | cancels the *game's* deadzone |
| `saturation` | float | 0.3–1.0 | where full deflection is reached |
| `sensitivity` | float | 0.3–2.5 | |
| `responseCurve` | float | 0.5–2.5 | above 1.0 is finer near centre |
| `circularRange` | bool | | circular vs square gate |
| `floating` | bool | | origin follows the touch |
| `drift` | bool | | origin follows past the ring |
| `analogSlide` | bool | | analogue trigger rather than full press |

## Importing

The importer is the security boundary, and it is written to assume the file is
hostile:

- **Unknown fields are dropped.** A file from a later version imports as far as
  this version understands it, rather than failing.
- **Every numeric is clamped** to the range above. Out-of-range values do not
  reject the file; they are pulled into range, so a layout that would put a
  control off-screen or set an impossible curve simply cannot.
- **Labels are capped** at 8 characters with control characters stripped.
- **The control count is capped** at 64 per orientation.
- **A wrong `format` or a `formatVersion` above 1 is rejected outright.**
- Nothing in the file can name a file path, URL, or anything outside itself.

## Compatibility

`formatVersion` increments only for a change a version-1 reader could not
handle safely. Adding a field does not qualify — old readers drop what they do
not know, which is why unknown fields are dropped rather than rejected.

Exported files are deterministic: the same layout produces byte-identical
output, so two files can be diffed.
