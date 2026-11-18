# The Align panel (Window > Align)

Fireworks MX's Align panel is a Flash movie shipped in `Configuration/Command Panels/Align.swf`
(an Exchange extension, "Align Panel" 1.0) that drives the document through Fireworks' script
API. The help only says it exists (*Using Fireworks MX*: "Flash SWF movies used as Fireworks
panels"). Its controls and labels below were read off the panel running in Fireworks MX under
Wine; what each button does was found from the script calls the movie makes and then
measured in Fireworks MX (2026-11-18), with the numbers in
`crates/engine/src/align_panel.rs`'s tests.

As in a fresh Fireworks MX install, the panel shares a group with Optimize: the dock's
**Optimize and Align** group has the tabs Optimize and Align (Align second), and Window > Align
opens the group on the Align tab. (The oracle's Wine prefix had Align floating on its own, a
saved panel layout, not MX's default.) To Canvas, Anchors mode and the point part are kept in
the preferences, as MX's panel keeps them between sessions.

## Layout

* **To Canvas** (toggle, left) and **Anchors** (toggle, right). A toggle that is on looks
  pressed, with its glyph in orange.
* **Align:** left edge, horizontal center, right edge | top edge, vertical center, bottom edge.
* **Distribute:** top edge, vertical center, bottom edge | left edge, horizontal center,
  right edge.
* **Match Size:** width, height, width and height. **Space:** evenly vertically, evenly
  horizontally, and under them (from Fireworks CS6, which puts the three in a grey box of
  their own) a field with "Evenly" or a gap in pixels, with a menu.

The spacing field works as in Fireworks CS6 (watched in CS6, 2026-11-18): it shows "Evenly"
until clicked; a click turns it into the last gap typed (0 at first) so a number can be typed
straight away (CS6 selects the 0 on the first click only and later puts the caret after the
old gap; FireCraft always selects it, so typing replaces it). It takes digits and a leading
minus sign only; letters are ignored. Its menu lists the gap ("12 px") and Evenly. Emptied, it
means Evenly, as in CS6. The last gap is kept between sessions.
* In **Anchors** mode To Canvas, Distribute, Match Size and Space are hidden. Under the Align
  row are three toggles for the part of each selected point that lines up: preceding control
  points, anchors and control points (the default), succeeding control points; the text under
  them describes the one chosen.

Tooltips are MX's ("Align left edge", "Distribute vertical center", "Match width and height",
"Space evenly horizontally", ...).

## What the buttons do

Bounds are each object's bounding box. The reference is the selection's combined bounds, or
the canvas with To Canvas on.

* **Align**: each object puts the chosen edge or centre on the reference's. One object alone
  lines up with the canvas only with To Canvas on (Modify > Align uses the canvas for a single
  object anyway).
* **Distribute**: the objects are ordered by the chosen edge or centre, and those edges are
  spaced evenly. Without To Canvas the first and last stay put. With To Canvas the first goes
  to the canvas's start and the last to its end; a single object goes to the start.
* **Match Size**: every object is scaled to the largest width and/or height among them (with
  To Canvas, the canvas's size if larger), keeping its top-left corner. Bitmaps are resampled
  with Preferences > Interpolation.
* **Space**: without To Canvas this is Modify > Align > Distribute Widths/Heights (Fireworks'
  `distribute`). With To Canvas the objects, ordered by left (top) edge, fill the canvas from
  edge to edge with equal gaps, each placed after where the previous one ended up.
* **Space by a gap (from Fireworks CS6)**: with a gap in the spacing field (negative gaps
  overlap), the objects in order of their top (left) edges follow each other with
  that many pixels between them: the first stays where it is, or with To Canvas goes to the
  canvas's top (left) edge, and the others follow, the last one included. Every object after
  the first lands on a whole pixel (halves up), while the running position adds up exactly
  (a path at 20.5, 60 wide, then a 41.5-wide one with a 7 px gap: 20.5, 88, 136). This is
  CS6's panel script (`__space` in its `Align.swf`, a Flex panel), recovered from the movie's
  bytecode and run in Fireworks CS6 on 2026-11-18; every case in the tests matched. Fireworks
  MX files keep the result (it is only moved objects).
  CS6's own Evenly differs a little from MX's (it rounds the gap to whole pixels and, with To
  Canvas, puts the last object at the canvas's end); FireCraft's Evenly stays MX's.
* **Anchors mode**: the selected points of the selected path (Subselection or Pen) line up,
  two or more of them. Left/Right/Top/Bottom take the outermost point; for a handle part, the
  point whose handle is outermost, and that handle's position. The centres take the middle
  between the outermost points' *anchors*, for handle parts too (as MX's panel does). Moving
  an anchor moves its handles with it; aligning handles leaves the anchors. A selection with
  anything other than paths gives "Invalid selection. Please try again." (MX shows it as an
  alert).

Each click is one undo step. MX names every one "Command Script"; FireCraft uses "Align",
"Distribute", "Match Size", "Space" and "Align Points".

## Positions

* Paths keep fractional positions (centring a 60 px rectangle on a 325 px selection put it at
  152.5).
* Bitmaps go to the nearest whole pixel, halves rounding **up** from the target (152.5 -> 153,
  157.5 -> 158, 10.5 -> 11 for a move to the left, 82.5 -> 83).
* Modify > Align > Distribute Widths/Heights (and Space without To Canvas) put the objects
  between the outermost ones on whole pixels, **rounded down** from where equal gaps would put
  them, for paths too (94.5 -> 94; 66.67 and 133.33 -> 66 and 133). The gaps add up from
  unrounded positions. The order is that of the objects' centres: with x 0 (10 wide), 50 (40
  wide), 60 (10 wide) and 200 (20 wide), MX moved the 10-wide one to 56 and the 40-wide one to
  113.

## Not matched

* **Distribute Widths with overlapping objects**: when an object's centre lies outside the
  outermost objects' order (x 10 150 wide, x 40 20 wide, x 200 30 wide), MX moved the small one
  to 95, where equal gaps give 170; applying it again gave 170. A wide object lying under two
  smaller ones (x 100 and 150, 10 wide, over x 0, 300 wide) moved them to 93 and 196. MX's
  rule for these is unknown; FireCraft spaces them with equal gaps in the order of their
  centres.
* MX's panel aligns points of the first contour of each selected path, across several
  selected paths. FireCraft aligns the selected points of all contours, of the one path whose
  points are selected (its Subselection tool selects points in one path at a time).
* Unmeasured: bitmaps at negative half-pixel targets (FireCraft rounds -2.5 up to -2), text
  and groups moved to fractional positions (FireCraft: text and groups move by the fractional
  amount; bitmaps in a group round their own move), whether Match Size scales strokes and
  effects in MX (FireCraft: as Numeric Transform does).
