# Blend modes (Fireworks MX, measured and read from the binary)

Measured with a scripted Fireworks MX document: 6 base x 6 blend colours per mode, rendered and
saved by Fireworks (`setBlendMode`). The Tint formula and the luma weights were then read from
`Fireworks.exe` with REA: the blend-function selector at `0x56d9c4` maps each mode value to a
function (Tint `0x56d6b0`, Luminosity `0x56d580`, Hue `0x56d270`, Color `0x56d480`, ...).

| Mode | `BLDi` | FireCraft formula | Max error vs Fireworks (0-255) |
|---|---|---|---|
| Normal | absent | source over; onto an opaque pixel `round((s*a + d*(255-a))/255)` | 0 (see below) |
| Multiply | 2 | b*s/255 | 0 |
| Difference | 3 | abs(b-s) | 0 |
| Screen | 5 | 255-(255-b)(255-s)/255 | 0 |
| Darken / Lighten | 9 / 10 | min / max | 0 |
| Hue | 11 | byte HSL: (h_s, l_b, s_b) | 0 |
| Saturation | 12 | integer HSV: (h_b, s_s, v_b); grey base unchanged | 0 |
| Color | 13 | integer HSV: (h_s, s_s, v_b) | 0 |
| Luminosity | 14 | integer HSV: (h_b, s_b, luma(s)) | 0 |
| Invert | 17 | 255-b | 0 |
| Erase | 25 | removes the base where the object is | 0 |
| Tint | 29 | b + (s-b)*(255-luma(b))/255, integer, /255 rounding as Fireworks | 0 |

`luma(c) = (109 R + 366 G + 37 B) >> 9`: Rec. 709 weights 0.2125/0.7154/0.0721 scaled by 512,
computed once at startup in Fireworks.

Mode value 4 has a function in the binary (`0x56cc80`) but no menu entry in MX.

All modes are bit-exact on the measured grid. Fireworks has two colour models, both ported in
`crates/raster/src/fwhsl.rs`:

- **Byte HSL** (`0x564e70` RGB->HSL, `0x565110` back): floating point, channels scaled by the
  constant 1/255 (a multiply, which matters for truncation), every component truncated to a byte;
  hue byte = degrees/360*255. Used by the Hue mode and the Hue/Saturation effect.
- **Integer HSV** (`0x565490`, `0x565590`): all integer, C division; hue in sextants of 42/43
  (0, 85, 170 for red, green, blue). Used by Saturation, Color and Luminosity.

Partial opacity is a linear mix between the base and the blended colour (measured at 25/50/75%).

## High-precision colour math (FireCraft option)

Document Property inspector > Color math (default for new documents in Preferences). "High
precision" keeps the same colour models and parameters but computes in `f64` and rounds once
(`crates/raster/src/colormath.rs`): no ~1.4 degree hue steps, no byte truncation of saturation and
lightness, float luma and Tint. Images between steps stay 8 bits per channel. Saved as `FCMi`
(see fireworks-png.md); Fireworks MX renders such files with its own math.

**Normal onto opaque pixels rounds** (2026-11-05): semi-transparent texture-fill pixels over
white match Fireworks MX only with `round((s*a + d*(255-a))/255)` (21,600 pixels); FireCraft
truncated before. It raised parity on gym.png and spectrum_wave.png by ~7 points each (pixels
within 2 levels) but lowered ball.png, sun.png and tm.png by 0.3-1.3: effect-heavy files, so
Fireworks probably truncates when compositing effect results. Open question.
