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
    outline (or alpha channel) mask the mask's own look is drawn under the content.
  FireCraft renders that file as MX does (100%).
* **Delete Mask** (`dom.removeElementMask`, `mask_probe_mx_removed.png`): "apply" turns the
  masked object into an Image of what showed, at the mask's bounds (80x60 at 30,30); "discard"
  leaves the content, a mask over two objects becoming a plain Group of them. CS6 asks "Apply
  mask before removing?" (Apply, Discard, Cancel). FireCraft adds **Break Apart**: the content
  objects and the mask become separate objects again, the mask on top, as before Group as Mask
  (with the masked object's effects, opacity or blending, they stay together in a group that
  keeps them).
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
objects hold the selection are tinted #9977FF (the user's choice). In the engine a mask is the reference's child slot `MASK_SLOT`
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
