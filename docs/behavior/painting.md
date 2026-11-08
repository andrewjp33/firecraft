# Painting tools

Brush, Pencil, Eraser and Paint Bucket (`crates/engine/src/paint.rs`), measured against Fireworks
MX by performing the gestures in the real program (under Wine, `tools/fwmx-oracle/xin.py`) and
replaying them with `crates/engine/examples/paint_probe.rs`.

## Which bitmap a stroke paints on

| Situation when the mouse is pressed | Result in Fireworks MX |
|---|---|
| A bitmap is selected | The stroke goes into it, wherever it starts |
| Nothing selected, press on a bitmap's pixels | The stroke goes into that bitmap |
| Nothing selected, press on empty canvas | A new bitmap on the current layer, left selected |

So a second stroke elsewhere continues the bitmap the first one made. The Eraser works on the
selected bitmap or the one under the press and never makes one. A pixel selection confines all
of them. Fireworks shows no selection box while a painting tool is active.

Fireworks trims a bitmap to its opaque area. FireCraft trims a new bitmap when the stroke ends;
an existing one keeps at least its old rectangle.

## Strokes

* A stroke runs through the **middle of the pixels under the mouse** (`floor + 0.5`), for the
  Brush and for the Pencil, anti-aliased or not.
* Same brush engine as path strokes ([strokes.md](strokes.md)).
* The Pencil is the "1-Pixel" preset, or "1-Pixel Anti-Aliased" with Anti-Aliased on (off by
  default). Auto Erase draws in the fill colour when pressed on a pixel of the stroke colour.

## Eraser

Defaults from the preferences file: size 13, round, Edge 2 (`EraserToolSoftEdge` is in tenths: 20),
opacity 100%.

* **The tip is Fireworks' own brush tip** (`tip_value`, [strokes.md](strokes.md)): round with the
  bell curve, square with a linear ramp; the Edge is its softness. Single dabs of sizes 7 to 60 at
  Edge 2 to 100 match Fireworks MX to within one level.
* A round tip has `size + 1` cells per side (the extra one on the far side), a square one `size`.
  Odd sizes centre on the mouse's pixel; even round ones on its left/top edge, even square ones
  on its right/bottom edge.
* One dab per pixel along the stroke; each removes its cover from what's left; the opacity
  applies once to the whole stroke, so overlapping dabs never exceed it.

## Paint Bucket

Defaults: tolerance 32, Anti-alias edge. It selects like the Magic Wand in its Classic style
([magic-wand.md](magic-wand.md)) and paints the fill colour through that mask. Clicking a path
fills the path. Nothing outside the bitmap's opaque area is filled.

**Alpha counts as a colour channel in the distance** (the Magic Wand's too), measured on an
eraser's soft edge with a green seed at tolerance 32:

| Pixel | d | Mask = 255 + tolerance - d | Result |
|---|---|---|---|
| alpha 223 | 32 | inside the region | filled completely |
| alpha 49 (next to the region) | 206 | 81 | alpha 114 |
| alpha 0 (not next to the region) | 255 | not in the 1-px ring | stays transparent |
| black line | 180 | 107 | 42% fill colour |

The same rule held for the wand at tolerance 102 (alpha 114 at distance 156: mask 201).

## Live strokes

Each update stamps only the new dabs into the stroke's kept ink (`LiveInk`; the Eraser keeps what
is left of each pixel), so the time per update doesn't depend on the stroke's length. A test
compares this with drawing the whole stroke at once, for every preset. `examples/paint_bench` and
`examples/paint_long` measure it.

## Results

Same gestures, flattened result compared with Fireworks MX's:

| Tool | Result |
|---|---|
| Pencil (3 strokes), Brush (1-Pixel Anti-Aliased, diagonal) | identical |
| Eraser, size 13 | identical along the stroke; 93 pixels differ at the rounded ends, at most 34 levels |
| Paint Bucket around that erased shape | 127 more pixels differ, on its soft outline, about 2 levels |
