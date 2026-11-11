# Gradient fills (Fireworks MX)

Fireworks MX's gradient position function (`0x61ea90`, a switch on the fill type at
`0x61eb37`), reproduced and checked against Fireworks renders of every type with a black-to-white ramp:
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

## Making and editing gradients

Measured in Fireworks MX by choosing each fill in its Fill Options panel and reading the fill back
through the scripting API (`fillHandle1`..`3`, `fill.gradient.nodes`).

The Fill Options menu: None, Solid, Web Dither, Pattern, then Linear, Radial, Ellipse, Rectangle,
Cone, Starburst (`fc_Pinch`), Bars, Ripples, Waves, Satin, Folds.

**A first gradient runs from the stroke colour to the fill colour.** Changing to another gradient
keeps the stops and resets the handles. Default handles for an object with bounds x, y, w, h
(c = its centre):

| Fill | Round handle | Square handle | Third handle |
|---|---|---|---|
| Linear | left middle | right middle | round + (0, w) |
| Radial, Ellipse, Rectangle, Cone, Ripples | c | right middle | top middle |
| Starburst | c | c + (w/4, h/4) | c + (w/4, -h/4) |
| Bars | c | right middle | c + (0, w/2) |
| Waves | c | c + (w/4, h/4) | top middle |
| Satin, Folds | c | bottom right | top middle |

On the canvas: the round handle moves the whole fill; a square handle moves only itself; dragging a
connecting line rotates the fill about the round handle. The third handle only exists where the
type uses it (Ellipse, Rectangle, Ripples, Starburst); the others are shaped by two.

In the file (`FPLv`): `CATs` category, `FSHi` the type number above, `FETi` anti-aliased, `FEFi` the
feather radius (Fireworks has no separate "feather" flag: radius > 0), and `FGLv` the ramp: `FGVv`
and `FGYv.FG0v` the colour stops, `FGYv.FG1v` the opacity stops in each stop's alpha byte.

Feather is Fireworks' triangle filter (FirePixel's reconstruction); a newly feathered edge is 10
pixels. The Gradient tool's drag and its bitmap fill follow the manual and weren't measured.

## Pattern and Web Dither fills

Measured in Fireworks MX by reading fills back through its scripting API; FireCraft renders Fireworks
MX's own saved Pattern, Web Dither and Transparent files with no differing pixels.

**Tools panel.** The Stroke and Fill boxes show the selected object's stroke and fill. They keep them after
it is deselected, and new objects take them on whole: fill kind, pattern, stroke colour and size, and having
no stroke at all (measured). A solid fill opens the colour picker, whose footer offers None, Gradient,
Pattern and Web Dither; a gradient, pattern or dither fill opens its own pop-up, the same one as the Property
inspector's, with the kind of fill as its first control.

**Preset list.** It shows a preset's name while the stops are that preset and is blank once they differ
(measured: adding a stop blanks it, though the stored gradient name `INMs` stays).

**Pattern.** `FSHi 15`, the tile stored in the file (`FPTv { INMs IMGv }`). A new or changed pattern puts
the round handle on the object's centre, with the other two one tile away (right and up), so the first
pattern on a 60,80 240 x 120 object at 128 x 128 sits at 180,140 / 308,140 / 180,12. Choosing another
pattern resets the handles the same way. The menu lists the Patterns folder by name; three names differ
from their files: Berber Rug is "Berber", Illusion 1 is "Illusion", Impressionist-Red is "Impressionist".
**Other...** adds a PNG, GIF, JPEG, BMP or TIFF file at the bottom of the list.

**Web Dither.** `FSHi 16`, `FD1i`..`FD4i` a 2 x 2 tile by rows: first, second / second, first. Each channel
uses the websafe levels (multiples of 0x33) on either side of it, or just the nearer one within a quarter of
a step: 0x34 stays 0x33, 0x80 is 0x66 and 0x99, 0xC0 is 0xCC. The first colour takes every lower level,
the second every higher one. Editing a dither colour makes the source colour their average, rounded up
(`#000033` and `#FFFFFF` give `#808099`).

**Transparent** (`FDTb`): every tile entry becomes the websafe colour nearest the source, and every other
pixel is left empty, by canvas position (x + y odd).

**Cursors over the handles** (measured): with the Pointer, a white arrow over the round and square handles
and the rotate cursor over a connecting line.
