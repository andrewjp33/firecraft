# Symbols

Measured with Fireworks MX (`dom.convertToSymbol` on test shapes, then the saved tree; see
`docs/fireworks-png.md` for the node layout).

* **Convert to Symbol** (F8) centres the content on the symbol's origin: a 50x40 rectangle at
  (10, 10) is stored as -25..25 x -20..20 and its instance's matrix translates by (35, 30). The
  instance takes the place of the topmost converted object. FireCraft rounds the centre to whole
  pixels so bitmaps stay sharp.
* **Types** (`MSYi`): Graphic (absent), Button (1; four frames: Up, Over, Down, Over While Down;
  instances show Up), Animation (2; instances play along with the document's frames).
* New symbols get the node Fireworks writes for them (`LUTs`, grid/guides, `ATSb` 1 ...); new
  instances `XFMi` 0, `ANNi` = frame count, `ANXf`/`ANYf` 0, `ASCf` 100.
* **Break Apart** replaces an instance with the symbol's objects as the instance shows them; the
  symbol stays in the Library.

## Where FireCraft differs from Fireworks MX

* The Symbol Editor (a document window, as in MX) updates the document and its instances live,
  not only when it closes.
* It has a **Done** button (from Fireworks CS) that closes it and returns to the document.
* **Save** in the Symbol Editor saves the document the symbol belongs to (MX doesn't save from
  the Symbol Editor; with live updates, saving the document is the useful meaning).
