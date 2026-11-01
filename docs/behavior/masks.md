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
