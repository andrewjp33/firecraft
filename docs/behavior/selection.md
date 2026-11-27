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

## Bitmap mode and the Exit Bitmap Mode button

Fireworks MX (2026-11-15): a Pointer double-click on a bitmap enters bitmap mode with the
Marquee tool chosen (undo step "Edit Bitmap"; no selection box; the document footer's round stop
button turns red). Drawing a marquee and pressing Delete removes pixels ("Delete"); the stop
button ends bitmap mode ("Exit Bitmap Mode"): the bitmap stays selected with its handles, trimmed
to what's left, and the tool stays the Marquee. A double-click with the Marquee tool doesn't
enter bitmap mode.

FireCraft: the same double-click (`DocState::enter_bitmap_mode`, kept in undo history as
`bitmap_edit`; it ends when the bitmap is no longer selected); `in_bitmap_mode` is true with a
pixel selection, a double-clicked bitmap still selected, or an untouched empty bitmap. The
footer's stop button, after Next frame as in Fireworks MX, is red then and runs
`exit_bitmap_mode`, as the Pointer tool and Escape do. Picking another bitmap in the Layers panel
while editing moves the editing there ("Edit Bitmap").

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
* **Tool cursors** elsewhere on the canvas (Fireworks MX, 2026-11-26): the Marquee and Oval
  Marquee show a thin 1 px cross, the Magic Wand a wand running down to the right from a small
  sparkle at its tip, the hotspot. FireCraft draws lookalikes from shapes.
* **Shift, Alt, Shift+Alt** add a mark below right of the marquee tools' cross, and below the
  Magic Wand's sparkle: **+** (add to the selection), **−** (subtract), **×** (intersect). Over
  the selection the marked cursor shows too, not the move cursor (a drag there draws, it
  doesn't move). Seen in Fireworks MX 2026-11-26; FireCraft's marks are its own drawings.
* **The cursor over the selection** (Marquee, Oval Marquee or Magic Wand, no Shift or Alt) is a
  white arrow over a small dashed box, the hotspot at the arrow's tip (seen in the running
  Fireworks MX 2026-11-26); the box is white inside and the whole cursor has a white outline, so
  it reads on dark pixels. FireCraft draws a lookalike from shapes (no Fireworks artwork).
* **A click inside the selection ends it**, with either tool, recorded as "Select Pixels". So does
  a Marquee click outside it. A Magic Wand click outside selects there ("Magic Wand Tool").
* **A marquee is cut to the canvas when the button is let go** (measured 2026-11-26 on a
  200 x 150 bitmap filling the canvas). While dragged from -30,40 to 60,100 the outline shows
  whole, past the canvas; let go, it becomes 0,40 60x60 (the Property inspector's marquee
  fields agree). A marquee added with Shift is cut the same way. With Feather 10 the feather
  comes first: the same drag then Delete left alpha 0 right up to the canvas's left edge
  (no fade along it), the fade only at the right, top and bottom; the bounds read 0,30 70x80.
  The Lasso is cut too (a polygon from -30,40 to 38,100 gave 0,40 38x60); FireCraft's Lasso
  doesn't select yet.
* **The marching ants are screen pixels**, whatever the zoom: 4 black, 4 white, in diagonal
  stripes across the screen (the colour follows x - y: a marquee's top row at y 457 was black
  from x 443 to 446, its bottom row at y 516 from 446 to 449; measured 2026-11-26). FireCraft
  draws them, the crop box's dashes and handles on whole physical pixels, so they stay even at
  any zoom and with display scaling (125%). They used to be canvas pixels long, uneven when
  zoomed and smeared between pixels at 125%.
* **The marching ants run on the selection's outermost pixels**: a marquee 0,40 to 60,100 at
  100% shows its left and top ants on the first selected pixel and its right and bottom ants on
  the last one (x 59, y 99), never on the pixel outside (measured 2026-11-26). FireCraft draws
  every edge of the selection on its selected side, and the marquee's outline while dragging
  the same way.
* One run disagreed: right after a Paint Bucket fill, a drag from inside the marquee drew a new
  marquee instead of moving it. Every other run (with and without a fill first, with and
  without hovering first) moved it; FireCraft always moves it.

FireCraft: `marquee` snaps the rectangle to whole pixels and anti-aliases only ovals. Each corner
goes to the pixel grid line nearest the pointer (`DocState::marquee_box`), as the Crop tool's box
does, and the canvas draws that snapped box while dragging, where the marching ants will run: zoomed
in, what will be selected shows before the button is let go (the user's request, 2026-11-24;
FireCraft once took `floor` of both corners, which agrees with the MX measurement at 100% but
left the end corner up to a pixel behind the pointer when zoomed in). Fireworks MX's own drag
outline zoomed in was not measured; `pixels_contain` and `move_pixel_selection` serve the drag, with a move
cursor over the selection for the Marquee, Oval Marquee, Lasso and Magic Wand tools (Shift or
Alt pressed: they draw a new selection to combine instead). The marching ants show with every
tool, the Brush and Pencil included (they paint inside the selection). Edit > Insert > Empty
Bitmap and the Layers panel's New Bitmap Image keep the selection, which then applies to the new
bitmap.

## The canvas context menu; New Bitmap Via Copy/Cut

A right-click on the canvas opens one of three menus in Fireworks MX (2026-11-15):

* **While pixels are selected:** Edit ▸ (Cut, Copy, Paste, Paste as Mask, Duplicate, Clone),
  Magnification ▸ (Fit Selection, Fit All, 6% to 6400%), Crop Selected Bitmap, Crop Document |
  New Bitmap Via Copy, New Bitmap Via Cut, Merge Down | Select All, Deselect, Select Similar,
  Modify Marquee ▸, Hide Edges | Save Selection, Restore Selection, Selective JPEG ▸ | Free
  Transform, Numeric Transform... | Insert Hotspot, Insert Slice | Exit Full Screen Mode.
* **With objects selected:** Edit ▸ | Magnification ▸, Select ▸ | Hide Selection, Hide Edges |
  Arrange ▸, Transform ▸ | Group, Ungroup | Convert to Symbol, Animation ▸, Symbol ▸, Insert
  Hotspot, Insert Slice, Export Select Slice... | Flatten Selection, Merge Down, Flatten Layers |
  Exit Full Screen Mode.
* **Nothing selected:** Paste, Show All | Magnification ▸, Select All, Flatten Layers | Modify
  Canvas ▸ | Insert New Button..., Insert New Symbol..., Insert Empty Bitmap | Import...,
  Export..., Export Preview... | Toolbars ▸, Panels ▸ | Preferences... | Exit Full Screen Mode.

**New Bitmap Via Copy / Via Cut** (also Edit > Insert > Bitmap Via Copy / Bitmap Via Cut, same undo
step names): the edited bitmap's pixels inside the marquee become a new bitmap right above it in
its layer (only that bitmap's pixels, not others under the marquee); Cut clears them from the
source. The marquee stays and the new bitmap is the one being edited (pencil), selected. Undo
steps: "New Bitmap Via Copy", "New Bitmap Via Cut".

FireCraft shows the same three menus (`menus::context_menu`; not with the Zoom tool, whose
right-click zooms out). Items it has are the same commands as in the menu bar; the others are
disabled. Not captured from Fireworks: the Modify Marquee, Select, Toolbars and Panels
submenus (FireCraft lists Expand/Contract/Border/Smooth Marquee and its Select menu's items, and
leaves out Toolbars and Panels).

## Deleting selected pixels

Fireworks MX (2026-11-15), red at 10,10 100x80, marquee 0,0 to 60,100, Delete (undo step
"Delete"): while bitmap editing goes on red keeps its size (0,0 200x150 as MX edits it); at "Exit
Bitmap Mode" it shrinks to the pixels left, 60,10 50x80. A bitmap whose pixels were all deleted is
removed from its layer at Exit Bitmap Mode (in that step; undo brings it back).

FireCraft has no separate bitmap mode, so it shrinks the bitmap right away, in the Delete step:
Delete, Edit > Clear, Edit > Cut and the source of New Bitmap Via Cut all trim the bitmap to its
remaining pixels (`DocState::shrink_bitmap`); a hole in the middle leaves the bounds as they are.
A bitmap emptied completely stays while it is being edited (the marquee still applies to it) and
goes away when bitmap editing ends or it is deselected, as an Insert > Empty Bitmap one does.

Any exit from bitmap mode shrinks the edited bitmap in Fireworks MX, even when no pixel changed: on
a flat image opened as one 200 x 150 bitmap (pixels covering 10,10 170x120), a Magic Wand click
then Escape left 10,10 170x120 ("Exit Bitmap Mode"). A bare Escape right after opening, with
nothing done, changed nothing and recorded nothing. FireCraft's `exit_bitmap_mode` (Pointer tool,
Escape) shrinks the bitmap the selection applied to in the same step; strokes, fills and deletes
shrink it as they finish ([painting.md](painting.md#which-bitmap-a-stroke-paints-on)).

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
