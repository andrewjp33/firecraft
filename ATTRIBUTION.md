# Attribution

FireCraft is MIT OR Apache-2.0. It adapts small pieces from these projects under their licences:

## FirePixel

- Project: https://github.com/Zignixx/firepixel
- Licence: Apache License 2.0
- Copyright: the FirePixel authors

Adapted:

- `crates/raster/src/nine.rs`: the per-axis 9-slice mapping rule (`AxisMap::new`/`apply`, guide
  rounding, zero margin for guides outside the box, fallback to plain scaling) from
  `crates/fp-render/src/nine.rs`. FireCraft added the inverse mapping and bitmap resampling.
- `docs/fireworks-png.md`: the names of CS-era 9-slice nodes (`NGSv` with `NGLb`, `NGKi`, four
  `NGCf` guides, `NGMv`, `NGVv`) follow FirePixel's reader (`crates/fp-fwpng/src/read.rs`).
