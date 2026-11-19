# Element masks (Fireworks MX)

Learned by scripting Fireworks MX (`dom.group("mask to image" | "mask to path")`), saving, and
comparing its render (`tools/fwmx-oracle`), then checked on real documents (cone3, comicBorder,
769_edit2 render at 99-100% match).

A masked object is a group (`GRPv`) with a type:

| `GRTi` | Fireworks command | Layout | Effect |
|---|---|---|---|
| 0 | Group | `ELMv` children | plain group |
| 1 | Mask to image (vector mask) | `ELMv` = [mask cache `IMGv`, content...], `MRMv` = [mask object] | content alpha x mask strength |
| 1 | Mask to image (bitmap mask) | `ELMv` = [mask `IMGv`, content...], no `MRMv` | content alpha x mask strength |
| 2 | Mask to path | `ELMv` = [mask path, content...] (mask on top) | clip to the mask path's outline |

* Mask strength for type 1 (grayscale): Fireworks' luma of the mask pixel times its alpha (grey
  `#808080` reveals 50%). Type 2 with a bitmap as the mask is "alpha channel": the bitmap's
  alpha, the bitmap stored as it is (not inverted).
* **Flags** (measured 2026-11-18 with MX's script API on `~/Documents/work/probes/mask_probe_mx.png`;
  the result is `mask_probe_mx_flags.png`): each mask has `element.mask` = { `mode` ("mask to
  image" / "mask to path"), `linked`, `enabled`, `showAttrs`, `autoExpandImages` }, changed with
  `dom.linkElementMask(bool)`, `enableElementMask(bool)`, `setElementMaskMode(mode)`,
  `setElementMaskShowAttrs(bool)`. In the file: `LNKb{0}` when unlinked, `MENb{0}` when
  disabled (both absent otherwise), `MNAb{1}` when the fill and stroke are hidden (MX's default;
  absent when shown), placed before `GRTi`. Mode = `GRTi` (1 grayscale, 2 path outline or alpha
  channel). FireCraft used to read `MNAb` as grayscale/alpha, which was wrong (FirePixel had it
  right); parity on all reference files is unchanged by the fix.
  * **Linked** (default): moving or transforming the masked object moves the mask too.
    Unlinked, only the content moves: `moveSelectionBy(20, 0)` moved the content of an unlinked
    mask from x 20 to 40 and left the mask at 30 (FireCraft: `Element::move_by` /
    `transform_by`, used for the user's moves; canvas-wide moves still move everything).
  * **Disabled**: the content shows whole, unmasked.
  * **Show fill and stroke**: no visible change on a grayscale mask (measured); on a path
    outline (or alpha channel) mask the mask's **fill is drawn under** the content and its
    **stroke over** it (the user's `ref_fw_images/pasteInside.png`: content clipped to the path,
    the path's wide green stroke covering the content's edge; 96.4% -> 100%).
  FireCraft renders that file as MX does (100%).
* **Delete Mask** (`dom.removeElementMask`, `mask_probe_mx_removed.png`): "apply" turns the
  masked object into an Image of what showed, at the mask's bounds (80x60 at 30,30); "discard"
  leaves the content, a mask over two objects becoming a plain Group of them. CS6 asks "Apply
  mask before removing?" (Apply, Discard, Cancel). FireCraft adds **Break Apart**, which does
  what Fireworks MX's **Modify > Ungroup** does to a masked object (measured 2026-11-18: four
  rectangles pasted inside a group of two circles ungroup to a Group of the four and the
  circles' group on top, both selected; `~/Documents/work/probes/ungroup_masked_mx.png`): the
  content, several objects as one group, and the mask on top. The masked object's effects,
  opacity and blending stay with the content. FireCraft's Ungroup does the same. This is MX's
  model: a masked object holding several objects is a group with a mask attached (stored as the
  group's members), which is why the Layers panel shows such content as "Group: N objects".
* **Mask images are stored inverted**: colour channels hold 255 - value (alpha unchanged). Measured
  by masking a red rectangle with flattened ovals: a white oval is saved black and shows the
  content fully, black is saved white and hides it, red (luma 54) is saved `#c9c9c9` and shows
  21%. Bitmap masks are stored this way inline; FireCraft inverts them on read and write, and
  Fireworks MX renders FireCraft's re-saves identically (bolWNWide, max difference 0).
* The mask cache `IMGv` is the mask's render in the same inverted form (fully transparent pixels
  stored as 0 0 0 0, not inverted). It covers the mask's bounds grown by its effects' reach, with
  no stroke or antialiasing margin: in 769_edit2 a group mask of a path and a bitmap with a 5 px
  glow has a cache 397x80 at 326,3, the union of the path and the bitmap grown by 5. FireCraft
  reads it by that rule (it used to need exact mask bounds, and read 769_edit2's cache as a second
  content object), doesn't use it for rendering, and writes it back while the mask is unchanged
  (a fingerprint of the mask taken on reading, `cache_key`). Once the mask changed, or for a
  vector mask without one, saving draws a new cache (`firecraft_doc::mask_cache`). Checked with
  `crates/io/examples/mask_cache_probe.rs` against the caches in the files: Fireworks MX's
  (cone3, mean difference 0.04 per channel), CS6's (probe document, 0.1 to 0.2, antialiasing)
  and 769_edit2 (0.002), with the same rectangles, and a probe made in MX
  (`~/Documents/work/probes/mask_probe_mx.png`: grey, white, path-outline and group masks; 0.01 to
  0.03 for all three caches).
* **Fireworks MX draws a mask from the mask, not from the cache** (measured 2026-11-18): the
  probe with mask A moved 40 px and its cache left at the old place (`mask_cache_probe`'s
  `stale.png`) and the same with a redrawn cache (`fresh.png`) re-save from MX identically
  (0 differing pixels), the mask at its new place. FireCraft renders both as MX does (99.9%).
* Masks nest: a mask can itself be a masked group (cone3.png).
* Objects can be hidden individually: `VISb{0}` as the first child of the object.

## Layers panel

Seen in Fireworks CS6 (2026-11-18, `tools/fwcs6-oracle`, a probe document built with
`dom.group("mask to path" | "mask to image" | "normal")`):

* A masked object's row shows the content's thumbnail, a chain (the link between object and
  mask), then the mask's thumbnail (black where it hides), and the content's name ("Rectangle",
  "Path"). A mask over several objects is named like a group, "Group: 3 objects", and CS6 saves its
  content as separate `ELMv` children, not as a group (FireCraft's `Masked.content` list).
* An unnamed group's row reads "Group: 2 objects".
* CS6 has **no** way to open a masked object or a group in the Layers panel: no arrow on any of
  those rows, and its panel menu only has Add Mask, Edit Mask, Disable Mask and Delete Mask.
  Measured by FirePixel (its RE spec 70): clicking the mask thumbnail selects the mask (green frame
  on the thumbnail, "Vector mask" or "Bitmap mask" in the Property inspector); clicking the object
  thumbnail or the name goes back to the object.

FireCraft extension (the idea comes from FirePixel's group and mask tree): masked objects and
groups have an arrow before their thumbnail that opens them. An open masked object lists its mask
("Vector mask", "Vector mask (path outline)" or "Bitmap mask"), then its content objects; an open
group lists its members; nested groups and masks open the same way. Clicking an object listed
inside selects it alone, like the Subselection tool (Shift adds or removes it); its eye, lock and
renaming work as on top-level rows. Which rows are open is kept per
document while FireCraft runs, not saved.

**Selecting the mask** (CS6, as FirePixel measured it): a click on a masked object's mask
thumbnail, or on its mask row, selects the mask. The thumbnail gets a 2 px #66FF00 frame, the
object's row stays highlighted and the Property inspector says "Vector mask" or "Bitmap mask".
A click on the content thumbnail or the name goes back to the object. With the mask selected the
Pointer moves and transforms the mask alone, and the inspector's fill, stroke and effects apply
to it. A mask that is a group (or itself masked) opens from its row, and the objects in it can be
selected the same way (FirePixel's E-1). Cut, Duplicate and Alt-drag leave a selected mask in its
object (Alt-drag just moves it); Delete (or Edit > Clear) on a selected mask asks how to delete
the mask, as CS6 does.

With a mask selected the Property inspector shows its settings, Fireworks' names: **Mask: Path
outline / Grayscale** and **Show fill and stroke** (enabled for path outline) for a vector mask,
**Mask: Alpha channel / Grayscale** for a bitmap mask. Changing one is an undo step.

The **chain** between the two thumbnails links and unlinks the mask (faded when unlinked); a
disabled mask's thumbnail has a red cross (Fireworks CS). Right-clicking a masked object's row or
its mask row offers Disable/Enable Mask, Link/Unlink Mask and Delete Mask...; Modify > Mask has
the same.

**Dragging rows** (FireCraft): objects inside groups and masks drag like top-level ones: within
their list (reorder), out of it onto any other row (they leave the group or mask), or into an
open group's or mask's list. A group left empty goes; a mask left with no content becomes an
ordinary object, as if broken apart. An object can't be dropped inside itself. The rows whose
objects hold the selection are tinted #9977FF with white text (the user's choice); an unlinked
mask's chain is drawn very faint.

## Paste as Mask

Measured 2026-11-18 in Fireworks MX through Edit > Paste as Mask (menu clicks; the script call
`dom.clipPasteAsMask` refused every argument tried). Files: `~/Documents/work/probes/`
`paste_as_mask_group_mx.png`, `paste_as_mask_add.png`.

* Enabled for exactly one selected object; with two objects (or none) it is greyed out.
* The clipboard's objects keep their own places and become the mask. One vector: path outline
  ("mask to path"); a bitmap: grayscale ("mask to image"); several objects: a group mask, path
  outline. Flags: linked, enabled, fill and stroke hidden.
* Afterwards the mask is selected (MX's title shows "(Mask)"); the history step is "Paste".
* On an object that already has a mask MX asks: "A mask already exists. Do you want to replace
  the mask or add to the existing mask?" (Replace, Add, Cancel). Replace: the pasted objects
  become the mask and set its mode. Add: the mask becomes a group of the old mask with the
  pasted objects on top, the mode unchanged, and the pasted objects are selected inside it.
* A path-outline mask that is a group takes the outline of every path in it (FireCraft
  whitens each member path; MX's file renders identically).

FireCraft takes what Edit > Paste would (Fireworks objects or a picture on the system clipboard,
copied picture files, or its own clipboard); a picture from another application lands centred,
as with Paste.

## Paste Inside

Measured 2026-11-18 in Fireworks MX through Edit > Paste Inside (Ctrl+Shift+V); files
`~/Documents/work/probes/paste_inside_pi1..4.png`, all rendered by FireCraft as MX does (100%).

* For exactly one selected object, like Paste as Mask.
* The selected object becomes the mask and the clipboard's objects (at their own places, each a
  content object of its own) go inside it: a path masks by its outline, a bitmap by its alpha
  channel ("mask to path" either way), with **Show fill and stroke** on, so the mask's own look
  shows under the content.
* Pasted inside an object that already has a mask: no question; the objects join its content on
  top and the mask stays as it was.
* History "Paste". MX's selection afterwards varied between runs (nothing, or the masked
  object); FireCraft selects the masked object.

## Selecting inside masks on the canvas

Measured 2026-11-18 in Fireworks MX on cone3: a **Pointer** click selects the top-level masked
object (the blue rectangle with its mask); a **Subselection** click selects that object's mask
(`fw.selectedMask` set), and a second click stays on it (it doesn't go into a mask that has a
mask of its own). FireCraft does the same (`DocState::hit_sub`: into groups down to the member,
at a masked object its mask).

FireCraft addition, for objects chosen in the Layers panel: a Subselection click that lands on a
selected object (its own hit test) belonging to the same top-level object keeps the selection,
masks' objects included (cone3's four gradient rectangles inside the inner mask), so it can be
dragged, its gradient edited or double-clicked to open a symbol. A Subselection click on a
selected group's member still selects the member.

**Linked masks move with their content, whole**: moving or transforming an object in a masked
object's content moves the whole masked object (its mask and all its content) while the mask is
linked (`DocState::motion_refs`); unlinked, the object moves alone. A selected mask moves alone
(that is editing the mask). In the Layers panel a mask that has a mask of its own (cone3's inner
mask) shows its content, its own chain and its mask side by side, so its link can be toggled
there (unlinked, cone3's gradient rectangles drag one at a time).

Rows holding the selection inside them: purple #9977FF for masks and masked objects, mint
#66EEBB for groups, both with white text (the user's choices).

**A masked object is its content** (as in Fireworks MX, where the masked object is the object
with a mask attached; MX's script API calls a masked rectangle a `RectanglePrimitive` with a
`mask`): with a masked object holding one object selected, the Property inspector is titled for
that object ("Rectangle") and its fill, stroke and text controls edit it
(`DocState::paint_ref`); opacity, blending and size are the masked object's. Effects are shared:
the only object in a masked object's content, without effects of its own, shows and edits the
masked object's effects whichever of the two is selected (`effect_ref`; Fireworks keeps them on
the mask group). A masked object holding several objects is a group with a mask (MX's
`[object Group]`), "Group: N objects". A masked object's mask row is named for its kind
("Vector mask", "Bitmap mask"). A mask that is itself masked (cone3) opens the way breaking it
apart in Fireworks MX shows it: its own mask ("Group: 2 objects", the two circles, in mask
style) and its content as one group row ("Group: 4 objects", the four gradient rectangles) side
by side; that row selects all of its objects and opens to list them.

**Group, Ungroup, Convert to Symbol and Break Apart inside a mask or group** (FireCraft): objects
that share one list (a mask's content, or a group's members) group or become a symbol where they
are, in the topmost one's place; one group selected inside a mask or group ungroups in place, and
symbol instances sharing a list break apart in place (a masked instance counts as its symbol). Objects from
different lists aren't grouped. In the engine a mask is the reference's child slot `MASK_SLOT`
(`ElementRef::mask`, `is_mask`).

**Thumbnails** (Fireworks CS, as FirePixel measured it, its RE spec 73; `crates/ui-egui/src/thumbs.rs`):
34 x 23 frames, the page fitted inside, the object drawn at its place on it (checkered where
transparent; a hidden object still shows). A masked object's thumbnail is its content alone,
unmasked; the mask's is the mask grey on black: its brightness times its alpha for a grayscale
mask, its alpha for an alpha-channel or path-outline mask. A selected mask's thumbnail has the
green frame. They follow the document, but not during a drag or a stroke: they change when the
mouse is released. Not copied: CS6's "masked result with the fill underneath" content thumbnail
when "Show fill and stroke" is on (FireCraft doesn't model that setting yet), its 10 x 10 bitmap
badge and the pen badge of vector masks. Thumbnails are drawn a few per frame (12 ms, at least
one), and an object's fingerprint is checked only after the document changed.
