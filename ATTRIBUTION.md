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
