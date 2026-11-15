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
* Not yet: ending the marquee when the Pointer tool is chosen or Escape is pressed (Exit Bitmap
  Mode). FireCraft keeps the marquee until something else ends it. Also unmeasured: what MX does
  when a vector object's row is clicked with a marquee active (FireCraft ends the marquee and
  selects the object, with no undo step, as before).
