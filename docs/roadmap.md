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
  Eyedropper, Hand, Zoom
- Objects: clipboard (shared with Fireworks), Duplicate/Clone, Group/Ungroup, Arrange, Align,
  Rotate/Flip, Numeric Transform, names, per-object show/hide and lock, Layers panel reordering,
  Superselect/Subselect, live dragging
- Live Effects editor in the Property inspector
- Symbols: Convert to Symbol, Library, Symbol Editor, Break Apart, 9-slice symbols (`NGSv`)
- Clean-room oracle tooling and UI test helpers (`tools/fwmx-oracle`)

## Next

Rendering parity on real files is tracked in [parity.md](parity.md).

1. **Paths, stage 2**: Modify > Combine Path (Join, Split, Union, Intersect, Punch, Crop), Alter
   Path (Simplify, Expand Stroke, Inset Path), Freeform and Reshape Area, Knife, Redraw Path,
   Polygon/Star tool
2. **Group members, the rest**: Group, Ungroup, Break Apart and Convert to Symbol for members
   selected with the Subselection tool (they do nothing yet); Layers panel rows for members
3. **Crop tool**; Lasso and Polygon Lasso
4. **Brush, Pencil, Eraser, Paint Bucket**, Blur/Sharpen/Dodge/Burn/Smudge, Rubber Stamp
   (strokes already draw with Fireworks' brush engine and textures:
   [behavior/strokes.md](behavior/strokes.md)); write `PCLv` for new paths so random brushes
   scatter the same in Fireworks
5. **Text** tool and engine (render instead of using the cached bitmap; keep `TXTv` round trip)
6. **Fills and strokes in the Property inspector**: gradient editing, textures, edge
   and stroke settings (`FPLv`/`GLDv`/`TXFv`/`BPLv`)
7. **Optimize panel + Export**: GIF/JPEG/PNG-8 with palettes, 2-Up/4-Up previews
8. **Frames**: adding/removing/timing frames, onion skinning, animated GIF export; Tween
   Instances; button symbol states
9. **Web layer**: slices, hotspots, HTML export, rollovers (Behaviors panel)
10. Styles and URL panels; an opt-in **Fireworks CS6** save mode (reference: FirePixel's fp-fwpng)
11. Filters menu (Adjust Color, Blur, Sharpen)
12. **Pattern fills**: the Fill pop-up's Pattern category, read from the Patterns folder set in
    Preferences (already stored, nothing reads it yet); needs a JPEG decoder, as 43 of MX's 46
    patterns are JPG
13. JavaScript API compatibility for `.jsf` commands (Commands menu)
14. Adopt more `craft-libs` crates (craft-ui docking/tabs, craft-keymap, craft-settings) for suite
    consistency; GPU canvas

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
