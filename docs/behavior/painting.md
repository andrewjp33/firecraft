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
| A marquee drawn on empty canvas with no bitmap being edited | A new bitmap on the current layer, only inside the marquee ([selection.md](selection.md)) |

So a second stroke elsewhere continues the bitmap the first one made. The Eraser works on the
selected bitmap or the one under the press and never makes one. A pixel selection confines all
of them. Fireworks shows no selection box while a painting tool is active.

Fireworks trims a bitmap to its opaque area when bitmap editing ends ("Exit Bitmap Mode"), after
any bitmap tool: a Brush or Eraser stroke inside a 200 x 150 bitmap whose pixels cover 10,10
170x120 left it 10,10 170x120, margins it had before included (measured 2026-11-15). FireCraft
has no bitmap mode to leave, so it trims at the end of every stroke, fill and delete; see
[selection.md](selection.md#deleting-selected-pixels) for Exit Bitmap Mode itself. Undo step names
follow Fireworks MX: "Brush Tool", "Eraser Tool", "Pencil Tool" (by analogy, not measured),
"Fill" (Paint Bucket).

## Empty bitmaps

The Layers panel's **New Bitmap Image** button (between New/Duplicate Layer and Delete; Add Mask
sits beside it in Fireworks MX) and **Edit > Insert > Empty Bitmap** add an empty bitmap to paint,
fill or draw a marquee in. Measured in Fireworks MX under Wine on a 120 x 80 canvas (2026-11-15,
menu driven with `xin.py`, document read back with a `.jsf`):

| Step | Fireworks MX |
|---|---|
| Insert > Empty Bitmap | An image at 0,0, 120x80 (the canvas), transparent, at the top of the current layer, selected; no selection box; the title shows "(Bitmap)"; **no undo step** |
| Leave it (the bitmap-mode stop button, or select something else) still empty | It is removed, again with no undo step |
| Brush stroke into it | Still 0,0 120x80 while editing it |
| Leave it after the stroke | Trimmed to the painted pixels (17,10 63x43), as the undo step "Exit Bitmap Mode" |

The scripting call `dom.addNewImage(rect, fill)` is something else: an image of that size that
stays when deselected. `addNewImage()` without a rectangle fails.

FireCraft (`DocState::insert_empty_bitmap`): the same canvas-sized, selected, unrecorded bitmap,
with no selection box. It counts as empty while its pixels are untouched (its image stamp is the
one it was made with), and is removed without an undo step when it is neither selected nor
holding the pixel selection. A stroke into it trims it at the end of the stroke, and a Paint
Bucket fill (the whole bitmap counts, or the marquee drawn in it) trims it after the fill, so
FireCraft has no separate bitmap mode to leave. A press on the canvas with the Pointer does not
drag it.

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
