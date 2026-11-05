# Objects: selecting, arranging, grouping

## Selecting

* **Pointer** (V): click an object to select it (Shift adds or removes); groups are selected
  whole. A selected path is shown by its own outline, highlighted (as in Fireworks); other
  objects and rectangles by their bounding box with corner handles. Pressing inside the selection's box drags the selection even where another object lies
  on top (Fireworks). Objects are picked where they have pixels (a pixel test, 2 px tolerance),
  not by bounding box. **Select Behind** (the Pointer's flyout) selects the next object down at
  the same spot on each click.
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
| Group / Ungroup | Ctrl+G / Ctrl+Shift+G | the group takes the place of the topmost grouped object |
| Arrange | Ctrl+Up/Down (Shift: to front/back) | within each layer, or within a group for members |
| Align | Ctrl+Alt+1..7, 9 | left, centre, right, top, centre, bottom; distribute widths/heights |
| Rotate / Flip | Ctrl+Shift+9 / 7 | Modify > Transform; also Numeric Transform (Ctrl+Shift+T) |
| Convert to Symbol | F8 | see [symbols.md](symbols.md) |
| Arrow keys | (Shift: 10 px) | nudge the selection, or the selected path points |

Group, Ungroup, Break Apart and Convert to Symbol don't act on members selected with the
Subselection tool yet (roadmap).

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

## Colour pop-up

Every colour box (Tools panel stroke and fill, the Property inspector's tool options, a
selected path's fill and stroke, the canvas colour, effect and plug-in colours, the Canvas Color
and New Document dialogs) opens Fireworks CS's colour pop-up, as
re-created by FirePixel (`crates/ui-egui/src/fpui/color_picker.rs`): a 21 x 15 swatch grid, a
preview, HEX and RGB fields with copy buttons, opacity with a drop-down slider, None (and, for
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
([strokes.md](strokes.md)); stroke textures are not loaded yet.

**Stroke Options...** (end of the category menu) opens Edit Stroke (after FirePixel's
`brush_edit.rs`): Options (ink amount, spacing and flow for natural brushes, build-up, texture,
edge texture, edge effect, tips, tip spacing and variation for several tips, dashes with on/off
lengths), Shape (tip preview, square, size, edge, aspect, angle) and Sensitivity (each stroke
property's dependence on pressure, speed, horizontal and vertical direction and randomness),
with a sample stroke drawn by the brush engine. OK applies it to the selected strokes (one undo
step) or, with nothing selected or from the tool options, to new strokes; the dialog never
changes the brush type (natural/simple), as in Fireworks.
