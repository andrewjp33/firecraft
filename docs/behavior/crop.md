# Cropping (Crop tool, Crop Document, Crop Selected Bitmap)

Measured in Fireworks MX under Wine on 2026-11-16 (scratch prefix, private Xvfb, driven with
`tools/fwmx-oracle/xin.py`; results dumped with a `.jsf` passed to the running instance).
Probe: `crates/engine/examples/crop_probe.rs`, a 300 x 200 white canvas with a red rectangle
10,10 80x60, a blue ellipse 255,5 40x40, a green rectangle 100,50 40x30, a 120x90 bitmap at
150,90 (varied colours, soft alpha) and a 30x20 bitmap at 5,175.

## The crop box (Crop tool)

| Action | Fireworks MX |
|---|---|
| Drag on the canvas | A box from where the drag began to the pointer. The start isn't limited, but the dragged corner stays on the canvas (dragging 60 px past the bottom right stopped at the canvas corner; a drag from -11,-18 towards -30,-30 gave -11,-18 to 0,0) |
| Drag one of the 8 handles | Resizes the box, past the canvas too (+10, +30, +120 px all kept); dragged past the opposite side the box turns over (top-left handle 20,20 dragged to 190,100 over a box to 170,90 gave 170,90 20x10) |
| Drag a side of the box (between handles) | Moves that side, as its middle handle does (reported by the user from Fireworks MX, 2026-11-16) |
| Drag inside the box | Moves it |
| Drag outside the box | Draws a new one |
| Double-click inside, or Enter | Crops ("Crop Document"); the Crop tool stays chosen |
| Escape | Removes the box and chooses the Pointer tool (also when the Crop tool was chosen from the Pencil) |
| Choose another tool | Removes the box; nothing is cropped. The Hand and Zoom tools (and holding Space) keep it and show it, so you can zoom or scroll and carry on (reported by the user from Fireworks MX, 2026-11-16) |
| Choose the Crop tool with a marquee | The marquee becomes a crop box of the same size (reported by the user from Fireworks MX, 2026-11-16) |
| Arrow keys | Move the crop box (Shift: 10 px); with a marquee tool they move the marquee (reported by the user from Fireworks MX, 2026-11-16) |
| Look | Marching ants with 8 small black square handles; the outside is not dimmed |
| Property inspector | The Document inspector plus W, H, X, Y of the box |

Preferences in `Fireworks MX Preferences.txt`: `CropDocCropsImages` (true), `CropToolStickiness`
(20), `CropTool_Constraint` (1), `CropTool_Horiz`/`CropTool_Vert` (100). Stickiness didn't show
in any of the drags above; the others have no control in MX's UI that we found.

One drag didn't fit: a top-left handle at 20,20 of a box to 202,122 dragged to 215,135 gave
202,122 to 210,130 (clamped to the canvas corner), while other handle drags past the canvas were
never clamped. Not reproduced since; FireCraft doesn't clamp handle drags.

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
