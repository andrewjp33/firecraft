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
* The mask cache `IMGv` (same inverted form) has exactly the mask's bounds; FireCraft keeps it and writes it back, but
  doesn't use it for rendering.
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
document while FireCraft runs, not saved. The mask row can't be selected yet (mask editing, roadmap
item 16).
