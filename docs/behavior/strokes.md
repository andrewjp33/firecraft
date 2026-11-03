# Strokes (brushes)

FireCraft draws path strokes with Fireworks' brush engine as reproduced by FirePixel
(`crates/stroke`, adapted from FirePixel's `fp-vector`/`fp-geom`, Apache-2.0; see
ATTRIBUTION.md). Its module comments (in German) describe the Fireworks behaviour each piece
reproduces; FirePixel derived it from the Fireworks binary and verified it against Fireworks
renders.

## Checked against Fireworks MX

Measured with `crates/io/examples/stroke_probe.rs`: it writes lines at chosen sub-pixel positions
(and a grid of 64 brushes: diameters 1-15, softness 0/25/50/100%, anti-aliased or not) with a
template file's stroke; Fireworks MX opens and re-saves the file, and its render is compared
with FireCraft's (`parity_report`):

| Test | Match | Mean error (levels) |
|---|---|---|
| 1-px lines at 1/32-px steps, angles 0-90 degrees | 100.0% | 0.00 |
| 64-brush grid | 99.9% | 0.03 |
| lines.png / lines2.png / tablet2.png (user files) | 100.0 / 99.9 / 100.0% | 0.00 / 0.06 / 0.15 |

What the measurements showed, and the engine implements:

* Points are snapped down to 1/16 px; the tip is stamped ("dabs") along the line, 1 px apart for
  the Pencil, from the start of each segment up to (not including) its end.
* Overlapping dabs combine as 1 - (1 - a)(1 - b); a soft or partly transparent brush weakens
  each dab (FirePixel's overlap table) so that the dabs over a pixel add up to the intended
  value.
* Anti-aliased tips (`BIAb` 1) are shifted to the dab's sub-pixel position (16 steps); others
  snap to whole pixels.
* A 1-px anti-aliased tip is a single pixel: a pixel at distance d from a line gets
  1 - ((1 + d) / 2)^2 (191 for a line through pixel centres, 112/111 on a pixel boundary).
* Softness (`BSEi`, tenths of a percent; the Property inspector's Edge slider shows 1000 - BSEi)
  fades the tip linearly in the squared distance, with Fireworks' bell curve (`BSFi`).
* Inside/outside placement (`BRPi`) doubles the diameter and masks the stroke with the path's
  fill coverage.

## Inputs

* The brush comes from the path's preset (`BPLv`: diameter `BDIi`, softness `BSEi`, shape,
  aspect `BASi`, concentration `BCNi`, spacing `BSPi`, natural brush settings, dashes,
  sensitivities); new FireCraft strokes use Fireworks' "1-Pixel Anti-Aliased" pencil. FireCraft's
  stroke settings (colour, diameter, softness, anti-aliasing) override the preset and are written
  back into it on save.
* The outline is Fireworks' stored polyline (`PCLv`, with pen pressure, velocity and a random
  seed per point) while the path's points are unchanged; moves and transforms carry it along;
  after point edits FireCraft flattens the path with Fireworks' own flattening (FirePixel:
  28,095 of 28,099 stored outlines reproduced point for point).

## Not yet

* Stroke textures (`TXBv` images) aren't loaded, so textured strokes draw untextured.
* Pen pressure stored on bezier points (`BZCi`/`BAZv`) is not kept when points are edited.
