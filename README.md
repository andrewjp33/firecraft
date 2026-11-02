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

FireCraft opens, edits and saves real Fireworks documents. Working today:

- **Fireworks PNG** read and write, verified against real Fireworks MX (and Fireworks CS5 by
  hand): re-saved Fireworks files and new documents open in Fireworks MX with their layers,
  frames, bitmaps, paths, groups, masks, symbols and effects
  ([docs/fireworks-png.md](docs/fireworks-png.md)). Anything FireCraft doesn't model yet (text
  editing, slices, presets) is carried through a save untouched.
- **Rendering** of Fireworks documents: blend modes, all 11 MX gradient types, masks, symbol
  instances (with perspective), animation frames, and the Live Effects (Gaussian/Motion/Radial
  Blur, Drop Shadow, Glow, Inner Glow, Bevels and Embosses, Hue/Saturation, Levels, Curves and
  more, plus built-in versions of the bundled Eye Candy 4000 LE / Splat LE filters). Scores
  against Fireworks' own renders are tracked in [docs/parity.md](docs/parity.md).
- **Workspace** in the style of Fireworks MX: menus, sectioned Tools panel with press-and-hold
  flyouts, Property inspector, Layers, Frames/History, Library; document windows (move,
  resize from any edge, minimise/maximise, Cascade/Tile) or **documents in tabs**.
- **Tools**: Pointer and Select Behind, **Subselection** (points of paths, members inside
  groups), Scale, Skew, Distort and rotate (live preview), the **9-slice Scaling tool** from
  Fireworks CS5, Marquee and Oval Marquee, **Magic Wand**, Line, **Pen**, Rectangle (Fireworks
  rectangle primitives with roundness), Ellipse, Eyedropper, Hand, Zoom
  ([docs/behavior/paths.md](docs/behavior/paths.md)).
- **Objects**: Cut/Copy/Paste, including **between FireCraft and Fireworks** (Windows, and
  Linux through Wine), Duplicate/Clone, Group/Ungroup, Arrange, Align, Rotate/Flip, Numeric
  Transform, names, per-object show/hide and **lock** (from Fireworks CS); Layers panel
  reordering by drag; Select > Superselect/Subselect
  ([docs/behavior/objects.md](docs/behavior/objects.md)).
- **Live Effects editor** in the Property inspector, with live preview, OK/Cancel and fine
  sliders ([docs/behavior/effects.md](docs/behavior/effects.md)).
- **Symbols**: Convert to Symbol, Library panel, Symbol Editor (live updates, Done button),
  Break Apart, and **9-slice symbols** from Fireworks CS that keep their slices under any
  transform ([docs/behavior/symbols.md](docs/behavior/symbols.md)).
- Edit > Insert > Bitmap via Cut/Copy, Select All/Deselect/Inverse/Feather, Canvas Size/Colour,
  layers (new, delete, show/hide, lock), undo/redo (100 steps by default, up to 999 in
  Preferences) with a History panel, animation playback, PNG export, drag-and-drop open.
- Preferences: live dragging ("Show objects while moving them", off for slower PCs), tool
  cursor style, Fireworks-exact or high-precision colour math, and more.

### The Magic Wand anti-alias fix

In Fireworks MX the Magic Wand's *Anti-alias* edge is tied to *Tolerance*: on a hard colour edge
the edge pixels are selected at exactly `tolerance / 255`, so low tolerances look identical to
*Hard* and only values around 100 show an edge, by which point the selection itself starts
leaking. This was measured in Fireworks MX itself
([docs/behavior/magic-wand.md](docs/behavior/magic-wand.md)); it is not a modern-PC
incompatibility. FireCraft's default **Smooth** anti-alias computes real edge coverage instead,
independent of tolerance. **Classic** reproduces Fireworks MX exactly (pick it in the Property
inspector or Preferences).

See [docs/roadmap.md](docs/roadmap.md) for what's next.

### File compatibility

Files are saved **Fireworks MX compatible** by default (they also open in later versions).
Features MX doesn't have are baked into ordinary objects or stored in data MX safely ignores; an
opt-in Fireworks CS6 save mode is planned for documents that need CS6 to understand them
([docs/fireworks-png.md](docs/fireworks-png.md#compatibility-levels-design)). Some pieces are
adapted from [FirePixel](https://github.com/Zignixx/firepixel) (Apache-2.0); see
[ATTRIBUTION.md](ATTRIBUTION.md).

## Build and run

Requires Rust 1.95+ (edition 2024). The Makefile wraps the common commands (`make help`):

```sh
make              # Linux + Windows debug builds -> dist/linux/firecraft, dist/windows/firecraft.exe
make build        # Linux debug build only
make run FILE=x.png
make windows      # Windows .exe only (needs: sudo apt install mingw-w64)
make test         # all tests
make parity       # compare renders with Fireworks for ../ref_fw_images (REF_DIR=...)
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
