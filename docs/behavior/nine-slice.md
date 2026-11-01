# 9-slice scaling

Fireworks MX has no 9-slice scaling; Fireworks CS5 added a **9-slice Scaling tool** (destructive,
works on any objects) and CS persistent 9-slice guides on graphic symbols. FireCraft implements
the tool now, because its output is plain objects and keeps files Fireworks MX compatible.

* Tool: shares the Scale tool's slot (right-click the slot, or press Q to cycle). Select objects,
  drag the four dashed guides, then drag the corner handle (or type W/H in the Property inspector
  and press Apply). Undoable as one step.
* Mapping per axis (rule adapted from FirePixel, see ATTRIBUTION.md): guides are rounded to whole
  pixels; the margins outside the guides keep their size, the middle stretches; a guide outside the
  box gives a zero margin; if margins overlap or the target is smaller than both margins, that axis
  scales normally.
* Paths: anchors and control handles are mapped as absolute points, so curves inside the fixed
  margins keep their exact shape. Bitmaps are resampled bilinearly, with sampling clamped to each
  output pixel's source slice so seams stay crisp.
* Planned: persistent 9-slice symbols (needs symbols first), saved as `NGSv` on the symbol, which MX
  ignores and CS understands.
