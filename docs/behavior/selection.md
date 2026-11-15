# Pixel selections (marquee, Magic Wand)

Measured in Fireworks MX under Wine on 2026-11-15. Document: 200 x 150 canvas, a red bitmap at
10,10 100x80 on Layer 1 and a blue one at 80,50 100x80 on Layer 2. The marquee ran from 60,30 to
140,110. MX's docked panels (Layers, History, Property inspector) sit right of and below its
default 1024 x 740 window; on a bare Xvfb they only show once the main window is made bigger
(`xin.py move 500 300 resize 1280 1000`). Step names were read from the Edit menu's Undo item.

| Action (marquee active) | Fireworks MX | Undo step |
|---|---|---|
| Draw a marquee on red | Red is edited (pencil in its Layers row), Layer 1 is current | Select Pixels |
| Click blue's row in the Layers panel | The marquee stays where it is and now applies to blue; the pencil moves to blue's row | Edit Bitmap |
| Edit > Clear | Only blue's pixels inside the marquee go; the marquee stays | Delete |
| Click red's row | Same marquee, now on red | Edit Bitmap |
| Undo "Edit Bitmap" | Back to editing blue, marquee unchanged | |
| Undo "Delete" | Blue's pixels come back, marquee unchanged | |
| Click a layer's name row | The marquee ends | Layer Change |
| Select > Deselect | The marquee ends | Deselect |
| Select > Select All | | Select All |
| Choose the Pointer tool, or press Escape | Bitmap editing ends and the marquee with it | Exit Bitmap Mode |

So a selection belongs to the canvas, not to a bitmap: the same marquee can clear or fill one
bitmap after another.

## Marquee shape, dragging and clicking

Measured in Fireworks MX on 2026-11-15 (empty 200 x 150 document, Marquee with Anti-Alias, the
default):

* **Rectangles are whole pixels with hard edges.** A drag from the pixel at canvas 20,20 to the one
  at 60,50 selects pixels 20 to 59 by 20 to 49; filled with the Paint Bucket, the saved file has
  only alpha 0 and 255. A second marquee overlapping the first, filled the same way, leaves no
  seam. Anti-Alias only smooths an oval's outline. (FireCraft used to put the edges where the
  pointer was, on half pixels, and soften them; a second fill then flooded through that soft
  edge and left a lighter seam.)
* **Dragging inside the selection moves it**, with the Marquee tool or the Magic Wand: the
  outline moves, the pixels stay ("Move").
* **A click inside the selection ends it**, with either tool, recorded as "Select Pixels". So does
  a Marquee click outside it. A Magic Wand click outside selects there ("Magic Wand Tool").
* One run disagreed: right after a Paint Bucket fill, a drag from inside the marquee drew a new
  marquee instead of moving it. Every other run (with and without a fill first, with and
  without hovering first) moved it; FireCraft always moves it.

FireCraft: `marquee` snaps the rectangle to whole pixels (`floor` of both corners) and
anti-aliases only ovals; `pixels_contain` and `move_pixel_selection` serve the drag, with a move
cursor over the selection for the Marquee, Oval Marquee, Lasso and Magic Wand tools (Shift or
Alt pressed: they draw a new selection to combine instead). The marching ants show with every
tool, the Brush and Pencil included (they paint inside the selection). Edit > Insert > Empty
Bitmap and the Layers panel's New Bitmap Image keep the selection, which then applies to the new
bitmap.

## Marquees on empty canvas

| Situation | Fireworks MX |
|---|---|
| Nothing selected; marquee 120,10 to 190,45 on empty canvas | A marquee like any other ("Select Pixels") |
| Then the Paint Bucket inside it | A new bitmap on top of the current layer (Layer 2), filled inside the marquee, being edited (pencil); 0,0 200x150 while edited; the marquee stays ("Fill") |
| Red being edited; marquee 10,100 to 60,140, away from red's pixels | The marquee applies to red, which becomes 0,0 200x150 while edited |
| Then the Paint Bucket inside it | The fill goes into red; no new bitmap |

FireCraft: a pixel selection can have no bitmap (`target: None`). The Paint Bucket, the Gradient
tool and Brush/Pencil strokes then make a new bitmap on top of the current layer, confined to the
selection, and that bitmap becomes the one the selection applies to. For a bitmap being edited,
fills and strokes inside the selection may reach past its pixels: FireCraft grows it to cover the
selection and trims it afterwards to its old rectangle plus what was painted (Fireworks keeps it
canvas-sized until bitmap editing ends). Undoing the fill removes the new bitmap and leaves the
marquee without one.

## FireCraft

* `PixelSelection` holds the mask with its canvas position (`x`, `y`) and the bitmap it applies
  to (`target`). Commands crop it to their bitmap (`PixelSelection::on`). A marquee's mask
  covers the marquee itself, so the part beyond the bitmap it was drawn on counts for any other
  bitmap picked later.
* Each undo step keeps the selection from before the step (`history::Selected`, with masks
  shared between steps while they don't change). Undo brings back the pixel selection and the
  bitmap it applied to. Without a pixel selection, undo keeps the objects selected now, as before.
* Steps: Select Pixels (marquee, Magic Wand), Edit Bitmap (Layers panel), Layer Change, Deselect
  (Select > Deselect or a click with a marquee tool), Select All, Select Inverse, Feather.
  Picking the bitmap that's already being edited records nothing.
* The Layers panel shows a pencil in the lock column of the bitmap being edited.
* Choosing the Pointer tool (from the Tools panel, V or the menu) or pressing Escape ends bitmap
  editing (`DocState::exit_bitmap_mode`): the marquee goes as one undo step, "Exit Bitmap Mode",
  and the bitmap that was edited stays selected with its handles. An empty bitmap (Insert > Empty
  Bitmap) nothing was put into goes away, with no undo step, as measured in Fireworks MX. Escape
  does this only when nothing else is using it: no dialog, pop-up or text field, no text being
  edited, no stroke or live edit in progress, and not with the Pen tool.
* While pixels are selected the canvas shows only the marquee, no object box or handles (as in
  Fireworks MX).
* Unmeasured: what MX does when a vector object's row is clicked with a marquee active (FireCraft
  ends the marquee and selects the object, with no undo step), and whether tools other than the
  Pointer also end bitmap editing.
