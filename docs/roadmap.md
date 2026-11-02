# Roadmap

Priorities follow Fireworks MX's everyday workflow. Each item is "done" when it matches Fireworks MX
(measured with the oracle where possible) and is covered by tests.

## Done (v0.1 foundation)

- Fireworks PNG container, lossless tree, tiles; open/save verified in Fireworks MX
- Workspace: menus, Tools, Property inspector, Layers, Frames/History, windows or tabs
- Pointer, Marquee/Oval Marquee, Magic Wand (Classic + Smooth anti-alias), Line, Rectangle,
  Ellipse, Eyedropper, Hand, Zoom; Clear, Bitmap via Cut/Copy, undo/redo
- 9-slice Scaling tool (CS5 behaviour, MX-compatible output)

## Next

Rendering parity on real files is tracked in [parity.md](parity.md); its feature table ranks
items 5 and 6 (live effects, gradient fills) and blend modes highly.


1. **Clipboard** (Cut/Copy/Paste objects and pixels), Duplicate/Clone, Group/Ungroup, Arrange
2. **Pen and Subselection** tools; editing bezier points; Scale/Crop tools; Transform
3. **Brush, Pencil, Eraser, Paint Bucket** (Fireworks' brush presets: `BPLv` fields)
4. **Text** tool and engine (render instead of using the cached bitmap; keep `TXTv` round trip)
5. **Live Effects** in the Property inspector (drop shadow, bevels, glow, blur; `EFLv` parameters
   are already preserved) and blend modes
6. **Fills**: gradients, patterns, textures (`FPLv`/`GLDv`/`TXFv`)
7. **Optimize panel + Export**: GIF/JPEG/PNG-8 with palettes, 2-Up/4-Up previews
8. **Frames**: animation, onion skinning, animated GIF export
9. **Web layer**: slices, hotspots, HTML export, rollovers (Behaviors panel)
10. Symbols/instances, Library, Styles, URL panels; then persistent 9-slice symbols (`NGSv`)
    and an opt-in **Fireworks CS6** save mode (reference: FirePixel's fp-fwpng)
11. Filters menu (Adjust Color, Blur, Sharpen)
12. JavaScript API compatibility for `.jsf` commands (Commands menu)
13. Adopt more `craft-libs` crates (craft-ui docking/tabs, craft-keymap, craft-settings) for suite
    consistency; GPU canvas
14. Stroke rendering parity with Fireworks' brush stamping (1-px strokes render heavier today)

## Lowest priority

- Paste position from FireCraft into Fireworks MX: Fireworks ignores the position stored in the
  "PNG" clipboard data (its own included) and keeps it only from its private "Fireworks Internal
  Clipboard Format 3.0" (served through OLE). Decoding that format (dump it Windows-side, then
  REA) would let FireCraft write it too. See docs/behavior/clipboard.md.
- Edit > Paste into a new document (File > New sized from the clipboard, as Fireworks does).

- "Send to Photoshop / Illustrator / Flash / Dreamweaver", Launch and Edit, FrontPage roundtrip:
  menu entries may exist but integrations are out of scope.
