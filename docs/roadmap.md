# Roadmap

Priorities follow Fireworks MX's everyday workflow. Each item is "done" when it matches Fireworks MX
(measured with the oracle where possible) and is covered by tests.

## Done

- Fireworks PNG container, lossless tree, tiles; open/save verified in Fireworks MX; plain
  MX-format saves that Fireworks CS5 also opens (checked by hand)
- Workspace: menus, Tools panel with flyouts, Property inspector, Layers, Frames/History, Library;
  document windows (move, resize, minimise/maximise, Cascade/Tile) or tabs
- Rendering: blend modes, gradients (all 11 MX types), masks, symbol instances (perspective),
  animation frames and playback, Live Effects and the bundled Eye Candy / Splat LE filters
  ([parity.md](parity.md))
- Tools: Pointer, Select Behind, Subselection (path points; members inside groups), Scale /
  Skew / Distort / rotate with live preview, 9-slice Scaling tool, Marquee / Oval Marquee (Normal,
  Fixed Ratio and Fixed Size styles), Magic Wand (Classic + Smooth anti-alias), Live marquee for
  both (Fireworks CS: changing Edge or Tolerance redoes the selection just made), Line, Pen, Rectangle (primitives with roundness), Ellipse,
  Eyedropper, Hand, Zoom, Brush, Pencil, Eraser, Paint Bucket ([behavior/painting.md](behavior/painting.md))
- Crop tool, Edit > Crop Document and Crop Selected Bitmap, saved as Fireworks MX saves a crop
  (moved canvas origin) ([behavior/crop.md](behavior/crop.md))
- Masks, editing (v0.3.0, [behavior/masks.md](behavior/masks.md)): the Layers panel's mask rows
  with rendered thumbnails, the link chain and an arrow that lists a mask and the objects inside
  (select, rename, drag in, within and out); selecting the mask; Modify > Mask (Reveal/Hide All,
  Reveal/Hide Selection, Group as Mask, Disable, Link, Invert, Delete Mask with Apply, Discard
  or Break Apart), Edit > Paste as Mask and Paste Inside, the Add Mask button; the Property
  inspector's mask settings; painting on bitmap masks; Group/Ungroup/Convert to Symbol/Break
  Apart inside masks; mask flags (`LNKb`, `MENb`, `MNAb`) and caches saved as Fireworks MX
  saves them
- Modify > Canvas > Canvas Size (units, anchor) and Image Size (pixel/print size, resolution,
  constrain, resample methods) ([behavior/canvas-size.md](behavior/canvas-size.md))
- Pixel selections belong to the canvas: picking another bitmap in the Layers panel keeps the
  marquee and applies it to that bitmap ("Edit Bitmap"); selection changes are undo steps and undo
  brings the marquee back ([behavior/selection.md](behavior/selection.md))
- Empty bitmaps: the Layers panel's New Bitmap Image and Edit > Insert > Empty Bitmap, removed again
  when deselected unused ([behavior/painting.md](behavior/painting.md#empty-bitmaps))
- Text tool: click or drag to make text, in-place editing, per-character formatting, Property inspector,
  Missing Fonts, Fonts folder, saving text Fireworks MX opens ([behavior/text.md](behavior/text.md))
- Fills: Fill Options, Edit Gradient (stops, opacity, presets), Web Dither, Pattern, handles on the
  canvas, Gradient tool, Hard/Anti-Alias/Feather edges ([behavior/gradients.md](behavior/gradients.md))
- Objects: clipboard (shared with Fireworks), Duplicate/Clone, Group/Ungroup, Arrange, Align,
  Rotate/Flip, Numeric Transform, names, per-object show/hide and lock, Layers panel reordering,
  Superselect/Subselect, live dragging
- Live Effects editor in the Property inspector
- Window > Align: Fireworks MX's Align panel (Align, Distribute, Match Size, Space, To Canvas,
  and Anchors mode for path points), plus Fireworks CS6's Space by a gap in pixels
  ([behavior/align.md](behavior/align.md))
- All of Fireworks MX's filters as Live Effects and in the Filters menu: Auto Levels, Levels and Curves
  (editors with histograms and Auto), Blur, Blur More, Sharpen, Sharpen More, Unsharp Mask, Find Edges,
  Invert; bit-exact except Unsharp Mask and Invert's soft edges ([behavior/effects.md](behavior/effects.md))
- Symbols: Convert to Symbol, Library, Symbol Editor, Break Apart, 9-slice symbols (`NGSv`)
- Clean-room oracle tooling and UI test helpers (`tools/fwmx-oracle`)
- Optimize panel, Preview and File > Export: GIF, animated GIF, JPEG, PNG 8/24/32 with Fireworks MX's
  palettes and settings (stored in `EXPv` as Fireworks MX does), plus WebP and JPEG XL with Quality and
  Alpha Quality; Auto Crop/Auto Difference; exact alpha with the Exact palette; lossless PNG optimizer (OptiPNG-style
  levels); the color table with edited, locked and transparent colors, custom palettes, Load/Save Palette
  ([behavior/export.md](behavior/export.md))
- File > Open Recent: Fireworks MX's 10 files with MFC's shortened names, reopened on the desktop and
  (Chrome/Edge, with file handles kept in IndexedDB) in the browser ([behavior/opening-files.md](behavior/opening-files.md#open-recent))
- File > Revert: Fireworks MX's question, the file read again in the same window and zoom, undo
  history cleared ([behavior/opening-files.md](behavior/opening-files.md#file--revert))
- Tool options and each effect's last settings kept between sessions, as Fireworks MX keeps them;
  Defaults button in effect popups ([behavior/effects.md](behavior/effects.md#last-settings-and-defaults);
  settings of tools not yet built: Next, item 12)
- Opening flat images as Fireworks MX does: PNG, JPEG, GIF (animated GIFs as frames), BMP, TIFF,
  Targa, plus WebP, JPEG XL and animated PNG ([behavior/opening-files.md](behavior/opening-files.md))

## Next

Rendering parity on real files is tracked in [parity.md](parity.md).

Other items keep their numbers.

1. **Paths, stage 2**: Modify > Combine Path (Join, Split, Union, Intersect, Punch, Crop), Alter
   Path (Simplify, Expand Stroke, Inset Path), Freeform and Reshape Area, Knife, Redraw Path,
   Polygon/Star tool
2. **Group members, the rest**: Group, Ungroup, Break Apart and Convert to Symbol work for members
   that share one group or mask (and the Layers panel lists, selects and drags them); members from
   different groups or masks still do nothing
3. **Lasso and Polygon Lasso** (the Crop tool is done); the Export Area tool
   - **Transforming selected pixels**: the Marquee tools' W/H/X/Y only show the bounds today. In
     Fireworks MX (measured 2026-11-27) typing there scales or moves the pixels inside the marquee
     ("Transform" steps): they lift out, leaving a transparent hole, and float until the selection
     ends (moved over green and away again, the green was untouched). The same floating pixels
     serve Free Transform and the Scale/Skew/Distort tools on a marquee (now disabled).
4. Blur/Sharpen/Dodge/Burn/Smudge, Rubber Stamp; the Paint Bucket's gradient and pattern fills;
   write `PCLv` for new paths so random brushes scatter the same in Fireworks
   - Investigate the Outline brush's soft-edge opacity (alpha remap 5) against Fireworks MX
     ([behavior/strokes.md](behavior/strokes.md), Not yet)
5. **Text**: text in a path (Fireworks CS), Fireworks MX's kerning quirks (no " A", an "St" Arial lacks; map them with MX under Wine, which reproduces Windows), synthetic bold like MX's (curves kept, narrower), Check Spelling (last).
6. **Fills**: the Gradient tool's exact behaviour against Fireworks MX
   - Pattern fills from JPEG patterns (smoke_fc.png, Bubbles): Fireworks MX redraws a pattern from its
     own Patterns folder, decoded by libjpeg; FireCraft's JPEG decoder (zune-jpeg) differs by up to 4
     levels on half the pixels (jpeg-decoder: 3). Also measure where Fireworks MX puts a new pattern's
     handles on an odd-sized object (FireCraft: on the half pixel)
7. **Optimize panel + Export, the rest** (done so far: [behavior/export.md](behavior/export.md); the
   color table with custom palettes, Load/Save Palette, Remove Unused Colors, the press-and-hold
   highlight, Show Swatch Feedback and Fireworks MX's palette order)
   - **Previews and export dialogs**: 2-Up/4-Up and Export Preview (the transparency eyedroppers and
     swatch feedback should work in their views too); Export Wizard; export in the background (the PNG
     optimizer's level 7 can take a while on large images, and the app waits for it)
   - **Optimize panel menu items not built**: Save Settings / Delete Settings (the Settings row's + and
     -, saved presets in an Export Settings folder), Optimize to Size, Sharpen JPEG Edges
   - **Color table**: Replace Palette Entry (Fireworks MX's palette menu: swap an entry for another
     color, measure in Fireworks MX how it differs from Edit Color); perhaps scroll the table to the hovered swatch (with a modifier key, so
     it doesn't jump while the pointer moves over the image)
   - Done after measuring in Fireworks MX (`probes/sort_edit_mx`): edits keep the palette (no switch to
     Custom), Sort by Popularity reorders an Exact file, Luminance by Rec. 709, Show Swatch Feedback
     is a preference ([behavior/export.md](behavior/export.md#the-color-table))
   - **Fireworks MX's own adaptive palette**: its color count (cone3 as PNG 8 Adaptive 256 with alpha:
     Fireworks MX 195 entries, FireCraft's median cut 256 with 10 unused), the order of a reduced
     palette (Fireworks MX's is unsorted, FireCraft's most used first), the small color shifts of its
     animated-GIF Adaptive (#00C000 to #00BD00), and a GIF with alpha ordered by the semi-transparent
     colors' own lightness before the matte (FireCraft: after); plus its dither pattern
   - More dither types, including ones that stay stable from frame to frame in animations (with
     "Maximize colors per frame" each frame has its own table)
   - GIF Loss (lossy LZW); Selective JPEG quality; animated WebP export; WebP and the PNG optimizer in
     the browser build (libwebp and libdeflate need a C-to-wasm compiler)
   - Per-slice optimize settings (with slices, item 9)
   - **16-bit colour (PNG 48/64 export)**: layers, effects and the renderer work in 8 bits per
     channel today (Precise colour math computes in `f64` but stores 8 bits between steps), so a
     16-bit export would only hold 8-bit values. Needs 16-bit (or float) images through the
     pipeline, 16-bit PNG/TIFF/PSD import, and a document setting
8. **Frames**: adding/removing/timing frames, onion skinning, animated GIF export; Tween
   Instances; button symbol states
   - **Animation symbols**: playback, Modify > Animation, the Animate dialog, the Property
     inspector's fields and the motion path (shown and dragged) done
     ([behavior/symbols.md](behavior/symbols.md#animation-symbols)). Still to do: selecting an
     animation instance in its later frames, animations nested inside symbols
9. **Web layer**: slices, hotspots, HTML export, rollovers (Behaviors panel)
10. Styles and URL panels; an opt-in **Fireworks CS6** save mode (reference: FirePixel's fp-fwpng)
11. Filters menu, the rest: Levels/Curves eyedroppers; check the menu's Sharpen strengths and
    filters on paths/text against Fireworks MX; Unsharp Mask's exact blur; Invert's soft-edge weight
12. **Remember last-used values across restarts, the rest**: done for the tool options that exist
    and for every effect's settings (`crates/ui-egui/src/tool_settings.rs`, see
    [behavior/effects.md](behavior/effects.md#last-settings-and-defaults)). When one of these is built,
    add its settings to `serialize_tools`/`parse_tools` (Fireworks MX's preference keys, as reference
    for what it keeps; values from the oracle prefix's `Fireworks MX Preferences.txt`):
    - **Blur and Sharpen tools** (item 4): size, edge, shape, intensity, blend mode, pressure options
      (`BlurTool*`, `SharpenTool*`: `Diameter`, `SoftEdge`, `IsCircle`, `StampSpacing`, `Intensity`,
      `BlendMode`, `PressureAffectsSize`, `PressureAffectsIntensity`)
    - **Dodge and Burn tools** (item 4): size, edge, shape, exposure, range, pressure options
      (`DodgeTool*`, `BurnTool*`: `Diameter`, `SoftEdge`, `IsCircle`, `StampSpacing`, `Exposure`,
      `Range`, `PressureAffectsSize`, `PressureAffectsExposure`)
    - **Smudge tool** (item 4): size, edge, shape, pressure, blend mode, Use Entire Document, Smudge
      Color and its colour, pressure options (`SmudgeTool*`: `Diameter`, `SoftEdge`, `IsCircle`,
      `StampSpacing`, `Pressure`, `BlendMode`, `UseEntireDocument`, `FingerPaint`, `FingerPaintColor`,
      `PressureAffectsSize`, `PressureAffectsPressure`)
    - **Rubber Stamp** (item 4): source aligned/fixed, sample layer, size, edge, spacing (`StampToolMode`,
      `StampToolSource`, `StampToolSize`, `StampToolEdge`, `StampToolStampSpacing`)
    - **Eraser's spacing and pressure options**, once the Eraser has them (`EraserToolStampSpacing`,
      `EraserToolPressureAffectsSize`, `EraserToolPressureAffectsOpacity`)
    - **Paint Bucket and Gradient tool's own settings**, once they're separate as in Fireworks MX
      (`FillTool*`/`GradientTool*`: `FillsSelection`, `Opacity`, `BlendMode`, `PreserveAlpha`,
      `Tolerance`); today both share the tool options' paint settings
    - **Polygon/Star tool** (item 1): sides, polygon or star, automatic angle, spikiness
      (`PolygonToolSides`, `PolygonToolIsStar`, `PolygonToolIsAuto`, `PolygonToolSpikiness`)
    - **Freeform tool** (item 1): Push/Pull size and pressure, Reshape Area size, strength and
      pressure, preview (`FreeFormPushNPull*`, `FreeFormGrav*`)
    - **Lasso and Polygon Lasso** (item 3): edge and feather, kept apart from the Marquee's
      (`LassoToolAntiAliased`, `LassoToolFeatherAmount`)
    - **Crop tool's constraint**, once it has one (`CropTool_Constraint`, `CropTool_Horiz`,
      `CropTool_Vert`)
    - **Modify > Alter Path and Select menu amounts** (item 1 and later): Simplify, Expand Stroke,
      Inset Path, Expand/Contract/Border/Smooth Marquee (`SimplifyAmount`, `Expand*`, `Inset*`,
      `ExpandSelectionAmount`, `ContractSelectionAmount`, `BorderSelectionAmount`,
      `SmoothSelectionAmount`)
13. JavaScript API compatibility for `.jsf` commands (Commands menu)
14. **Opening and importing other applications' files**, in this order:
    1. **Photoshop (PSD)**, highest priority: layers as bitmap objects with names, visibility,
       opacity and blend modes; layer groups; text layers as editable text where possible
       (otherwise their pixels); masks. Measure what Fireworks MX makes of a PSD with the oracle
       first.
    2. **Illustrator (AI)**: paths, fills and strokes from PDF-based `.ai` files (and the older
       PostScript ones), as Fireworks' vector import does.
    3. **FreeHand (FH)**: FreeHand documents' paths, text and layers (FreeHand 8-11; Fireworks MX
       imports them).
    These open as documents and also come in through **File > Import** (not built yet; placing an
    image on the canvas with a click or drag, as in Fireworks).
15. Adopt more `craft-libs` crates (craft-ui docking/tabs, craft-keymap, craft-settings) for suite
    consistency; GPU canvas
17. **Bevel Boss profile editor**: the Live Effects dialog for Eye Candy's Bevel Boss edits every
    setting but the bevel profile (`BEvl`, `Bevel: n/x y f/...`) and Darken Deep Areas (`Drkn`).
    Add a curve editor like Eye Candy's (draggable points, add/remove, its preset profiles) with a
    live preview, each point toggling sharp/smooth (the third number per point, 1 = sharp). How
    profiles are drawn is measured (pieces split at sharp points, the polynomial through each,
    clamped; see [behavior/plugins.md](behavior/plugins.md))
18. **Exact fill edges in Precise colour math** (the user's idea, 2026-11-24): Fireworks MX samples
    fills 4 x 4 (edges move in quarter pixels; [behavior/paths.md](behavior/paths.md#fill-anti-aliasing)),
    so a rectangle 20.1 wide gets no partial pixel although W shows 21. In Precise documents, compute
    each pixel's exact covered area instead, rounded once (20.1 wide: a 10% edge, alpha 26; 20.5: 128):
    the same model as MX's sampling without its quantization, as Precise does for effects. Classic
    documents (all files opened from Fireworks) keep the bit-exact 4 x 4 rasterizer, so parity doesn't
    move.
    - An area-accumulation rasterizer (about as fast as 4 x 4 sampling) with even-odd and non-zero
      rules, used by `stroke_fw::fill_coverage` when the document is Precise: plain and feathered
      fills, and the Inside/Outside stroke clip (so a stroke's clip meets the fill's exact edge).
    - Overlapping contours: where edges of two contours cross the same pixel, the average winding
      can't tell where each lies (A on the left half, B on the top half: even-odd should be 50%,
      averaging gives 0%; non-zero 75% vs 100%). Detect those pixels and sample them densely
      (16 x 16) instead. MX's per-contour byte XOR for even-odd paths stays Classic-only.
    - Not changed: stroke stamping (Fireworks' brush engine places tips in 1/16-pixel steps) and
      hard edges (one sample at each pixel's centre).
    - Note: Fireworks MX redraws vector paths itself, so a Precise document's soft edges look like
      its 4 x 4 ones when opened there (as Precise effects already differ slightly).
    - Tests: exact-area values (20.1 -> 26, 20.5 -> 128, a diagonal edge against its analytic area,
      the overlap pixels above) and Classic renders unchanged (MX probes and parity).
19. **Tool cursors for the tools not built yet** (the built tools have theirs: lookalikes drawn from
    shapes in `crates/ui-egui/src/cursors.rs`, never Fireworks' artwork). Seen in Fireworks MX
    over the canvas, 2026-11-26; give each tool its cursor when it's built:
    - **Lasso and Polygon Lasso** (item 3): a lasso loop; Shift/Alt/Shift+Alt add the marquee's
      + / − / × marks (`Badge`), and over the selection the move-selection cursor, as the Marquee
    - **Blur, Sharpen, Dodge, Burn, Smudge** (item 4): no pointer, the tip's outline in the
      selection blue, as the Brush and Eraser (`tip_outline` in canvas.rs)
    - **Rubber Stamp** (item 4): a small circled target with ticks (where the source is taken
      from); while painting, the tip's outline
    - **Freeform and Reshape Area** (item 1): an arrow with a small ring beside it (and the tool's
      size circle while it pushes or pulls)
    - **Knife** (item 1): a "not allowed" sign (a circle with a bar) when no path is selected; a
      blade over a selected path
    - **Slice and Hotspot tools** (item 9): the marquee's thin cross (`Cur::Marquee(Badge::None)`)
    - **Polygon/Star tool** (item 1): the thin cross, as the Rectangle and Ellipse
    - Modifier behaviours behind some of MX's cursors, not built:
      - **Ctrl: a temporary tool while held**, which one depending on the tool, and only for the
        tools that aren't selection tools (the user, 2026-11-26: the Crop tool, for one, has
        none). The Pen's is the Subselection tool (FireCraft has this already); the Brush's is
        the Pointer. Seen with Ctrl in Fireworks MX (black Pointer arrow): Brush, Pencil,
        Eraser, Paint Bucket, Eyedropper, Line, Rectangle, Text, Hand, Zoom. Measure the rest
        (each tool, which temporary tool, and whether it acts like it: selecting, moving) before
        building it.
      - **Alt** with the Brush, Pencil, Paint Bucket, Gradient and Line tools shows the eyedropper
        and picks the colour under it.
      Build the behaviour first, then show its cursor.
20. **File > Save a Copy...**: save the document to another Fireworks PNG while it stays as it is
    (same file name, unsaved changes still marked, Save still going to its own file). Measure
    Fireworks MX first: the name it suggests, whether the copy joins Open Recent, and what it does
    when the copy is given the document's own file name. On the desktop through the save dialog,
    in the browser as a download or the file picker, like Save As.
21. **Ctrl for sub-pixel Pointer drags** (the user's request, 2026-11-28): with the Pointer
    tool itself, Ctrl during a drag moves by the exact offset instead of whole pixels (live and
    on release, `canvas.rs`), as Ctrl does with the Scale, Skew and Distort tools; Ctrl+Alt+drag
    still duplicates, sub-pixel. Not for a temporary Pointer (Ctrl held with another tool, item
    19), whose Ctrl is taken. Ctrl's temporary tool is chosen when the drag starts, so Ctrl
    pressed mid-drag while adding a rectangle or ellipse stays sub-pixel.
22. **Line tool on whole pixels, Ctrl for sub-pixel** (the user's request, 2026-11-28): the Line
    tool never snaps (`shape_points` and `add_line` in canvas.rs take the exact pointer position),
    unlike the Rectangle and Ellipse tools. Measure Fireworks MX first: where its Line tool puts
    the endpoints (pixel centres, grid lines or unsnapped) and whether that depends on the stroke
    (a 1-pixel line is crisp along pixel centres, not grid lines). Snap the same way, show the
    snapped line while dragging, and let Ctrl mid-drag draw exactly at the pointer, as with the
    Rectangle and Ellipse tools; document it in [behavior/paths.md](behavior/paths.md).

## When needed

- Keep FireCraft-only document settings (Precise colour math, pixel-correct perspective, WebP/JPEG XL
  export settings) through a Fireworks MX re-save, which drops them today: a disabled Hue/Saturation
  effect on a hidden carrier object survives (tested; recipe and rules in
  [fireworks-png.md](fireworks-png.md#data-that-survives-a-fireworks-mx-re-save-tested-2026-11-10)).

## Lowest priority

- Customizable panels as in Fireworks MX: drag panels out to float, regroup them (the panel
  options menu's "Group ... with", "Rename panel group"), and keep the layout between sessions.
  Today the dock's groups are fixed (Align is a tab of "Optimize and Align", as in a fresh MX).
- Infinite Symbol Editor canvas (as in later Fireworks): render only the visible region. For
  now the editor's canvas has room all around the symbol (at least its own size), recomputed
  each time the symbol is opened.
- Paste position from FireCraft into Fireworks MX: Fireworks ignores the position stored in the
  "PNG" clipboard data (its own included) and keeps it only from its private "Fireworks Internal
  Clipboard Format 3.0" (served through OLE). Decoding that format (dump it Windows-side and analyse
  it) would let FireCraft write it too. See [behavior/clipboard.md](behavior/clipboard.md).
- Edit > Paste into a new document: File > New is sized from the clipboard (done 2026-11-16).
  Fireworks MX's "resample the pasted image?" prompt isn't there: FireCraft never resamples.
- "Send to Photoshop / Illustrator / Flash / Dreamweaver", Launch and Edit, FrontPage roundtrip:
  menu entries may exist but integrations are out of scope.
