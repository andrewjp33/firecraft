# Attribution

FireCraft is MIT OR Apache-2.0. It adapts small pieces from these projects under their licences:

## FirePixel

- Project: https://github.com/Zignixx/firepixel
- Licence: Apache License 2.0
- Copyright: the FirePixel authors

Adapted:

- `crates/ui-egui/src/fpui/`: FirePixel's re-creation of Fireworks CS's colour pop-up
  (`color_picker.rs`, from `crates/fp-ui/src/inspector/color_picker.rs`), its glyph set
  (`glyphs.rs`, from `inspector/glyphs.rs`), field/button helpers (`fields.rs`, from
  `inspector/fields.rs`, `widgets.rs`, `dialogs/mod.rs`), its default theme colours (`theme.rs`)
  and English texts (`texts.rs`, from `fp-i18n`'s en locale). FireCraft's changes are listed at
  the top of `color_picker.rs`.
- `crates/engine/src/paint.rs`: how a painted stroke is composed and the Eraser's dab spacing,
  from FirePixel's `crates/fp-engine/src/paint.rs` and `commands/bitmap.rs`. The Eraser's tip was
  measured against Fireworks MX (`docs/behavior/painting.md`).
- `crates/raster/src/path.rs` (`fill_polygons_feathered`): Fireworks' feathered fill edge as
  reconstructed in FirePixel's `crates/fp-vector/src/feather.rs`.
- `crates/raster/src/effects.rs` (`tent_plane`, shadow and glow softness): the triangle filter and
  truncating shadow offset as measured in FirePixel's `fp-raster/src/mask.rs` and `fp-effects/src/shadow.rs`.
- `crates/raster/src/bevel.rs` (AngleSoftness): direction averaging and map softening as measured in
  FirePixel's `fp-effects/src/bevel.rs`.
- `crates/text`: fonts, layout and rasterising, adapted from FirePixel's `fp-text` crate.
- `crates/doc/src/brush_presets.rs`: Fireworks' stroke presets with their brush settings, from
  FirePixel's `fp-engine/src/commands/panel.rs` (`BRUSH_PRESETS`) and the English names from its
  `fp-i18n` en locale (FireCraft added the sensitivities it lacked, read back from Fireworks MX).
- `crates/raster/src/pattern.rs`: how Pattern and Web Dither fills map to pixels, as measured in
  FirePixel's `crates/fp-vector/src/paint.rs`.
- `crates/raster/src/effects.rs` (`gaussian_blur`): Fireworks' Gaussian blur as reconstructed in
  FirePixel's `crates/fp-raster/src/filters/gauss.rs` (kernel in single precision, separable
  passes rounding down to 8 bits, colour over non-transparent pixels, output margin).
- `crates/stroke` (`firecraft-stroke`): FirePixel's stroke engine, from `crates/fp-vector/src`
  (`attrs.rs`, `brush.rs`, `coverage.rs`, `natural.rs`, `stroke.rs`, and `mul` from `paint.rs`) and
  `crates/fp-geom/src/flatten.rs` (FirePixel v0.2.23), with the items they used from `fp-geom`,
  `fp-color` and `fp-raster` copied into `support.rs` (`Mask`, `RgbaImage` and the contour types
  reduced). FireCraft's changes: imports, rows rasterized sequentially instead of with rayon,
  the older chord flattening and fill painting left out. Licensed Apache-2.0 like the original.

- `crates/raster/src/nine.rs`: the per-axis 9-slice mapping rule (`AxisMap::new`/`apply`, guide
  rounding, zero margin for guides outside the box, fallback to plain scaling) from
  `crates/fp-render/src/nine.rs`. FireCraft added the inverse mapping and bitmap resampling.
- `docs/fireworks-png.md`: the names of CS-era 9-slice nodes (`NGSv` with `NGLb`, `NGKi`, four
  `NGCf` guides, `NGMv`, `NGVv`) follow FirePixel's reader (`crates/fp-fwpng/src/read.rs`).

## Third-party libraries

FireCraft's dependencies keep their own licences (see `Cargo.lock`). Binary releases must carry
their notices; among them:

- `jpeg-encoder` (JPEG export): MIT or Apache-2.0, and the IJG licence: "This software is based in
  part on the work of the Independent JPEG Group."
- libwebp (WebP export, via the `webp` crate): BSD-3-Clause.
- `jixel` (JPEG XL export): BSD-3-Clause or Apache-2.0. `jxl-oxide` (JPEG XL import): MIT or
  Apache-2.0; it and its `jxl-jbr` are vendored in `vendor/` with one dependency version changed
  (`vendor/README.md`). Their Brotli decoder, `brotli-decompressor`: BSD-3-Clause or MIT.
- `oxipng` (PNG optimizer): MIT, with libdeflate (MIT) and `zopfli` (Apache-2.0).

