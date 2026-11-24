# Objects: selecting, arranging, grouping

## Selecting

* **Pointer** (V): click an object to select it (Shift adds or removes); groups are selected
  whole. A selected path is shown by its own outline, highlighted, with blue squares at an open
  path's ends (as in Fireworks); other
  objects and rectangles by their bounding box with corner handles. Pressing inside the selection's box drags the selection even where another object lies
  on top (Fireworks). Objects are picked where they have pixels (a pixel test, 2 px tolerance),
  not by bounding box. **Select Behind** (the Pointer's flyout) selects the next object down at
  the same spot on each click.
* **Selection band** (Pointer dragged from empty canvas): a thin black rectangle; on release every
  object whose **bounding box** it overlaps is selected, objects only partly inside included.
  Shift+drag toggles the objects it touches (adds the unselected, removes the selected). Measured
  in Fireworks MX on 2026-11-15: a band from 2,2 to 115,95 selected a red bitmap at 10,10 100x80
  and a blue one at 80,50 100x80 it only partly covered; a band in the empty top-left corner of a
  sparse diagonal of single pixels (glowpx.png: 129x169 bitmap at 704,423) selected it, while a
  band just outside the box, over its glow only, didn't. Not an undo step. Hidden and locked
  objects and layers are skipped, as with Select All. This is the easy way to pick objects with
  few pixels, which a click hits only on a pixel.
* **Mouse highlight** (Preferences > Editing, on by default, as in Fireworks): with the Pointer,
  the object a click would select is outlined in red under the mouse (a path along its own
  outline with red squares at an open path's ends, other objects by their box); with Select
  Behind, the object its click would pick (the one below the selection at that spot); with the
  Subselection tool, the member inside a group.
* **Subselection** (A): selects inside groups, down to the innermost member under the pointer
  (a group's members, a masked group's content); for paths it also selects and edits points
  (see [paths.md](paths.md)). A selected member can be moved, transformed, given effects,
  opacity and blend mode, reordered (Arrange), duplicated and deleted, all within its group;
  deleting a group's last member removes the group.
* **Select > Superselect** (Ctrl+Right) selects the groups the selected members are in;
  **Subselect** (Ctrl+Left) selects the members of the selected groups.
* Select All (Ctrl+A) selects every object in visible, unlocked layers, except hidden and
  locked objects; Deselect (Ctrl+D).
* Hidden and locked objects can't be selected on the canvas, with Select All, or from the
  Layers panel.

## Commands

| Command | Shortcut | Notes |
|---|---|---|
| Cut / Copy / Paste | Ctrl+X / C / V | shared with Fireworks through the system clipboard ([clipboard.md](clipboard.md)) |
| Clear | Backspace / Delete | with path points selected (Subselection, Pen) removes the points instead |
| Duplicate / Clone | Ctrl+Alt+D / Ctrl+Shift+D | Duplicate offsets the copy by 10 px; Clone keeps it in place |
| Alt+drag | (Pointer) | copies the selection and drags the copies (see below) |
| Group / Ungroup | Ctrl+G / Ctrl+Shift+G | the group takes the place of the topmost grouped object |
| Arrange | Ctrl+Up/Down (Shift: to front/back) | within each layer, or within a group for members |
| Align | Ctrl+Alt+1..7, 9 | left, centre, right, top, centre, bottom; distribute widths/heights (the objects between the outermost land on whole pixels); more in Window > Align ([align.md](align.md)) |
| Rotate / Flip | Ctrl+Shift+9 / 7 | Modify > Transform; also Numeric Transform (Ctrl+Shift+T) |
| Convert to Symbol | F8 | see [symbols.md](symbols.md) |
| Arrow keys | (Shift: 10 px) | nudge the selection, or the selected path points |

Group, Ungroup, Break Apart and Convert to Symbol don't act on members selected with the
Subselection tool yet (roadmap).

## Alt+drag duplicates

With the Pointer (or Select Behind), dragging with Alt held drags copies of the selection and
leaves the originals where they were, as in Fireworks MX (measured 2026-11-18):

* Each copy goes **just above its own original** in the stacking order (two non-adjacent
  objects give two copies, each over its original), unlike Edit > Duplicate, which stacks the
  copies together.
* The copies end up selected; the whole drag is one undo step, "Duplicate".
* Pressing on an unselected object with Alt selects it first and copies it.
* **Alt's state when the mouse button is let go decides**: Alt pressed only during the drag
  still duplicates, and Alt let go before the button makes it a plain Move. FireCraft switches
  the live drag the moment Alt changes, so what you see is what you get.
* An Alt+click that doesn't move makes no copy.

Ctrl+Alt+drag duplicates too, in Fireworks MX and in FireCraft. That helps on Linux desktops
that use Alt+drag to move windows (MATE's default "Movement Key"): there Fireworks MX under
Wine never gets a plain Alt+drag, but Ctrl+Alt+drag still reaches it. FireCraft got plain
Alt+drag on the same MATE desktop (checked 2026-11-18).

## Layers panel

* Each object is a row under its layer, with its own **eye** (show/hide) and **lock** (from
  Fireworks CS; stored as the object's `LCKb`, which Fireworks MX ignores). Hiding or locking an
  object deselects it; each toggle is one undo step.
* Click a row to select the object (Shift adds); double-click to rename it (`OBNs`); drag rows
  to restack objects or move them to another layer, drag a layer's row to reorder layers.
* Rectangles drawn as Fireworks rectangle primitives are labelled "Rectangle", other paths
  "Path", as in Fireworks.

## Moving

Objects follow the pointer while dragged ("Show objects while moving them" in Preferences; off
shows only an outline, for slower PCs). The Scale, Skew and Distort tools and rotating preview
live under the same preference.

## Interpolation

Fireworks MX's Preferences > General > Interpolation ("Set the default pixel transformation
algorithm": Bicubic, Bilinear, Soft, Nearest Neighbor; Bicubic by default) is the preference
key `EditTransformMode` (Bicubic 1, Soft 3; measured 2026-11-16). FireCraft's Preferences >
Interpolation does the same job: bitmaps scaled, rotated, skewed or distorted (the tools,
Rotate/Flip, Numeric Transform, the Property inspector's W and H) are resampled with it. Turns by 90 degrees, flips and
whole-pixel moves copy pixels exactly whatever the choice. Image Size keeps its own choice
(`ImageSizeTransformMode`; setting General to Soft left Image Size at Bicubic); MX also has
`ExportTransformMode` (export scaling) and `AliasTransformMode` (unknown), not used yet. The
kernels are standard ones, not yet measured against MX's ([canvas-size.md](canvas-size.md#not-matched-yet)).

## Property inspector W, H, X and Y

With objects selected the Property inspector shows their box: W, H, X and Y, as the whole
pixels it covers (see below; several objects: the box around them all, as in Fireworks MX; an animation symbol: its
whole motion, see [symbols.md](symbols.md)). W and H scale the objects keeping the box's left
side or top; X and Y move them. Fireworks MX records all four as **"Transform"**, for paths and
bitmaps alike (the user checked, 2026-11-24). X and Y move by whole pixels from the shown,
rounded position, so an object at a sub-pixel place (from an earlier Ctrl transform) keeps its
soft edges as a Pointer drag does instead of snapping onto the grid. An animation's W and H stay
greyed: they cover the motion, which scaling the instance doesn't map onto. Typed values apply on
Enter or on leaving the field; dragging a field changes the objects as it goes, each step scaling
the objects as they were when the drag began (bitmaps are resampled once), one undo step on
mouse up.

### Sub-pixel steps from the Property inspector (keep this)

A FireCraft feature the user relies on (chosen 2026-11-24; `firecraft_engine::covered_pixels` and
`from_shown` in crates/ui-egui/src/inspector.rs, each with a unit test):

* **The fields show the pixels covered.** X and Y are the pixel holding the box's left and top
  edges; W and H count every pixel column and row the box touches, partly covered (soft) ones
  included. The selection box on the canvas outlines the same pixels, so the numbers, the box and
  the drawn pixels agree. W and H are the object's own outline: a path's stroke can reach further.
* **A typed value applies as a change from the shown one** (`real + (typed - shown)`). Whole
  numbers move or resize by whole pixels and keep a sub-pixel edge's look; a fraction adds a
  sub-pixel step.

| Real box | Shows | Type | New real | Shows | Right edge |
|---|---|---|---|---|---|
| x 10, w 50 | X 10, W 50 | W 50.1 | w 50.1 | W 51 | faint (see below) |
| x 10, w 50.1 | W 51 | W 51.9 | w 51 | W 51 | solid again |
| x 10, w 50 | W 50 | W 50.5 | w 50.5 | W 51 | half-covered |
| x 10, w 50.5 | W 51 | W 51.5 | w 51 | W 51 | solid again |
| x 10, w 40.4 | W 41 | W 45 | w 44.4 | W 45 | same look as before |
| x 10.5, w 20 | X 10, W 21 | X 11 | x 11.5 | X 11, W 21 | same look, moved 1 |

Typed values apply on Enter; dragging a field changes by whole pixels.

**Fill edges come in quarter pixels.** Path fills are anti-aliased with 4 x 4 samples per pixel
(`firecraft_raster::path::fill_polygons`, unchanged since FireCraft's first version and not
measured against Fireworks MX), so a right edge at 50.1 draws no partial pixel yet (50.25 draws
25%, 50.5 50%, 50.9 a full one) even though W shows 51. Whether Fireworks MX draws exact
coverage there is still to be measured.

## Scale, Skew and Distort tools: whole pixels

As in Fireworks MX, a Scale tool resize goes by whole pixels: the box sides a handle drags move
a whole number of pixels, so an upright rectangle path on the pixel grid stays crisp instead of
gaining soft edges from a sub-pixel size, and one at a sub-pixel place keeps the same look (its
width or height changes by whole pixels; the user's expectation, 2026-11-24). Dragging inside the
box (any of the three tools) moves by whole pixels. **Holding Ctrl** gives the sub-pixel resize or
move instead (FireCraft's addition, the user's request, 2026-11-24; Ctrl is also the pivot's
"no snapping" key). A turned or slanted box can't sit on the grid and follows the pointer; Skew
and Distort handles and rotation aren't snapped.

With objects selected, the three tools show the objects' Property inspector, as Fireworks MX does;
W, H, X and Y follow the box while a handle or the box is dragged
(`transform_tool::dragged_bounds`, exact even when slow objects are previewed as a warped picture).
The tools' keys moved to their Tools panel tooltips.

While dragging, objects that render quickly are transformed for real at each step; slower ones
(over 30 ms to draw, effects included: a large rectangle with a Blur in a debug build) are shown as
a picture of their look, warped, and transformed for real on release. That picture is drawn in the
object's place (`Element::preview`, never saved): the object stays what it is, so the Layers panel
and Property inspector keep showing "Rectangle" (until 2026-11-24 the picture replaced the object
as a bitmap during the drag). A time just over the limit is measured twice and the faster counts;
`FIRECRAFT_PROFILE=1` logs which preview a drag uses and the time. With nothing selected the inspector shows the
tool's instructions.

## Rectangle and Ellipse tools: whole pixels

A drawn rectangle or ellipse has its corners on the pixel grid lines nearest the press and release
points, and the drag outline shows it there; holding **Ctrl** draws it exactly where the pointer
is, sub-pixel (as with the Scale tool). Note that the default stroke, Fireworks' Pencil "1-Pixel
Anti-Aliased", centred on a grid line covers half of the pixels each side of it; "1-Pixel Hard"
draws it crisp.

## Colour pop-up

Every colour box (Tools panel stroke and fill, the Property inspector's tool options, a
selected path's fill and stroke, the canvas colour, effect and plug-in colours, the Canvas Color
and New Document dialogs) opens Fireworks CS's colour pop-up, as
re-created by FirePixel (`crates/ui-egui/src/fpui/color_picker.rs`): a 21 x 15 swatch grid, a
preview, HEX and RGB fields with copy buttons, opacity with a drop-down slider (fill and stroke
colours only: Fireworks CS6's pop-up has none for effect colours, the canvas or dialogs; a drop
shadow's opacity is its own field, as in Fireworks), None (and, for
fills and strokes, kind/placement buttons), and the colour-wheel dialog. The eyedropper is on as
soon as it opens: the preview follows the colour under the pointer anywhere in FireCraft's
window, and a click there takes it (clicks outside the window don't pick a colour). Esc cancels,
Enter applies typed values. Gradient/pattern fills and stroke placement buttons are shown but
not yet active.

## Stroke controls

The Property inspector shows Fireworks CS's stroke controls (FirePixel's layout,
`crates/ui-egui/src/stroke_panel.rs`), for the selected paths and for new strokes (shape and pen
tools): the colour box (its footer sets the stroke position for a selection), the tip size,
the stroke category menu (None, Fireworks' 12 categories with their 54 presets, Stroke
Options...), the inside/centred/outside buttons, the edge preview and the edge softness.
Choosing a category keeps the stroke's colour, size and position, as Fireworks does; dragging
the size or softness is one undo step. Strokes draw with Fireworks' brush engine
([strokes.md](strokes.md)). The third row is the stroke's **texture** (Fireworks' textures when
its folder is found, FireCraft's built-in ones, those in the document, Other... for a PNG or GIF)
and its amount; edge texture is in Edit Stroke. See [strokes.md](strokes.md#textures).

**Stroke Options...** (end of the category menu) opens Edit Stroke (after FirePixel's
`brush_edit.rs`): Options (ink amount, spacing and flow for natural brushes, build-up, texture,
edge texture, edge effect, tips, tip spacing and variation for several tips, dashes with on/off
lengths), Shape (tip preview, square, size, edge, aspect, angle) and Sensitivity (each stroke
property's dependence on pressure, speed, horizontal and vertical direction and randomness),
with a sample stroke drawn by the brush engine. OK applies it to the selected strokes (one undo
step) or, with nothing selected or from the tool options, to new strokes; the dialog never
changes the brush type (natural/simple), as in Fireworks.
