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

## Textures

A stroke's texture (`TXBv { INMs MSKv }` in the path's attributes) is an 8-bit mask, tiled from
the path's texture origin (`TOXf`/`TOYf`, which moves with the path). The brush's texture amount
(`BTBi`, the Property inspector's "Texture" percentage) and edge texture (`BTEi`) say how much
it removes ink; FirePixel's engine applies it. Verified with tools/fwmx-oracle
(`crates/io/examples/texture_probe.rs`): simple and natural brushes at several amounts, edge
texture, and the texture origin match Fireworks MX pixel for pixel, with Fireworks' own
textures (Grain, Burlap, Hatch 1, Line-Diag 2, Waffle) and with a made-up one.

* **Fireworks draws from the mask in the file**, not from its texture folder: a file whose
  "Grain" mask is stripes draws stripes in MX. So a FireCraft file shows the same textures in
  Fireworks whatever their source.
* **From an image, the mask is `255 - grey`** (dark pixels take ink away): the masks MX embeds
  for its Grain, Burlap (PNG), Hatch 1, Line-Diag 2 and Waffle (GIF) files equal FireCraft's
  byte for byte. Transparent pixels count as white (not checked against Fireworks; its textures
  are opaque).
* Mask tiles are 256 x 256 bytes (`TSZi{100}`) with tile flags 1; paths using the same texture
  share its tiles, as in Fireworks.
* Fireworks keeps a texture on every path (default "Grain") even at 0%; FireCraft keeps the one
  read, and new strokes get the tool's texture.

**Where textures come from**: an installed Fireworks' `Configuration/Textures` folder (Edit >
Preferences > Fireworks folder, or found in the usual install locations, also under Wine; MX's
49 textures all decode like ImageMagick reads them), FireCraft's own built-in textures (FireCraft
Noise, Weave, Diagonal; drawn in code, used when Fireworks isn't found), the textures in the
open document, and any PNG or GIF (Texture > Other...). Nothing of Fireworks is bundled.

## Not yet

* The 3D Glow brush (natural, alpha remap) comes out up to ~10-19 levels too bright in parts
  of its glow compared with Fireworks MX (lines2.png, 92.6% match). FirePixel's own renderer
  gives the same pixels, so it's in the engine as reconstructed (possibly an MX/CS difference),
  not in FireCraft's port.

* Random brushes on paths FireCraft creates or edits (Oil, Random, some Watercolor, scatter)
  can scatter differently in Fireworks MX: MX picks a random seed per point when it first builds
  a path's polyline (`PCLv`), and FireCraft doesn't write one yet. Paths from Fireworks keep
  theirs and match (texture probe: Watercolor Heavy exact once MX's polyline is present).
* Pen pressure stored on bezier points (`BZCi`/`BAZv`) is not kept when points are edited.
