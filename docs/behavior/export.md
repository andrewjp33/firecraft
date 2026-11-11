# Export and the Optimize panel (Fireworks MX behaviour, measured)

File > Export writes the document with its export settings, which the Optimize panel edits and the
Preview tab shows (the exported file decoded back, with its size). Code: `crates/doc/src/export.rs`
(settings), `crates/io/src/export/` (encoders, the `EXPv` record), `crates/raster/src/quantize.rs`
(palettes), `crates/ui-egui/src/optimize.rs` (panel).

## Formats

| Format | Encoder | Options |
| --- | --- | --- |
| GIF, Animated GIF | `gif` crate | palette, colors, dither, transparency, interlaced; loops, Auto Crop, Auto Difference |
| PNG 8 | `png` crate (1/2/4/8-bit by palette size, unfiltered) | palette, colors, dither, transparency, exact alpha, PNG optimizer |
| PNG 24, PNG 32 | `png` crate | matte (PNG 24), PNG optimizer |
| JPEG | `jpeg-encoder` | quality (Fireworks MX's scale), smoothing, progressive, full-resolution color |
| WebP (FireCraft) | libwebp via the `webp` crate; desktop only | quality, alpha quality, lossless |
| JPEG XL (FireCraft) | `jixel` (pure Rust, from libjxl-tiny) | quality, alpha quality (alpha distance), lossless |

The AGPL encoders `zenwebp` and `jxl-encoder` were considered and left out (FireCraft is MIT or
Apache-2.0). The browser build has no C compiler for libwebp, so WebP export is desktop-only for now.

**PNG optimizer.** File > Export can recompress PNG 8/24/32 losslessly with `oxipng` (MIT; an
OptiPNG-style optimizer linking libdeflate, so desktop only): levels 1-6 are its presets 0-5, 7 its
strongest with Zopfli. The Preview shows the plain encoding (fast); the export is smaller. On
PP-8.png, win-8.png and transparent-minions-dvd-vc-rating3.png (Fireworks MX files, 2145, 2878 and
7346 bytes) FireCraft's re-export is pixel-identical at 2294, 3096, 7906 bytes unoptimized, 2215,
2959, 7519 at level 2 (the default, as OptiPNG's), 2131, 2905, 7512 at 5 and 2098, 2900, 7360 at 7
(0.06-1.4 s).

## Stored in the document: `MKBv > EXPv`

Measured by setting each option through `dom.setExportOptions` in Fireworks MX, saving, and
comparing the records:

| Field | Meaning |
| --- | --- |
| `FMTi` | 0 GIF, 1 JPEG, 2 PNG, 4 GIF animation |
| `CLRi` | 0 indexed, 1 24 bit, 2 32 bit (PNG 8/24/32) |
| `PLTi` | 0 custom, 1 adaptive, 2 grid (Uniform), 3 monochrome (Grayscale), 4 Macintosh, 5 Windows, 6 exact, 7 Web 216 |
| `PALv` | `FCLi` entries of an exact/custom palette (`PLNi` of them) |
| `PLRi` | colors requested (`numEntriesRequested`) |
| `PTNi` | transparency: 0 none, 1 index, 2 index alpha, 3 rgba (PNG 8 alpha) |
| `APSb`, `ASTi` | WebSnap Adaptive and its tolerance (14) |
| `PLDi` | dither percent (diffusion when non-zero) |
| `GFIb` | interlaced GIF |
| `GLTi` | GIF loss (not implemented) |
| `AACb`, `AADb` | animation Auto Crop, Auto Difference (`animAutoCrop`, `animAutoDifference`; both on) |
| `JPQi`, `JPSi`, `JPPb` | JPEG quality, smoothing, progressive |
| `JPOi` | JPEG sub-sampling option (`jpegSubsampling`; meaning unmeasured) |
| `FLPi` | animation repeats (0 forever) |
| `FNLv` | frames: `FNMv{FNPi, FNDi (delay, 1/100 s), VISb (export)}` |

FireCraft adds `FCFi` (1 WebP, 2 JPEG XL; `FMTi`/`CLRi` then say PNG 32, which Fireworks MX
exports instead), `FCQi`/`FCAi` (quality, alpha quality), `FCLb` (lossless), `FCJb` (full-resolution
JPEG color), `FCMi` (matte), `FCXb` (exact alpha), `FCOi` (PNG optimizer level). A record read from a file is kept exactly while its settings are
unchanged; a new document gets Fireworks MX's default record (now with its frame delays, which new
documents didn't carry to Fireworks MX before). Fireworks MX reads FireCraft's records back as set
(checked for GIF, JPEG, PNG 8 with alpha, animated GIF with the Windows palette, WebP).

## Defaults

Fireworks MX (`dom.exportOptions`): a new document exports as GIF, adaptive with WebSnap
(tolerance 14), 128 colors, no dither, no transparency, JPEG quality 80. FireCraft starts new
documents with plain Adaptive and 256 colors instead (the presets keep their own: GIF WebSnap 128
and Animated GIF WebSnap 128 are WebSnap at 128).

Opened files, as Fireworks MX opens them:
* JPEG: JPEG, at the quality its tables imply on Fireworks MX's scale (below).
* GIF: GIF (GIF animation when it has frames), exact palette, its own color count.
* PNG 8: PNG 8, exact palette, its palette's size (measured: PP-8.png 32, win-8.png 16,
  transparent-minions-dvd-vc-rating3.png 256; Fireworks MX picks Grayscale for PP-8's gray
  palette, FireCraft keeps it Exact so the file exports back unchanged).
* Fireworks MX reports no transparency for these PNG 8s although they have some; FireCraft keeps
  Alpha Transparency, with exact alpha when entries are partly transparent: all three export back
  pixel-identical.
* FireCraft: other PNGs export as PNG 24/32 by their color type, WebP and JPEG XL as themselves.

## Transparency (measured on a soft-edged image with alphas 0, 6, 12, 19...)

* **Index Transparency** is a color key: everything is flattened onto the matte (white when the
  canvas is transparent) and the matte's palette entry becomes transparent, so opaque white pixels
  vanish too.
* **Alpha Transparency**, GIF, and PNG 8 with a fixed palette (Grayscale, Web 216...): pixels
  under alpha 16 become transparent (12 did, 19 didn't); the rest are flattened onto the matte.
  The palette is n−1 colors plus a transparent entry. (FireCraft used to map the clear pixels to
  the nearest gray, black.)
* **Alpha Transparency**, PNG 8 with Adaptive, WebSnap or Exact: a palette with alpha, rounded to
  16 levels (multiples of 17: 6 → 0, 12 and 19 → 17). FireCraft's **Exact alpha** option keeps
  alpha as it is.

## Animation

Auto Crop: an opaque animation's frames cover only what changed since the previous frame
(disposal "keep"). Auto Difference: inside that, pixels equal to the previous frame are the
transparent entry (one palette entry is reserved for it). Frames with transparent pixels must
clear what was drawn before (disposal "background"): Auto Crop then keeps each frame to its visible
pixels and Auto Difference doesn't apply. Opening such a GIF back gives the same frames.

Opening GIFs: the document takes the GIF's screen size (it used to take the frames' extent, which
was too small when every frame was cropped).

## Palettes

* **Web 216** and **Uniform** (grid; 256 colors gives the same 216): FireCraft's GIF of a 64-level
  RGB cube equals Fireworks MX's pixel for pixel, at the same file size.
* **Windows**, **Macintosh**: Fireworks MX's 256-entry tables, read from GIFs it exported of that
  cube (`crates/raster/src/palettes/`). With fewer colors requested, Fireworks MX keeps the entries
  the image uses most. Mapping to the nearest entry by plain RGB distance matches 99.4-99.7% of
  Fireworks MX's pixels (weighted distances fit worse).
* **Grayscale** (`monochrome`): n grays, entry i = round(255·(n−1−i)/(n−1)); colors map by
  Rec. 709 luma, truncated: 97.3% of the cube exact, the rest within one level.
* **Black & White**: Grayscale with 2 colors.
* **Adaptive**: a weighted median cut, not Fireworks MX's own algorithm; its error on a hue/lightness
  sweep at 128 colors equals Fireworks MX's (RMSE 0.0509 for both). **WebSnap Adaptive** snaps
  palette colors within the tolerance of a Web 216 color; on that sweep slightly worse than
  Fireworks MX (0.0526 vs 0.0506).
* **Exact**: the image's colors when there are few enough, otherwise adaptive.
* Dither: Floyd-Steinberg at the dither percentage (Fireworks MX's diffusion pattern unmeasured).
* The adaptive palette starts from the colors in a fixed order, so an export is the same every
  time (it depended on a hash map's order before).
* Color mapping is by plain RGB distance (Windows/Macintosh above).

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
