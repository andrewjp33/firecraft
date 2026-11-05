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
* **Symbols of symbols**: an instance's outline (the dotted box, Scale/Skew/Distort handles,
  9-slice) comes from its symbol's content bounds, which count nested instances (innermost
  first, also after editing a nested symbol). Until 2026-11-05 a symbol made only of other
  symbols' instances had no bounds, so its instances (mechscene2.png's floor, Symbol 3) could be
  dragged but showed no outline and couldn't be transformed.
* **Break Apart** replaces an instance with the symbol's objects as the instance shows them; the
  symbol stays in the Library.

## 9-slice symbols (Fireworks CS)

A symbol's `NGSv` holds its 9-slice guides: `NGLb` (locked), `NGKi` (colour) and four `NGCf`,
top y, bottom y, left x, right x in symbol space (`9slicesymbol*.png`: -44, 50, -166, 161 for
content -236..235 x -149..148), plus `NGMv`/`NGVv`. An instance maps the symbol's objects
through the slices: corners keep their size, the edge strips stretch one way, the middle both
ways, each object (bitmap, text image, path) by its position.
The slices use the instance's stretch along the symbol's own width and height (the mean
length of opposite sides of its transformed box over the symbol's size); the rest of the
transform (rotation, skew, flip, perspective) applies on top, so a 9-slice instance keeps its
slices under every transform. Instances smaller than the corners scale plainly.

FireCraft writes `NGSv` the way Fireworks CS does (after `GDSv`; `NGLb` 0, `NGKi` ff0000ff,
the four `NGCf`, empty `NGMv`/`NGVv`), so 9-slice symbols made or edited in FireCraft keep
their guides for CS, while Fireworks MX opens the file normally and scales them plainly
(checked: MX opens a FireCraft file with a 9-slice symbol; a CS file's `NGSv` survives a
FireCraft save unchanged). Guides are enabled in Convert to Symbol or the Library and moved
by dragging them in the Symbol Editor.
With the bitmap flattened, Fireworks CS renders the instances the same way (parity 72.8%; the
rest is resampling); with the original unflattened bitmap it leaves the text unscaled and
moves it, which FireCraft doesn't reproduce.

## Where FireCraft differs from Fireworks MX

* The Symbol Editor (a document window, as in MX) updates the document and its instances live,
  not only when it closes.
* It has a **Done** button (from Fireworks CS) that closes it and returns to the document, or
  to the Symbol Editor it was opened from.
* **Nested symbols**: a symbol opened from inside another Symbol Editor (Symbol 2 from Symbol 3
  in mechscene2.png) still belongs to the document: its edits go straight to the document, and
  every open Symbol Editor of that document picks up the changed symbols. (Before 2026-11-05 they
  went only into the outer editor's copy of the Library and were lost on Done.) Symbols made
  inside a Symbol Editor (Convert to Symbol there) join the document's Library.
* **Save** in the Symbol Editor saves the document the symbol belongs to (MX doesn't save from
  the Symbol Editor; with live updates, saving the document is the useful meaning).
* The Pointer picks a symbol instance where its content is (its distorted shape), not
  anywhere inside its dotted bounding rectangle as Fireworks does. Kept deliberately; noted in
  case it matters later (e.g. clicking a heavily distorted instance near its corners).
* 9-slice symbols keep their slices when skewed, distorted or put in perspective. Fireworks CS
  asks whether to remove the 9-slice scaling for those transforms (and for the 9-slice tool);
  FireCraft doesn't need to.
* Fireworks CS5 hangs opening a file with a skewed or perspective 9-slice instance (CS never
  makes one). For a symbol with such an instance FireCraft writes the guides in its own `FNGv`
  node instead of `NGSv`: CS5 then opens the file with 9-slice unchecked for that symbol
  (confirmed in CS5), and FireCraft reads the guides back. If CS5 re-saves the file the guides
  are lost; enabling 9-slice again in FireCraft puts them at the thirds.
