# Strokes (brush dabs)

Measured with Fireworks MX (`tools/fwmx-oracle`): `crates/io/examples/stroke_probe.rs` writes
lines at chosen sub-pixel positions with a template file's stroke, Fireworks MX opens and
re-saves the file, and its render is read back pixel by pixel.

## How Fireworks draws a stroke

* The path's points are snapped **down to 1/16 px** (lines 1/32 px apart render identically in
  pairs).
* The brush tip is stamped ("dabs") **every 1 px** along each segment, starting at the segment's
  start: a segment of length L gets round(L) dabs, the end point itself gets none. So the first
  pixel of a 1-px line is full and the last one half (one dab instead of two).
* Overlapping dabs combine like layered paint: coverage = 1 - (1 - a)(1 - b) ... (two
  half-strength dabs give 0.75).
* **Anti-aliased** tips (the brush preset's `BIAb` 1) are placed at the dab's exact position:
  each tip pixel is spread over the 2x2 pixels under it by bilinear weights. **Non-anti-aliased**
  tips snap to whole pixels (top-left at floor(centre - D/2 + 0.5)).
* Tip of diameter D (`BDIi`): a 1-px anti-aliased tip is a single full pixel. Larger tips: a
  disc; non-anti-aliased: the pixels whose centre is inside it (exact); anti-aliased: the
  disc's coverage of each pixel (close: Fireworks' edge rows are a little softer).

Result for the "1-Pixel Anti-Aliased" pencil (the usual Fireworks stroke): a pixel at distance d
from the line gets 1 - ((1 + d) / 2)^2 for d <= 1 (191 for a line through pixel centres, 112/111
for a line on a pixel boundary). FireCraft reproduces Fireworks' render of the probe lines to a
mean 0.27 levels (91% of pixels exact; a few pixels at the ends of angled lines differ).

## Softness (approximate)

`BSEi` is the edge softness in tenths of a percent (1000 = 100%; Fireworks' "Soft" brush is
500). How Fireworks fades the tip isn't known yet: a free fit of the tip's radial profile
reproduces each brush to ~1 level, but no simple formula fits all diameters and softness levels.
FireCraft's interim model fades the tip linearly over the outer part of its radius and weakens
each dab so the ~D/2 dabs a pixel receives add up to that fade (fitted at 100% softness; mean
error over the 192 measured brush profiles 3.5 levels per pixel, 4.4 for the previous model).
Next step: find the tip/softness code in the Fireworks MX binary (the Pencil/brush preset fields
`BSEi`, `BDIi`, `BSPi`, `BASi`, `BCNi` ...) and measure what it points to.

## Not yet covered

Brush categories other than the Pencil (Air Brush, Calligraphy, Charcoal, Crayon, Felt Tip,
Oil, Watercolor, Random, Unnatural): textures, spacing (`BSPi`), pressure and the other preset
fields; 1-px lines' ends on angled lines; stroke placement (inside/centred/outside).
