# FireCraft

FireCraft is an open-source web and screen graphics editor in the spirit of **Macromedia Fireworks MX**:
bitmap and vector editing in one document, a context-sensitive Property inspector, layers, frames and
the Fireworks PNG file format. It is written in pure Rust with egui, the same stack as the
[Craft apps](https://getartcraft.com/apps) (PhotoCraft, VectorCraft, ...), and runs natively on Windows
and Linux (macOS should work too) and in the browser via WebAssembly.

> FireCraft is an independent, clean-room project. It contains no Macromedia/Adobe code or artwork.
> Fireworks is a trademark of Adobe. Behaviour is learned from documentation and by measuring the
> original program (see [docs/clean-room.md](docs/clean-room.md)).

## Status (November 2026)

FireCraft opens, edits and saves real Fireworks documents, and draws most of them very close to how
Fireworks MX does. It is in active development. Vector and bitmap editing, text, fills, strokes, effects and
symbols are in place. Web export (Optimize, GIF/JPEG), frame editing and slices are not yet
([what's next](#not-done-yet)).

**Rendering parity** on 87 reference files, scored against Fireworks' own render of each file
(10 November 2026): 37 match 100%, 13 more are above 99%, 20 are between 90% and 99%, and 17 are
below 90%. The average is 93.8%. Every file survives the lossless round trip and a FireCraft save
that re-opens to the same render. Details: [docs/parity.md](docs/parity.md).

### Recently added

- **Text**: the Text tool and the whole Text menu except spelling.
  - Fonts, size, bold/italic/underline, alignment, kerning, leading, the Text Editor.
  - Text on a path (Attach, Detach, Orientation, Reverse Direction).
  - Vertical and right-to-left text.
  - Scaled, rotated, skewed and distorted text.
  - Gradient, pattern and texture fills, and strokes on text, with *Draw fill over stroke*.
  - Convert to Paths.
  - Characters are placed exactly where Fireworks MX places them (its kerning and rounding,
    measured in MX).
  - Missing fonts can keep Fireworks' saved appearance.
  - On the web, installed fonts can be used with the browser's permission.
  ([docs/behavior/text.md](docs/behavior/text.md))
- **Fills**: gradient editing with stops, opacity and presets; handles on the canvas; the
  Gradient tool; Pattern and Web Dither fills; fill textures; Hard, Anti-Alias and Feather edges
  ([docs/behavior/gradients.md](docs/behavior/gradients.md)).
- **Strokes**: Fireworks' own brush engine with its 54 presets, stroke textures, placement, the
  Edit Stroke dialog, and Fireworks CS-style stroke controls
  ([docs/behavior/strokes.md](docs/behavior/strokes.md)).
- **Painting**: Brush, Pencil, Eraser and Paint Bucket, using Fireworks' brush tips and
  sensitivities ([docs/behavior/painting.md](docs/behavior/painting.md)).
- **Pen tool** with Fireworks MX's curve drawing and point editing; Subselection improvements.
- **Perspective symbols**: a per-document choice between drawing them as Fireworks MX does or
  pixel-correct ([docs/behavior/symbols.md](docs/behavior/symbols.md)).
  - Fireworks MX mode maps each object, then applies its effects at canvas size.
  - Strokes keep their width under perspective.
  - Fill textures stay anchored to the canvas, as Fireworks MX draws them.
- **Effects accuracy**: Gaussian Blur computed as Fireworks computes it, Fireworks'
  triangle-filter softness for shadows and glows, and smooth bevel edges.
- **Tools panel** colour boxes follow the selection, text included; tool keys for Paint
  Bucket/Gradient (G), Eraser (E) and Pencil (Y). Fields with mixed values across the selection
  show blank, as in Fireworks MX.
- **History**: every drag is one undo step, recorded on mouse up, as in Fireworks MX.
- **Web**: real file pickers for Open, Save and Export; Fonts, Textures and Patterns folders;
  Ctrl+Alt+N/W/T alternatives for shortcuts the browser keeps for itself.

### Working today

- **Fireworks PNG** read and write, verified against real Fireworks MX (and Fireworks CS5 by
  hand). Re-saved Fireworks files and new documents open in Fireworks MX with their layers,
  frames, bitmaps, paths, text, groups, masks, symbols and effects
  ([docs/fireworks-png.md](docs/fireworks-png.md)). Anything FireCraft doesn't model yet
  (slices, presets) is carried through a save untouched.
- **Rendering**:
  - Blend modes and all 11 MX gradient types.
  - Masks, symbol instances (with perspective) and animation frames.
  - Text with Fireworks MX's layout.
  - Live Effects: Gaussian/Motion/Radial Blur, Drop Shadow, Glow, Inner Glow, Bevels and
    Embosses, Hue/Saturation, Levels, Curves and more.
  - Built-in versions of the bundled Eye Candy 4000 LE / Splat LE filters.
- **Workspace** in the style of Fireworks MX:
  - Menus, a sectioned Tools panel with press-and-hold flyouts, the Property inspector, Layers,
    Frames/History and the Library.
  - Document windows (move, resize from any edge, minimise/maximise, Cascade/Tile), or
    **documents in tabs**.
- **Tools** ([docs/behavior/paths.md](docs/behavior/paths.md)):
  - Selection: Pointer, Select Behind, **Subselection** (points of paths, members inside
    groups).
  - Transform: Scale, Skew, Distort and rotate (live preview), the **9-slice Scaling tool**
    from Fireworks CS5.
  - Bitmap: Marquee, Oval Marquee, Magic Wand (Fireworks MX's anti-alias, or a smoother one
    by default: [docs/behavior/magic-wand.md](docs/behavior/magic-wand.md)), Brush, Pencil,
    Eraser, Paint Bucket, Gradient.
  - Vector: Line, **Pen**, Rectangle (Fireworks rectangle primitives with roundness), Ellipse,
    **Text**.
  - Eyedropper, Hand, Zoom.
- **Objects** ([docs/behavior/objects.md](docs/behavior/objects.md)):
  - Cut/Copy/Paste, including **between FireCraft and Fireworks** (Windows, and Linux through
    Wine).
  - Duplicate/Clone, Group/Ungroup, Arrange, Align, Rotate/Flip, Numeric Transform.
  - Names, per-object show/hide and **lock** (from Fireworks CS).
  - Layers panel reordering by drag; Select > Superselect/Subselect.
- **Live Effects editor** in the Property inspector, with live preview, OK/Cancel and fine
  sliders ([docs/behavior/effects.md](docs/behavior/effects.md)).
- **Symbols** ([docs/behavior/symbols.md](docs/behavior/symbols.md)):
  - Convert to Symbol, Library panel, Break Apart.
  - Symbol Editor, nested editors included, with live updates and a Done button.
  - **9-slice symbols** from Fireworks CS that keep their slices under any transform.
- **Edit and document**:
  - Edit > Insert > Bitmap via Cut/Copy.
  - Select All, Deselect, Select Inverse, Feather.
  - Canvas Size and Canvas Colour.
  - Layers: new, delete, show/hide, lock.
  - Undo/redo (100 steps by default, up to 999 in Preferences) with a History panel.
  - Animation playback, PNG export, drag-and-drop open.
- **Preferences**:
  - Live dragging ("Show objects while moving them", off for slower PCs).
  - Tool cursor style.
  - Colour math for new documents: Precise (default) or Fireworks MX's exact formulas.
  - Perspective symbols in new documents: pixel-correct (default) or drawn as Fireworks MX does.
  - Fonts, Textures and Patterns folders.

### Not done yet

From [docs/roadmap.md](docs/roadmap.md), roughly in order:
- Combine/Alter Path, Knife, Polygon/Star.
- Crop and Lasso tools.
- Blur, Sharpen, Dodge, Burn, Smudge and Rubber Stamp.
- Optimize panel and GIF/JPEG/PNG-8 export.
- Editing frames (adding, removing, timing) and animated GIF export.
- Slices, hotspots and HTML export.
- Styles and the Filters menu.
- `.jsf` commands.
- Check Spelling, last.

### File compatibility

Every save is a **Fireworks MX file**: it opens in Fireworks MX and later versions. Features MX
doesn't have are baked into ordinary objects or stored in data MX safely ignores. An opt-in
Fireworks CS6 save mode is planned for documents that need CS6 to understand them
([docs/fireworks-png.md](docs/fireworks-png.md#compatibility-levels-design)).

New documents use FireCraft's improved drawing by default: *Precise* colour math and
*Pixel-correct* perspective symbols (both can be changed in Preferences, and per document in the
Property inspector). Files from Fireworks open in the Fireworks MX modes, so they look as they
were saved. A FireCraft file keeps its modes in data Fireworks ignores, so Fireworks MX draws it
with its own rules, which can differ slightly from FireCraft's improved modes.

Some pieces are adapted from [FirePixel](https://github.com/Zignixx/firepixel) (Apache-2.0); see
[ATTRIBUTION.md](ATTRIBUTION.md).

## Build and run

Requires Rust 1.95+ (edition 2024). The Makefile wraps the common commands (`make help`):

```sh
make              # Linux + Windows debug builds -> dist/linux/firecraft, dist/windows/firecraft.exe
make build        # Linux debug build only
make run FILE=x.png
make windows      # Windows .exe only (needs: sudo apt install mingw-w64)
make test         # all tests
make parity       # compare renders with Fireworks for ../ref_fw_images; results in ../parity_runs
make web-debug    # web build (unoptimized, with debug info) -> dist/web
make web          # web build, optimized
make clean        # remove FireCraft's own build outputs; compiled dependencies stay (fast rebuild)
make distclean    # remove everything (the next build compiles all dependencies again)
make size         # disk used by target/ and dist/
```

Finished binaries are copied to `dist/` (release builds: `dist/linux-release/`,
`dist/windows-release/`; web: `dist/web/`). Cargo's `target/` holds the compiled dependencies,
which rarely change: `make clean` keeps them, so a rebuild takes seconds rather than minutes.
With [sccache](https://github.com/mozilla/sccache) installed, the Makefile uses it
automatically and compiled dependencies survive even `make distclean`.

Or with cargo directly:

```sh
cargo run --release -p firecraft -- [file.png ...]     # desktop
cargo test                                             # all crates
```

Web build (needs [trunk](https://trunkrs.dev) and the `wasm32-unknown-unknown` target):

```sh
rustup target add wasm32-unknown-unknown
cd apps/firecraft-web && trunk serve        # http://127.0.0.1:8766
```

Windows: `cargo build -p firecraft` on Windows (MSVC toolchain), or cross-compile from Linux:

```sh
sudo apt install mingw-w64          # once
make windows                        # -> dist/windows/firecraft.exe
```

Startup: a small software-drawn splash shows (at least 2 s) while the GPU renderer starts
(`FIRECRAFT_NO_SPLASH=1` disables it). Renderer: wgpu on Vulkan/DX12/Metal; if the only such
device is a software rasterizer (e.g. VirtualBox) FireCraft uses OpenGL (glow), which can still use
the GPU. Force one with `FIRECRAFT_RENDERER=glow` or `FIRECRAFT_RENDERER=wgpu`.

## Layout

```text
crates/
  fwpng      L0  Fireworks PNG container: chunks, tagged document tree, pixel tiles (no deps)
  raster     L0  pixels, masks, magic wand, marquees, feather, vector rasterization
  stroke     L0  Fireworks' brush engine (adapted from FirePixel, Apache-2.0)
  text       L0  fonts, layout and rasterising with Fireworks MX's rules (adapted from FirePixel)
  doc        L1  document model: canvas, layers, frames, bitmap/path/text/group elements
  io         L2  Document <-> Fireworks PNG, flat PNG import/export
  engine     L3  session, tools, selection, commands, undo history
  sysclip    --  system clipboard ("PNG" format shared with Fireworks): X11 and Win32
  ui-egui    L4  the Fireworks-style workspace (egui); only talks to engine
apps/
  firecraft       desktop app (eframe)
  firecraft-web   browser app (eframe + trunk)
tools/fwmx-oracle  scripts that measure the original Fireworks MX under Wine, and UI test
                   helpers that drive FireCraft (ui_session.sh, uimap.py, xin.py)
```

## License

MIT OR Apache-2.0.
