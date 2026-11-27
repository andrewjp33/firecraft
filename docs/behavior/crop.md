# Cropping (Crop tool, Crop Document, Crop Selected Bitmap)

Measured in Fireworks MX under Wine on 2026-11-16 (scratch prefix, private Xvfb, driven with
`tools/fwmx-oracle/xin.py`; results dumped with a `.jsf` passed to the running instance).
Probe: `crates/engine/examples/crop_probe.rs`, a 300 x 200 white canvas with a red rectangle
10,10 80x60, a blue ellipse 255,5 40x40, a green rectangle 100,50 40x30, a 120x90 bitmap at
150,90 (varied colours, soft alpha) and a 30x20 bitmap at 5,175.

## The crop box (Crop tool)

| Action | Fireworks MX |
|---|---|
| Drag on the canvas | A box from where the drag began to the pointer, kept on the canvas: the dragged corner stays on it, and so does the start, per axis, unless both ends lie past the same side (see below) |
| Drag one of the 8 handles | Resizes the box, past the canvas too (+10, +30, +120 px all kept); dragged past the opposite side the box turns over (top-left handle 20,20 dragged to 190,100 over a box to 170,90 gave 170,90 20x10). A side let go up to 5 screen pixels past the canvas snaps to its edge (see below) |
| Drag a side of the box (between handles) | Moves that side, as its middle handle does (reported by the user from Fireworks MX, 2026-11-16) |
| Drag inside the box | Moves it; it snaps the same way |
| Ctrl while resizing or moving | No snapping |
| Drag outside the box | Draws a new one |
| Double-click inside, or Enter | Crops ("Crop Document"); the Crop tool stays chosen |
| Escape | Removes the box and chooses the Pointer tool (also when the Crop tool was chosen from the Pencil) |
| Choose another tool | Removes the box; nothing is cropped. The Hand and Zoom tools (and holding Space) keep it and show it, so you can zoom or scroll and carry on (reported by the user from Fireworks MX, 2026-11-16) |
| Choose the Crop tool with a marquee | The marquee becomes a crop box of the same size (reported by the user from Fireworks MX, 2026-11-16) |
| Arrow keys | Move the crop box (Shift: 10 px); with a marquee tool they move the marquee (reported by the user from Fireworks MX, 2026-11-16) |
| Look | Marching ants with 8 small black square handles; the outside is not dimmed. The ants run on the box's outermost screen pixels (left and top on the first pixel inside, right and bottom on the last), so a box the size of the canvas runs along the canvas's own edge pixels; each 5 x 5 handle covers 3 screen pixels before the box's edge and 2 after, at 100% and 200% alike (measured 2026-11-26) |
| Cursor | A crop cursor: two interlocking corner brackets, the hotspot on a small diagonal tick inside the frame they make. Inside the box a four-way arrow with thin (1 px) shafts and small solid heads. FireCraft draws lookalikes of both from shapes (`cursors.rs`; no Fireworks artwork), scaled to the system cursor size; over a handle it shows its double arrows. Both stay while dragging |
| Property inspector | The Document inspector plus W, H, X, Y of the box |

Preferences in `Fireworks MX Preferences.txt`: `CropDocCropsImages` (true), `CropToolStickiness`
(20), `CropTool_Constraint` (1), `CropTool_Horiz`/`CropTool_Vert` (100). None of them has a
control in MX's UI that we found.

### Drawing a new box (measured 2026-11-26)

Drags on the 300 x 200 probe at 100% (the user had reported the first one from Fireworks MX:
the whole box stays on the canvas):

| Drag | Box |
|---|---|
| -20,-20 to 320,220 | 0,0 300x200 |
| -20,-20 to 100,80 | 0,0 100x80 |
| 320,220 to 100,100 | 100,100 200x100 |
| -20,60 to 100,120 | 0,60 100x60 |
| -20,-20 to -40,-40 | -20,-20 20x20 |
| -20,-20 to 100,-40 | 0,-20 100x20 |
| -20,60 to -40,120 | -20,60 20x60 |
| 320,60 to 370,120 | 300,60 20x60 |

So, along each axis, the end is clamped to the canvas and so is the start, unless both clamp to
the same edge: the start then stays where it was, so the box keeps some size (this is the
-11,-18 to -30,-30 → -11,-18 to 0,0 of the 2026-11-16 session).

### Snapping to the canvas's edges (measured 2026-11-26)

A box from 50,50 to 250,150; its right handle dragged to the canvas's right edge (300) plus d:

| d (100%) | -25 … -1 | +1 … +5 | +6, +7, +9 … +40 |
|---|---|---|---|
| right side | 300 + d | 300 (snapped) | 300 + d |

Fast jumps and 1 px steps gave the same, and so did coming back in from outside (730 → 695):
only where the side is let go counts. The left handle (-3, -5 → 0; -6 kept; +3 inside kept), the
top side (-3 → 0) and a corner (303,203 → 300,200) behave alike. Moving the whole box snaps
too: moved so its right side would be at 305 it went to 300, at 306 it stayed, at 299 it
stayed; its left side at -3 went to 0. With Ctrl held neither a handle (303) nor a move (103)
snapped. At 200% a side 2 canvas pixels past snapped, 3 didn't: the distance is 5 **screen**
pixels. Inside the canvas nothing snaps. (`CropToolStickiness` 20 in the preferences didn't
match any distance seen.)

This explains a drag that didn't fit on 2026-11-16: a top-left handle of a box to 202,122 on
the then 210 x 130 canvas, dragged to 215,135, gave 202,122 to 210,130: both coordinates were
5 px past the canvas and snapped.

## What cropping does

Cropping the probe to 40,30 to 250,160 gave a 210 x 130 canvas with:

* the red and green rectangles kept whole (red crosses the left and top edges);
* the 120x90 bitmap trimmed to 100x70 (the part inside the box), its pixels unmoved (a bitmap
  whose part inside is wholly transparent is deleted: seen with Canvas Size, which crops the
  same way, [canvas-size.md](canvas-size.md));
* the blue ellipse (255..295 across) and the small bitmap, wholly outside, deleted.

Cropping to 40,30 to 330,230 (past the right and bottom) gave 290 x 200: only the small bitmap
went, the new area shows the canvas colour.

In the saved file every object keeps its old **document coordinates** (the scripting API still
reports the red rectangle at 10,10); the root `XLCf`/`YLCf` becomes the box's top-left (40, 30)
and `WIDf`/`HITf` its size. FireCraft keeps objects in canvas coordinates, so it moves them by
-40,-30 and adds 40,30 to the saved origin: the file it writes has the same object coordinates
as Fireworks' own crop, and Fireworks MX's re-render of it is identical to its own crop
(checked 2026-11-16).

## Edit menu

* **Crop Document**: disabled with nothing selected. With a bitmap selected it crops at once to
  the selection's bounds (no box): 210x130 with the 100x70 bitmap selected gave 100 x 70, the
  other objects deleted. Undo step "Crop Document". FireCraft uses the selected pixels' bounds
  when there are any (the right-click menu offers it while pixels are selected), otherwise the
  selected objects' bounds rounded out to whole pixels.
* **Crop Selected Bitmap**: enabled with a bitmap selected. Puts a crop box around the bitmap;
  Enter or a double-click trims the bitmap to the part inside the box (undo step "Crop
  Image"); a box reaching past the bitmap doesn't grow it (box 130,75 100x70 over a bitmap at
  110,60 100x70 left 130,75 80x55). Afterwards the Pointer tool is chosen and the bitmap stays
  selected.

## Not measured

* Hidden or locked layers and other frames: FireCraft crops every layer and frame alike.
* Effects reaching into the box from an object outside it (FireCraft goes by the object's
  bounds without effects, so such an object is deleted).
* Slices and hotspots outside the box (FireCraft deletes them like any object).
* Crop Document with a marquee: drawing a marquee failed in that session.
