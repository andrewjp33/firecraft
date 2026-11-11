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
- Text tool: click or drag to make text, in-place editing, per-character formatting, Property inspector,
  Missing Fonts, Fonts folder, saving text Fireworks MX opens ([behavior/text.md](behavior/text.md))
- Fills: Fill Options, Edit Gradient (stops, opacity, presets), Web Dither, Pattern, handles on the
  canvas, Gradient tool, Hard/Anti-Alias/Feather edges ([behavior/gradients.md](behavior/gradients.md))
- Objects: clipboard (shared with Fireworks), Duplicate/Clone, Group/Ungroup, Arrange, Align,
  Rotate/Flip, Numeric Transform, names, per-object show/hide and lock, Layers panel reordering,
  Superselect/Subselect, live dragging
- Live Effects editor in the Property inspector
- Symbols: Convert to Symbol, Library, Symbol Editor, Break Apart, 9-slice symbols (`NGSv`)
- Clean-room oracle tooling and UI test helpers (`tools/fwmx-oracle`)
- Optimize panel, Preview and File > Export: GIF, animated GIF, JPEG, PNG 8/24/32 with Fireworks MX's
  palettes and settings (stored in `EXPv` as Fireworks MX does), plus WebP and JPEG XL with Quality and
  Alpha Quality; Auto Crop/Auto Difference; exact alpha with the Exact palette; lossless PNG optimizer (OptiPNG-style
  levels) ([behavior/export.md](behavior/export.md))
- Opening flat images as Fireworks MX does: PNG, JPEG, GIF (animated GIFs as frames), BMP, TIFF,
  Targa, plus WebP, JPEG XL and animated PNG ([behavior/opening-files.md](behavior/opening-files.md))

## Next

Rendering parity on real files is tracked in [parity.md](parity.md).

Other items keep their numbers.

1. **Paths, stage 2**: Modify > Combine Path (Join, Split, Union, Intersect, Punch, Crop), Alter
   Path (Simplify, Expand Stroke, Inset Path), Freeform and Reshape Area, Knife, Redraw Path,
   Polygon/Star tool
2. **Group members, the rest**: Group, Ungroup, Break Apart and Convert to Symbol for members
   selected with the Subselection tool (they do nothing yet); Layers panel rows for members
3. **Crop tool**; Lasso and Polygon Lasso
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
   dither pattern; animated WebP export; WebP and the PNG optimizer in the browser build (libwebp and
   libdeflate need a C-to-wasm compiler); export in the background (the PNG optimizer's level 7 can
   take a while on large images, and the app waits for it)
   - **16-bit colour (PNG 48/64 export)**: layers, effects and the renderer work in 8 bits per
     channel today (Precise colour math computes in `f64` but stores 8 bits between steps), so a
     16-bit export would only hold 8-bit values. Needs 16-bit (or float) images through the
     pipeline, 16-bit PNG/TIFF/PSD import, and a document setting
8. **Frames**: adding/removing/timing frames, onion skinning, animated GIF export; Tween
   Instances; button symbol states
9. **Web layer**: slices, hotspots, HTML export, rollovers (Behaviors panel)
10. Styles and URL panels; an opt-in **Fireworks CS6** save mode (reference: FirePixel's fp-fwpng)
11. Filters menu (Adjust Color, Blur, Sharpen)
12. **Remember last-used values across restarts**: Fireworks keeps the last used colours, stroke settings
    and even effect slider values as preferences, and shows them next time, after an app restart too
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

## When needed

- Keep FireCraft-only document settings (Precise colour math, pixel-correct perspective, WebP/JPEG XL
  export settings) through a Fireworks MX re-save, which drops them today: a disabled Hue/Saturation
  effect on a hidden carrier object survives (tested; recipe and rules in
  [fireworks-png.md](fireworks-png.md#data-that-survives-a-fireworks-mx-re-save-tested-2026-11-10)).

## Lowest priority

- Infinite Symbol Editor canvas (as in later Fireworks): render only the visible region. For
  now the editor's canvas has room all around the symbol (at least its own size), recomputed
  each time the symbol is opened.
- Paste position from FireCraft into Fireworks MX: Fireworks ignores the position stored in the
  "PNG" clipboard data (its own included) and keeps it only from its private "Fireworks Internal
  Clipboard Format 3.0" (served through OLE). Decoding that format (dump it Windows-side, then
  REA) would let FireCraft write it too. See [behavior/clipboard.md](behavior/clipboard.md).
- Edit > Paste into a new document (File > New sized from the clipboard, as Fireworks does).
- "Send to Photoshop / Illustrator / Flash / Dreamweaver", Launch and Edit, FrontPage roundtrip:
  menu entries may exist but integrations are out of scope.
