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
* **A symbol can't contain itself** (Fireworks MX, checked by the user and in
  mechscene2_loop.png):
  * Dragging a symbol from the Library into a Symbol Editor where it would end up inside itself
    (directly, or because it contains the edited symbol) is refused with Fireworks' message
    "Could not drag and drop. Although symbols may contain other symbols, a symbol can not
    contain a copy of itself."
  * Pasting such an instance works, but the clipboard's symbols that lead back to the edited
    symbol come in as copies (same names, new ids, contents as they were copied): pasting
    Symbol 3 into Symbol 2 (which Symbol 3 is made of) adds a second "Symbol 3" made of a second
    "Symbol 2".
  * A file whose symbols do contain each other draws each such symbol once per chain instead of
    recursing (before 2026-11-05 FireCraft froze: about 12^16 drawings).
* **Break Apart** replaces an instance with the symbol's objects as the instance shows them; the
  symbol stays in the Library.

## 9-slice symbols (Fireworks CS)

In the Symbol Editor the guides come last for the pointer (FireCraft's choice): tools, handles and objects get the
press. A guide can be grabbed on empty canvas with the Pointer or Select Behind, or near the view's edge with any tool.

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

Opening a symbol: double-clicking an instance with the Pointer (or Select Behind), or with the
Subselection tool (as in Fireworks MX), which reaches an instance inside groups and masks: a
selected instance under the pointer first (chosen in the Layers panel, inside a mask too; a
masked instance counts as its symbol), else the instance under the pointer, however deep, opens
in the Symbol Editor.

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

**Perspective instances.** A per-document setting, *Perspective symbols* in the Property inspector and in
Preferences for new documents, like Color math. Files from Fireworks open as *Fireworks MX*, so they look as
saved; FireCraft files keep what they were saved with (`FCPi` 1 = Pixel-correct).

*Pixel-correct* warps the symbol as a whole, in true perspective. Symbols of plain paths are drawn at the size
they're seen (up to 32x, within a pixel budget) and averaged over each pixel where the perspective compresses them, so an enlarged symbol
resolves detail. Their effects are enlarged with them, including blurs, shadows, glows and bevels, so a 10-pixel
bevel on a 20-pixel symbol still covers it at any size. Textures, natural brushes, bitmaps and the plug-in effects
change with scale, so those symbols warp at their own resolution.

*Fireworks MX* transforms each object and its fill handles and fills as usual, then applies effects at canvas
size, so a gradient's bands tighten towards the far side and a 10-pixel bevel stays 10 pixels. Measured on Ripple
and Folds gradients, with Color Fill and Inner Bevel. Strokes keep their width under real perspective but scale
with a plain matrix (measured with Confetti: a perspective term of 0.00001 still scales, 0.0002 doesn't). Only 9-slice guides, not a Fireworks MX feature, keep the warp.
Fill textures aren't mapped: they're laid on the canvas, anchored at its origin, so every object in the
symbol shares one pattern. Measured with mechscene2's floor of 16 tiles with *Line-Horiz 1*: Fireworks MX
draws all the tiles' stripes in one phase, the same with the tiles reordered, or only the first or last kept,
and with the instance moved 1 or 2 pixels down or 1 right. (Elsewhere a texture is anchored to its object
and moves with it.)
Some saved images show the stripes one or two rows off: a stale drawing. Redrawn (every symbol level nudged
and saved again, in Fireworks MX on Windows and under Wine alike), they're anchored at the canvas origin.

## Animation symbols

Fireworks MX's "animation symbols" (Modify > Animation > Animate Selection) are instances with
animation settings. The instance sits in one frame's cell (`ALSv`) and plays across the frames
after it. Measured on 2026-11-15 by creating instances with the JavaScript setters
(`setAnimInstanceNumFrames`, `…OffsetDist`, `…ScaleAmount`, `…StartEndOpacity`,
`…RotationAmount`), saving, and exporting every frame (`dom.currentFrameNum`,
`fw.exportDocumentAs` with PNG 32):

| Tag | Setting | Notes |
|---|---|---|
| `ANNi` | frames | Shown in its own frame and the next `ANNi - 1`; not drawn after that (8-frame document, 5-frame animation: frames 6-8 empty). `ANNi` 1 = not animated. |
| `ANXf`, `ANYf` | movement | Total at the last frame, linear: (40, 20) over 5 frames is +10, +5 per frame. **Rounded to whole pixels** each frame (mechscene2: 23.125 px over 9 steps). |
| `ASCf` | scale, percent | 100 → `ASCf` linearly, about the centre of the instance's bounds (20 px → 25, 30, 35, 40 at 200%). |
| `ANRi` | rotation, degrees | Linear, about the centre; **positive = clockwise**, counter-clockwise is negative (`ffffffa6` = -90). The setters' "cw"/"ccw" argument is ignored. |
| `ASOi`, `AEOi` | start and end opacity | Per mille of an 8-bit alpha (50% = alpha 128, written 501; 20% = 51, written 200). The start alpha is rounded, then each step's alpha (100 → 20%: 255, 204, 153, 102, 51; 50 → 100%: 128, 160, 192, 223, 255). |

Tags at their defaults (`ANRi` 0, `ASOi`/`AEOi` 1000) are left out; `ANNi ANXf ANYf ASCf` are always
written, in the order `ANNi ANXf ANYf ANRi ASCf ASOi AEOi` after `XFMi`. Start frame and loop count
setters stored nothing and changed nothing (the instance's own cell is its start).

* **Symbol frames:** step `k` shows frame `k` of the symbol (Animations.png: Planet has 8 frames and
  `ANNi` 8, Clock 2 and 2…).
* **Stacking in later frames: upside down.** In the frames after its own, Fireworks MX draws the
  playing instances in the reverse of their stacking order: in Animations.png the bottom instance
  (a cup) is in front from frame 2 on and the top ones (countdowns) end up under the planet.
  FireCraft draws them under the frame's own objects (that order is not measured: the later cells
  of both files are empty).
* In a shared layer an animation starts in the first frame; instances that don't animate show in
  every frame, as before.

### Creating and editing (measured in Fireworks MX, 2026-11-15)

* **Modify > Animation**: Animate Selection... (Alt+Shift+F8), Settings..., a separator, Remove
  Animation. With ordinary objects selected only Animate Selection is enabled; with an animation
  symbol selected only Settings and Remove Animation.
* **The Animate dialog**: Frames (5), Move (72), Direction (0), Scale to (100), Opacity (100) to
  (100), Rotate (0) with CW / CCW; Cancel, OK; for an existing animation (Settings) also Edit...,
  which opens the symbol. Move 50 at Direction 30 is stored as `ANXf` 43.30127, `ANYf` -25:
  Direction counts **counter-clockwise from the right**. Settings shows the stored movement
  rounded: 40, 20 appears as Move 45, Direction 333.
* **Animate Selection** = Convert to Symbol with the type Animation (`MSYi` 2) plus the settings,
  one undo step "Convert to Symbol". On an instance of a graphic symbol it makes a new animation
  symbol holding that instance.
* **Frames**: when the animation needs more frames than the document has, Fireworks MX asks "The
  animation of this symbol extends beyond the last frame of the document. Automatically add new
  frames?" (Don't show again, OK, Cancel); OK adds empty frames ("Add Frames").
* **Property inspector** for an animation symbol: "Animation" and a name; Frames, Opacity from/to,
  Scaling, Rotation with clockwise/counter-clockwise buttons (movement only in the dialog). Its
  W/H/X/Y cover the whole motion (a 20 x 20 square moved 40, 20: W 60). Changes are recorded as
  "Instance Properties". With Rotation 0 the direction can't be stored (it's `ANRi`'s sign), so
  clockwise stays selected.
* **Motion path**: a selected animation shows a dotted blue line through the centres of its
  placements, with a 6-pixel diamond outlined in white at each: green (0, 255, 0) at the start,
  blue (0, 168, 255) for the frames between, red (255, 0, 0) at the end. The cursor stays the
  arrow over them. Dragging them with the Pointer, each drag one "Instance Properties" step:
  * **the end** goes to the pixel under the pointer; the instance stays (a square centred at
    30, 30, released over pixel 90, 39: `ANXf` 60, `ANYf` 9);
  * **the start** moves the instance so its centre is under the pointer; the end stays;
  * **a frame between** moves the whole animation by the drag;
  * **Shift** keeps the dragged point at a multiple of 45 degrees from the other end: level or
    upright keeps the pointer's distance on that axis (83, 22 -> 83, 0), a diagonal takes the
    larger of the two (55, 66 -> 66, 66).

  While dragging, Fireworks MX moves only the path and the instance follows on release;
  FireCraft moves the instance along live.
* **Remove Animation** makes the symbol a graphic symbol ("Symbol Properties"); the instance keeps
  `ANNi` and the rest but no longer plays. **Only instances of animation symbols play.**

FireCraft implements all of the above (`engine::animation`, `AnimateSettings`, `PathPoint`; the
dialog, the prompt with "Don't show again" kept in the preferences, the inspector fields, the
motion path and its dragging in `ui-egui/src/motion_path.rs`). Not yet: the Layers panel's
"Animation Symbol" label, Edit > Insert > New Symbol.

FireCraft: `element::Animation` (read and written with the instance), `Layer::shown` (what a frame
draws, both renderers). mechscene2 and Animations.png now match Fireworks MX's exported frames to
the level of their first frame (Animations: 28-200 pixels differing per frame, first frame 114).
Not yet: setting animations up (Animate Selection, the Property inspector's animation fields),
selecting an animation instance in the frames after its own, and nested animations inside symbols.
