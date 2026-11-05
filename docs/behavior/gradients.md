# Gradient fills (Fireworks MX)

Read from Fireworks.exe's gradient position function (`0x61ea90`, a switch on the fill type at
`0x61eb37`) and checked against Fireworks renders of every type with a black-to-white ramp:
FireCraft matches all 11 within 1 level on every pixel. Implemented in
`crates/raster/src/gradient.rs`.

## Types

Fireworks MX has 11 gradient fills (internal type id, category, `shape` in JavaScript):

| id | category | shape |
|---|---|---|
| 2 | `fc_Linear` | linear |
| 3 | `fc_Circular` | radial |
| 4 | `fc_Conical` | conical |
| 5 | `fc_Satin` | satin |
| 7 | `fc_Pinch` | pinch |
| 8 | `fc_Folds` | folds |
| 9 | `fc_Elliptical` | elliptical |
| 10 | `fc_Rectangular` | rectangular |
| 12 | `fc_Bars` | bars |
| 13 | `fc_Ripple` | ripple |
| 14 | `fc_Waves` | waves |

Starburst and Contour are not in MX (later versions); FireCraft keeps an approximate Starburst
for files from newer versions.

## Ramp position

Handles: start `PSX/PSY`, end `PEX/PEY`, side `PFX/PFY`. `p` = point - start,
`d` = end - start, (u, v) = the point in the handle frame (p = u (end - start) + v (side - start);
Fireworks maps the frame to 100 units per handle), `a = p.y / d.y`, `b = p.x / d.x`
(9999999.9 when the divisor is 0).

| Type | t |
|---|---|
| Linear, Bars | p.d / \|d\|^2 |
| Radial | \|p\| / \|d\| |
| Conical | angle between p and d / pi (law of cosines + acos) |
| Elliptical, Ripple | sqrt(u^2 + v^2) |
| Rectangular | max(\|u\|, \|v\|) |
| Pinch | \|u v\| |
| Satin | \|sin(a) (b - a) - cos(3b)\| / 2 |
| Waves | \|sin(a) (b - a)^2 - cos(3b) (b - a)\| / 2 |
| Folds | \|cos(3b sin(b + sin 0.25) cos(a + b)) sin(24ab) (a + b sin a) / 2\| |

Bars, Ripple, Satin, Pinch and Folds then repeat mirrored past 1 (t = \|t\|; fractional part,
flipped when the integer part is odd). Every type is finally clamped to 0..1.

Sampling: Fireworks evaluates the gradient at each pixel's integer coordinate (its top-left
corner), not its centre; Folds, Pinch and Waves are sensitive enough to show it.

## Fill textures

A fill's texture (`TXFv`, the same mask format as a stroke's, see
[strokes.md](strokes.md#textures)) is tiled from the path's texture origin (`TOXf`/`TOYf`) over
solid and gradient fills. Its amount is the fill preset's `FTBi` (per mille), weight
`w = FTBi * 255 / 1000`; `FSMi` picks how it works (FirePixel's `paint.rs`, Apache-2.0):

* **Opaque** (`FSMi` 2, the default): the colour goes toward white,
  `f = round((255 - T) w / 255)`, `c' = c + round((255 - c) f / 255)`.
* **Transparent** (`FSMi` 1, the Property inspector's "Transparent"): coverage goes,
  `s = T + (255 - w) - round(T (255 - w) / 255)`, `a' = round(s a / 255)`.

Checked with tools/fwmx-oracle (`crates/io/examples/fill_texture_probe.rs`): both modes, Grain
and a made-up texture, at 50, 70 and 100%, with a shifted origin, match Fireworks MX pixel for
pixel (transparent ones once Normal blending rounds, see
[blend-modes.md](blend-modes.md)). A fill colour with alpha below 255 showed fully opaque in MX
in that probe: not modelled yet. Parity: spectrum_wave.png 33.6% -> 95.9%, mechscene2.png's
floor (rectangular gradient + "Line-Horiz 1" at 50%) 68.2% -> 79.1%. The Property inspector's
fill section has Texture (the same menu as strokes), its amount and Transparent; new fills get
the default texture at 0%, as Fireworks keeps one on every fill.
