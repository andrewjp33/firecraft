# Export and the Optimize panel (Fireworks MX behaviour, measured)

File > Export writes the document with its export settings, which the Optimize panel edits and the
Preview tab shows (the exported file decoded back, with its size). Code: `crates/doc/src/export.rs`
(settings), `crates/io/src/export/` (encoders, the `EXPv` record), `crates/raster/src/quantize.rs`
(palettes), `crates/ui-egui/src/optimize.rs` (panel).

## Formats

| Format | Encoder | Options |
| --- | --- | --- |
| GIF, Animated GIF | `gif` crate | palette, colours, dither, transparency, interlaced; loops |
| PNG 8 | `png` crate (1/2/4/8-bit by palette size) | palette, colours, dither, transparency (alpha: a palette with alpha) |
| PNG 24, PNG 32 | `png` crate | matte (PNG 24) |
| JPEG | `jpeg-encoder` | quality (Fireworks MX's scale), smoothing, progressive, full-resolution colour |
| WebP (FireCraft) | libwebp via the `webp` crate; desktop only | quality, alpha quality, lossless |
| JPEG XL (FireCraft) | `jixel` (pure Rust, from libjxl-tiny) | quality, alpha quality (alpha distance), lossless |

The AGPL encoders `zenwebp` and `jxl-encoder` were considered and left out (FireCraft is MIT or
Apache-2.0). The browser build has no C compiler for libwebp, so WebP export is desktop-only for now.

## Stored in the document: `MKBv > EXPv`

Measured by setting each option through `dom.setExportOptions` in Fireworks MX, saving, and
comparing the records:

| Field | Meaning |
| --- | --- |
| `FMTi` | 0 GIF, 1 JPEG, 2 PNG, 4 GIF animation |
| `CLRi` | 0 indexed, 1 24 bit, 2 32 bit (PNG 8/24/32) |
| `PLTi` | 0 custom, 1 adaptive, 2 grid (Uniform), 3 monochrome (Grayscale), 4 Macintosh, 5 Windows, 6 exact, 7 Web 216 |
| `PALv` | `FCLi` entries of an exact/custom palette (`PLNi` of them) |
| `PLRi` | colours requested (`numEntriesRequested`) |
| `PTNi` | transparency: 0 none, 1 index, 2 index alpha, 3 rgba (PNG 8 alpha) |
| `APSb`, `ASTi` | WebSnap Adaptive and its tolerance (14) |
| `PLDi` | dither percent (diffusion when non-zero) |
| `GFIb` | interlaced GIF |
| `GLTi` | GIF loss (not implemented) |
| `JPQi`, `JPSi`, `JPPb` | JPEG quality, smoothing, progressive |
| `JPOi` | JPEG sub-sampling option (`jpegSubsampling`; meaning unmeasured) |
| `FLPi` | animation repeats (0 forever) |
| `FNLv` | frames: `FNMv{FNPi, FNDi (delay, 1/100 s), VISb (export)}` |

FireCraft adds `FCFi` (1 WebP, 2 JPEG XL; `FMTi`/`CLRi` then say PNG 32, which Fireworks MX
exports instead), `FCQi`/`FCAi` (quality, alpha quality), `FCLb` (lossless), `FCJb` (full-resolution
JPEG colour), `FCMi` (matte). A record read from a file is kept exactly while its settings are
unchanged; a new document gets Fireworks MX's default record (now with its frame delays, which new
documents didn't carry to Fireworks MX before). Fireworks MX reads FireCraft's records back as set
(checked for GIF, JPEG, PNG 8 with alpha, animated GIF with the Windows palette, WebP).

## Defaults (Fireworks MX's `dom.exportOptions`)

* New document: GIF, adaptive with WebSnap (tolerance 14), 128 colours, no dither, no
  transparency, JPEG quality 80.
* Opened JPEG: JPEG, at the quality its tables imply on Fireworks MX's scale (below).
* Opened GIF: GIF (GIF animation when it has frames), exact palette.
* FireCraft: opened PNGs export as PNG 8/24/32 by their colour type, WebP and JPEG XL as themselves.

## Palettes

* **Web 216** and **Uniform** (grid; 256 colours gives the same 216): FireCraft's GIF of a 64-level
  RGB cube equals Fireworks MX's pixel for pixel, at the same file size.
* **Windows**, **Macintosh**: Fireworks MX's 256-entry tables, read from GIFs it exported of that
  cube (`crates/raster/src/palettes/`). With fewer colours requested, Fireworks MX keeps the entries
  the image uses most. Mapping to the nearest entry by plain RGB distance matches 99.4-99.7% of
  Fireworks MX's pixels (weighted distances fit worse).
* **Grayscale** (`monochrome`): n greys, entry i = round(255·(n−1−i)/(n−1)); colours map by
  Rec. 709 luma, truncated: 97.3% of the cube exact, the rest within one level.
* **Black & White**: Grayscale with 2 colours.
* **Adaptive**: a weighted median cut, not Fireworks MX's own algorithm; its error on a hue/lightness
  sweep at 128 colours equals Fireworks MX's (RMSE 0.0509 for both). **WebSnap Adaptive** snaps
  palette colours within the tolerance of a Web 216 colour; on that sweep slightly worse than
  Fireworks MX (0.0526 vs 0.0506).
* **Exact**: the image's colours when there are few enough, otherwise adaptive.
* Dither: Floyd-Steinberg at the dither percentage (Fireworks MX's diffusion pattern unmeasured).

## JPEG

Fireworks MX's quality setting isn't libjpeg's: its JPEGs (exported at 0-100 and their tables read
back) match libjpeg quality 8, 9, 11, 17, 40, 51, 64, 74, 86, 94, 96, 100 at settings 0, 10, ..., 90,
95, 100. FireCraft encodes with the libjpeg quality for the setting (interpolated), and reads an
opened JPEG's quality back on Fireworks MX's scale (a libjpeg-92 file: 88; Fireworks MX shows 86).
Fireworks MX writes JPEGs without chroma subsampling (1×1), FireCraft too by default. Smoothing
is approximated with a blur of 0.5 px per level.

## Not yet

2-Up/4-Up and Export Preview; GIF loss (lossy LZW); custom palettes and the Color Table panel;
Selective JPEG quality; animated WebP export; WebP in the browser build; 16-bit (PNG 48/64) export,
which needs a 16-bit pipeline (layers are 8 bits per channel).
