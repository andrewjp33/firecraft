# Attribution

FireCraft is MIT OR Apache-2.0. It adapts small pieces from these projects under their licences:

## FirePixel

- Project: https://github.com/Zignixx/firepixel
- Licence: Apache License 2.0
- Copyright: the FirePixel authors

Adapted:

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
