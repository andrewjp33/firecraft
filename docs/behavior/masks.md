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

* Mask strength for type 1 with `MNAb{1}`: Fireworks' luma of the mask pixel times its alpha
  (grey `#808080` reveals 50%). `MNAb{0}` is taken to be "alpha channel" mode (not yet seen in a
  file; the JS `setElementMaskMode` signature still has to be found to confirm it).
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
  and 769_edit2 (0.002), with the same rectangles. Whether Fireworks MX draws from the cache or
  from the mask is not yet measured.
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
renaming work as on top-level rows. Rows whose objects hold the selection are tinted pale yellow.
Objects inside can't be dragged to another place in the list. Which rows are open is kept per
document while FireCraft runs, not saved.

**Selecting the mask** (CS6, as FirePixel measured it): a click on a masked object's mask
thumbnail, or on its mask row, selects the mask. The thumbnail gets a 2 px #66FF00 frame, the
object's row stays highlighted and the Property inspector says "Vector mask" or "Bitmap mask".
A click on the content thumbnail or the name goes back to the object. With the mask selected the
Pointer moves and transforms the mask alone, and the inspector's fill, stroke and effects apply
to it. A mask that is a group (or itself masked) opens from its row, and the objects in it can be
selected the same way (FirePixel's E-1). Delete, Cut, Duplicate and Alt-drag leave a selected mask
in its object: they do nothing, and Alt-drag just moves it (Delete Mask belongs to Modify > Mask,
not built). In the engine a mask is the reference's child slot `MASK_SLOT`
(`ElementRef::mask`, `is_mask`).
