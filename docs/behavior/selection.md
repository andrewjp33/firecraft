# Pixel selections (marquee, Magic Wand)

Measured in Fireworks MX under Wine on 2026-11-15. Document: 200 x 150 canvas, a red bitmap at
10,10 100x80 on Layer 1 and a blue one at 80,50 100x80 on Layer 2. The marquee ran from 60,30 to
140,110. MX's Layers and History panels only appear when a window manager runs on the test
display (see `tools/fwmx-oracle`). Step names were read from the Edit menu's Undo item.

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
