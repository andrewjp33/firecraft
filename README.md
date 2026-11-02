# FireCraft

FireCraft is an open-source web and screen graphics editor in the spirit of **Macromedia Fireworks MX**:
bitmap and vector editing in one document, a context-sensitive Property inspector, layers, frames and
the Fireworks PNG file format. It is written in pure Rust with egui, the same stack as the
[Craft apps](https://getartcraft.com/apps) (PhotoCraft, VectorCraft, ...), and runs natively on Windows
and Linux (macOS should work too) and in the browser via WebAssembly.

> FireCraft is an independent, clean-room project. It contains no Macromedia/Adobe code or artwork.
> Fireworks is a trademark of Adobe. Behaviour is learned from documentation and by measuring the
> original program (see [docs/clean-room.md](docs/clean-room.md)).

## Status: early foundation (v0.1)

Working today:

- **Fireworks PNG** read and write, verified against real Fireworks MX: new documents and re-saved
  Fireworks files open in Fireworks MX with their layers, bitmaps and vector paths
  ([docs/fireworks-png.md](docs/fireworks-png.md)). Anything FireCraft doesn't model yet (effects,
  textures, symbols, slices, text, presets) is carried through a save untouched.
- Fireworks MX-style workspace: menus, sectioned Tools panel, Property inspector, Layers,
  Frames/History, document windows with Original/Preview tabs, or **documents in tabs**
  (Window > Documents in Tabs, or Edit > Preferences).
- Tools: Pointer (select, move, nudge), Marquee and Oval Marquee (hard/anti-alias/feather,
  add/subtract/intersect), **Magic Wand**, Line, Rectangle, Ellipse, Eyedropper, Hand, Zoom, and
  the **9-slice Scaling tool** from Fireworks CS5 (output stays MX compatible;
  [docs/behavior/nine-slice.md](docs/behavior/nine-slice.md)).
- Edit > Clear, Edit > Insert > Bitmap via Cut/Copy, Select All/Deselect/Inverse/Feather,
  Canvas Size/Colour, layers (new, delete, show/hide, lock), undo/redo (100 steps by default, up to
  999 in Preferences) with a History panel that moves back and forward,
  PNG export, drag-and-drop open.

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
  ui-egui    L4  the Fireworks-style workspace (egui); only talks to engine
apps/
  firecraft       desktop app (eframe)
  firecraft-web   browser app (eframe + trunk)
tools/fwmx-oracle  scripts that measure the original Fireworks MX under Wine
```

## License

MIT OR Apache-2.0.
