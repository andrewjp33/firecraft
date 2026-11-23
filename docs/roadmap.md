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
  Skew / Distort / rotate with live preview, 9-slice Scaling tool, Marquee / Oval Marquee, Magic
  Wand (Classic + Smooth anti-alias), Line, Pen, Rectangle (primitives with roundness), Ellipse,
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
  levels) ([behavior/export.md](behavior/export.md))
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
7. **Optimize panel + Export, the rest**: 2-Up/4-Up and Export Preview; GIF Loss (lossy LZW); custom
   palettes and the Color Table panel; Selective JPEG quality; Fireworks MX's own adaptive palette and
   dither pattern; more dither types, including ones that stay stable from frame to frame in
   animations (with "Maximize colors per frame" each frame has its own table); animated WebP export; WebP and the PNG optimizer in the browser build (libwebp and
   libdeflate need a C-to-wasm compiler); export in the background (the PNG optimizer's level 7 can
   take a while on large images, and the app waits for it)
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
    - **Marquee's Style** (Normal / Fixed Ratio / Fixed Size and their numbers), once built
      (`MarqueeToolMode`, `MarqueeToolModeRatioX/Y`, `MarqueeToolModeWidthX/Y`)
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
