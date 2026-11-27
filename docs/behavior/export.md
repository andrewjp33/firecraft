# Export and the Optimize panel (Fireworks MX behaviour, measured)

File > Export writes the document with its export settings, which the Optimize panel edits and the
Preview tab shows (the exported file decoded back, with its size). Code: `crates/doc/src/export.rs`
(settings), `crates/io/src/export/` (encoders, the `EXPv` record), `crates/raster/src/quantize.rs`
(palettes), `crates/ui-egui/src/optimize.rs` (panel).

## Formats

| Format | Encoder | Options |
| --- | --- | --- |
| GIF, Animated GIF | `gif` crate | palette, colors, dither, transparency, interlaced; loops, Auto Crop, Auto Difference. The Animated GIF preview shows the current frame as the animation shows it (a frame not set to export: the exported frame before it) |
| PNG 8 | `png` crate (1/2/4/8-bit by palette size, unfiltered) | palette, colors, dither, transparency, PNG optimizer |
| PNG 24, PNG 32 | `png` crate | matte (PNG 24), PNG optimizer |
| JPEG | `jpeg-encoder` | quality (Fireworks MX's scale), smoothing, progressive, full-resolution color |
| WebP (FireCraft) | libwebp via the `webp` crate; desktop only | quality, alpha quality, lossless |
| JPEG XL (FireCraft) | `jixel` (pure Rust, from libjxl-tiny) | quality, alpha quality (alpha distance), lossless |

The AGPL encoders `zenwebp` and `jxl-encoder` were considered and left out (FireCraft is MIT or
Apache-2.0). The browser build has no C compiler for libwebp, so WebP export is desktop-only for now.

**PNG optimizer.** File > Export can recompress PNG 8/24/32 losslessly with `oxipng` (MIT; an
OptiPNG-style optimizer linking libdeflate, so desktop only): levels 1-6 are its presets 0-5, 7 its
strongest with Zopfli. The Preview shows the plain encoding (fast); the export is smaller.

Sizes on the PNG 8 references (pixel-identical in every case; the reference files themselves had
been through OptiPNG, so they're not Fireworks MX's sizes): Fireworks MX re-exporting them (Adaptive,
Alpha Transparency, which reproduces them exactly) writes 2719, 3136 and 8034 bytes for PP-8.png,
win-8.png and transparent-minions-dvd-vc-rating3.png. FireCraft writes 2294, 3096, 7906 with the
optimizer off, 2215, 2959, 7519 at level 2 (the default, as OptiPNG's) and 2098, 2900, 7360 at 7
(0.06-1.4 s); the OptiPNG'd files are 2145, 2878, 7346.

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
| `PLOb` | Remove Unused Colors (`optimized`; on) |
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
JPEG color), `FCMi` (matte), `FCOi` (PNG optimizer level), `FCXb` (Smoother alpha). A record read from a file is kept exactly while its settings are
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
  Alpha Transparency: all three export back pixel-identical. An opened GIF's color count includes
  its transparent entry, as a PNG 8's palette size does.
* FireCraft: other PNGs export as PNG 24/32 by their color type, WebP and JPEG XL as themselves.

## Matte

Fireworks MX's Matte isn't stored in the document (no `EXPv` field); its default is none. With no
matte, a pixel a format can't keep transparent is drawn **opaque in its own color**, down to alpha 1,
and only fully transparent pixels take a color (white on a transparent canvas, or the canvas color):
measured on 769_edit2.png exported by Fireworks MX as GIF (no, Index and Alpha transparency), PNG 8
with Index transparency (`769_edit2_out-8index.png`), PNG 24 and JPEG, every edge pixel in its own
color. A matte color blends the edges onto it instead (for an image going on that background).
FireCraft does the same (its PNG 8 Index export of 769_edit2.png: transparency 99.99% the same as
Fireworks MX's, every edge pixel within 16 levels). It used to treat no matte as white.

## Transparency (measured on a soft-edged image with alphas 0, 6, 12, 19...)

* **Index Transparency** is a color key: everything is made opaque as above, and the entry of the
  matte color (white when there's none and the canvas is transparent) becomes transparent, so
  opaque white pixels vanish too.
* **Alpha Transparency**, GIF: pixels under alpha 16 become transparent (12 did, 19 didn't in
  Fireworks MX); the rest are made opaque as above. The palette is n−1 colors plus a
  transparent entry.

PNG 8 with Alpha Transparency keeps alpha in its palette. Fireworks MX: Adaptive rounds alpha to
16 levels (multiples of 17: 6 → 0, 12 and 19 → 17); Exact keeps only fully clear and fully opaque
pixels (its exports of the PNG 8 references lost their soft edges); fixed palettes cut at 16 as
GIF does. FireCraft:

* **When every color+alpha combination fits the color count, alpha is kept exactly**, whatever
  the palette: each combination gets its own entry, so nothing is lost. With a fixed palette
  (Web 216, Windows, Grayscale...) the combinations are the palette colors the pixels map to,
  each with the alphas it needs.
* **When they don't fit**, Adaptive, WebSnap and Exact build a median-cut palette with alpha
  rounded to 16 levels, as Fireworks MX does: it spends fewer entries on alpha. Measured on a
  soft-shadowed image (RMS error composited over white / black): at 256 colors 3.14 / 3.03 rounded
  against 3.07 / 2.99 unrounded, at 64 colors 5.74 / 5.39 against 6.06 / 5.48, at 16 colors 11.70 /
  11.58 against 12.09 / 11.06. **Smoother alpha** keeps alpha unrounded instead (smoother soft
  edges, coarser colors). Fixed palettes try rounded alpha, then Fireworks MX's cut at 16.
* The transparent entry (and Auto Difference's) is added on top of Exact's colors, up to 256.

## Animation

A GIF frame can paint pixels or leave them as they are (its transparent entry), never turn a shown
pixel transparent again; only a frame's disposal can, by clearing its own rectangle once it has
been shown ("background"). So frame by frame:

* when the next frame turns a visible pixel transparent, this frame covers everything visible
  and clears it afterwards; the next frame starts from an empty canvas;
* otherwise the frame stays ("keep") and the next one builds on it: Auto Crop writes only the area
  that changed, Auto Difference writes unchanged pixels inside it as the transparent entry (one
  palette entry is reserved for it), which compresses well.

An opaque animation never clears; a transparent one only at the frames that remove something.
`anim2.gif` exported back this way plays the same four frames in ImageMagick, pixel for pixel, in
17893 bytes (the original: 18500).

### Speed

An animated GIF's frames are drawn, and mapped to the palette, on several threads (desktop builds;
the browser build uses one). Nearest-entry search goes by color cell (the color cube cut into
16 × 16 × 16 cells, each keeping the entries that can be nearest to a color in it: the same answers
as a full search). Above 262,144 distinct colors the median cut works on colors grouped by 5 bits
per channel (tubes.png's 103 blurred frames: 5.8 million colors; same error, 9.9 levels on
average). tubes.png as an animated GIF: 9.8 s → 2.5 s (debug build, 4 cores; release 2.3 s).
The Optimize panel's Colors and Dither boxes take effect when a drag ends or a typed value is
confirmed (Enter or clicking elsewhere), so the preview isn't exported again for each step.

### Maximize colors per frame (a FireCraft addition)

A GIF allows 256 colors per color table, and each frame can have its own table. With this option
(Animated GIF, Adaptive / WebSnap Adaptive / Exact / Grayscale / Uniform) every frame gets a table made for
the pixels it writes: with Auto Crop the area that changed, with Auto Difference only the changed
pixels (the rest are the transparent entry and keep the previous frame's). An animation can so
show far more than 256 colors in all: `tc217.gif` (a "true color GIF", 173 frames of 16×16 tiles,
32,697 colors) exports back exact within one level in 182 KB (the original: 184 KB), against 255
colors and errors up to 32 with one shared table; Fireworks MX opens it with all its colors. Two
more GIFs with a table per frame come back exact as well: `demoanim.gif` (162 frames, 32,510
colors; error 0 on every frame, 5.18 MB against the original's 5.17 MB; one shared table: errors up
to 76) and `gifColors.gif` (20 frames, 2,578 colors; within one level in 54 KB, the original
200 KB; one shared table: errors up to 59). Neither
Auto Crop nor Auto Difference is required; without both, each table covers its whole frame (256
colors per picture).

Details: whether a pixel changed is decided on its color (palette indices of different tables
mean different colors); the first frame covers the whole image, as with no global color table
ImageMagick and PIL fill what it leaves uncovered with an opaque background; Grayscale picks the
grays that fit each frame best (least squared error), not the fixed ramp; dithering skips the
pixels a frame doesn't write. Dithering can shimmer between frames where a frame redraws an area
with a different table (a dither that stays stable from frame to frame is a possible follow-up).
Stored as `FCPb` in `EXPv`. Exporting is often faster than with one shared table (each table is
made from fewer colors).

**Uniform** per frame is a grid fitted to the frame's colors: each channel's range (lowest to
highest value of the pixels the frame writes) cut into equal slices with a level in the middle of
each (two levels over 0..255: 64 and 191), and the channels given levels one at a time to the one
with the coarsest step while the grid fits the Colors count (256 colors can be 10 × 25 × 1, not
the 6 × 6 × 6 cube; a channel without spread gets one level). Compared with the fixed grid
(average / largest error in levels, all frames): tc217.gif 19 / 25 → 1.1 / 4 at 256 colors and
103 / 127 → 2.6 / 11 at 16; tubes.gif 16 / 25 → 15 / 20 and 68 / 127 → 39 / 61; anim2.gif
(mostly pure black and white, which the fixed grid hits exactly) 15 / 25 → 17 / 21 and
92 / 123 → 52 / 64; demoanim.gif 18 / 25 → 14 / 21; gifColors.gif (frames spanning nearly the whole
color range) 12 / 25 → 15 / 21.

**Web 216, Windows, Macintosh** below their full size (e.g. Macintosh at 16 colors): Fireworks MX
keeps the entries the pixels use most (`quantize::most_used`, still what a shared table does). Per
frame, each frame takes the entries of the palette that fit it best (`quantize::best_subset`:
picked one at a time, each the entry lowering the total squared error most, on colors grouped by
4 bits per channel), which unlike "most used" keeps an entry a few pixels need when nothing else is
near them. Over tc217, demoanim, gifColors, tubes and anim2 at 16 and 64 colors: per frame against
shared, the average error drops (Macintosh 16 on demoanim 24 → 17, on gifColors 38 → 23); best fit
against most used per frame, the average is about the same and the largest error mostly much lower
(gifColors Macintosh 16: 148 → 81, tubes 180 → 114, demoanim Macintosh 64: 93 → 38; slightly higher
in a few, tc217 25 → 30), for about 10% more time. At the full palette size every entry is kept, so
the option changes nothing there.

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
* **Palette order** (the file's, and the color table's Sort by None), measured on Fireworks MX's exports
  (`probes/palette_order_mx`, Rec. 601 luma): Adaptive with every color fitting, darkest first; Exact,
  lightest first; a GIF's transparent entry last. An animated GIF reverses the Adaptive order (lightest
  first; a reduced palette's order reversed too: `anim_adaptive4.gif` against `bands_adaptive4.gif`)
  and keeps Exact's. (Fireworks MX's animated Adaptive also shifts colors a little even when all fit:
  #00C000 became #00BD00, #FF8000 #FF8400; FireCraft keeps them exact.) A PNG 8's alpha palette from Adaptive, reduced or not:
  by opacity, the clear entry first, then the partly transparent levels, each lightest first (so `tRNS`
  ends at the last partly transparent entry); from Exact: opaque colors lightest first, the clear entry
  after. A reduced Adaptive palette without alpha has Fireworks MX's own order (unsorted), FireCraft's
  is the median cut's (most used first). Left as is: in a GIF with alpha, Fireworks MX orders the
  semi-transparent pixels' colors by their own lightness before they are blended onto the matte;
  FireCraft by the blended color's.
* Color mapping is by plain RGB distance (Windows/Macintosh above).

## JPEG

Fireworks MX's quality setting isn't libjpeg's: its JPEGs (exported at 0-100 and their tables read
back) match libjpeg quality 8, 9, 11, 17, 40, 51, 64, 74, 86, 94, 96, 100 at settings 0, 10, ..., 90,
95, 100. FireCraft encodes with the libjpeg quality for the setting (interpolated), and reads an
opened JPEG's quality back on Fireworks MX's scale (a libjpeg-92 file: 88; Fireworks MX shows 86).
Fireworks MX writes JPEGs without chroma subsampling (1×1), FireCraft too by default. Smoothing
is approximated with a blur of 0.5 px per level.

## The color table

The Optimize panel's color table, for GIF, animated GIF and PNG 8 (`crates/ui-egui/src/optimize/table.rs`),
follows Fireworks MX's help ("Viewing colors in a palette" and the pages after it) and its palette menu
strings (Fireworks Resources.dll):

* It shows the palette of the last export: kept up to date in Preview, otherwise a **Rebuild** button
  takes the place of the color count once the document has changed. Twelve lines of small swatches
  (12 x 10 points, single lines between them) show at once and the rest scroll, as in Fireworks MX.
  A PNG 8's partly transparent entries show their color over a checkerboard.
* Holding the mouse down on a swatch shows its pixels in the Preview in a highlight color until it is
  released (Fireworks MX's help: "temporarily change to another highlight color"). Measured in Fireworks
  MX (`probes/highlight_mx`): not one color but the held color's opposite hue, fully saturated, with its
  lightness inverted (green to #FF08FC, yellow to #0707FE, #808080 to #08FBFE, magenta to #0EFF08, navy
  to #FEFC82; a grey takes hue 0 and turns cyan); FireCraft computes it the same way, within a few levels
  of Fireworks MX's Windows 0-240 HLS steps. Partly transparent pixels keep their opacity (the user saw
  a partly transparent #BE3531 highlighted on screen as #5CF1FE in Fireworks MX); fully transparent ones
  show nothing. In that session, holding Fireworks MX's first swatch highlighted the pixels of another
  color (its table order and its highlight disagree); FireCraft highlights the held color.
* The other way round, Show Swatch Feedback (Fireworks MX's palette menu, on by default; FireCraft: the
  table's right-click menu): with the pointer over the Preview, the swatch of the color under it gets a
  white line in place of its black cell line (measured in Fireworks MX on `probes/highlight_mx/stripes.png`,
  once the document window is active). Over a clear pixel, the transparent entries are outlined. The
  table doesn't scroll to the swatch. With "Maximize colors per frame" it shows the table of
  the frame shown (stepping or playing; the frames' tables are kept with the preview only, never
  written to the document).
* Swatch marks: edited (a square at the bottom left), locked (bottom right), transparent (checkered),
  web safe (a diamond). Click selects, Ctrl+click adds, Shift+click a range; double-click edits; the
  tooltip gives the color, its marks and how many pixels use it.
* Buttons: Select Transparent Color, Add Color to Transparency, Remove Color from Transparency (then click
  a swatch or a pixel of the Preview), Edit Color, Snap to Web Safe, Lock Color, Add Color, Delete Color.
  Sort by None, Luminance or Popularity. The right-click menu adds Unlock All Colors, Remove Edit,
  Remove All Edits, Load Palette (`.act`, a GIF's or a PNG 8's colors), Save Palette (`.act`) and Rebuild
  Color Table. The palette menu's Custom... loads a palette, as in Fireworks MX.
* Any edit makes the palette **Custom**: the table itself (Fireworks MX's help: "Custom is a palette that
  has been modified or loaded"; whether its own UI switches the menu on an edit is unchecked). Locking
  doesn't: locked colors join whatever palette is chosen, as the help says.
* Transparent colors switch No Transparency to Index Transparency; with a custom palette they replace the
  matte color as the transparent one. A GIF has one transparent entry, so the pixels of every transparent
  color take it (in an animation that can make frames clear and redraw, which costs file size).

Stored in `EXPv > PALv`: one `FCLi` per entry, then `CLLb` (locked), `CLTb` (transparent), `CLMb` +
`BCLi` (edited) when set; `PLNi` counts them (tags from Fireworks MX's writer at Fireworks.exe 0x79cd39).
`FCLi` is the color pixels are matched to and `BCLi` the color they export as: Fireworks MX's script API
names it `newColorValue`, and two probes it exported (`probes/custom_palette_mx`, `..._mx2`, made by
`crates/io/examples/custom_palette_probe.rs`) map red pixels to the `FCLi` red's `BCLi` green, and to the
nearest `FCLi` (yellow) when no `FCLi` is red. Fireworks MX reads FireCraft's table back entry for entry
(`paletteMode` custom, `paletteInfo` with the same flags). Fireworks MX also keeps its last adaptive table
there for the other palettes; FireCraft reads only the locked entries of those and writes only them.
### Remove Unused Colors

Fireworks MX's Optimize option (`PLOb`, `optimized` in its script API, on by default; measured by toggling
it with `dom.setExportOptions` and comparing the saved records). Exports of `probes/remove_unused_mx`
(two colors, a custom table with an unused edited entry and a locked yellow): on, the file's palette
has only the entries pixels use, in the table's order, plus locked colors even when unused; off, the
whole table, or all 16 grays (plus the locked yellow) for Grayscale at 16 colors. Web 216 trimmed below
its size keeps only colors in use either way (16 requested: the 2 used and the locked yellow), so
FireCraft's trimming of Web 216, Windows and Macintosh now leaves unused entries out too. FireCraft keeps
the transparent entry as well (Auto Difference needs it after mapping). The Optimize panel shows it as
the "Remove unused colors" checkbox (Fireworks MX: in the panel's menu). It only changes anything when the palette
has entries no pixel uses: fixed palettes, custom ones, and the few adaptive entries no pixel ends up
nearest to. An image whose colors all fit (100dpcm_fw.png: 17 color+alpha combinations) has none.
cone3.png as PNG 8, Adaptive 256, Alpha: FireCraft's median cut makes 256 entries of which 10 go unused
(246 with the option on); Fireworks MX's own adaptive palette makes 195 (194 on), a difference of the
palette algorithm (roadmap: Fireworks MX's own adaptive palette), not of this option.

## Not yet

2-Up/4-Up and Export Preview; GIF loss (lossy LZW); the color table's Replace Palette Entry; Save/Delete
Settings, Optimize to Size, Sharpen JPEG Edges; Fireworks MX's own adaptive palette (see
[roadmap item 7](../roadmap.md));
Selective JPEG quality; animated WebP export; WebP in the browser build; 16-bit (PNG 48/64) export,
which needs a 16-bit pipeline (layers are 8 bits per channel).
