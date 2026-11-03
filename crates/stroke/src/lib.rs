//! Fireworks' stroke engine: brush tips, dab stepping ("walkers"), simple and natural brushes,
//! stroke placement, and Fireworks' own Bezier flattening.
//!
//! Adapted from FirePixel (https://github.com/Zignixx/firepixel, Apache-2.0, v0.2.23):
//! `fp-vector` (`attrs`, `brush`, `coverage`, `natural`, `paint`, `stroke`) and `fp-geom`
//! (`flatten`). The module comments are FirePixel's (in German) and describe the measured
//! Fireworks behaviour each piece reproduces. FireCraft's changes: the few items those modules
//! used from FirePixel's `fp-geom`, `fp-color` and `fp-raster` crates are in [`support`];
//! rows are rasterized sequentially instead of with rayon (FirePixel's results don't depend on
//! the thread count); see ATTRIBUTION.md.
#![forbid(unsafe_code)]

pub mod attrs;
pub mod brush;
pub mod coverage;
pub mod flatten;
pub mod natural;
pub mod paint;
pub mod stroke;
pub mod support;

pub use attrs::{BrushParams, BrushType, PathContour, PointDynamics, Stroke, StrokePlacement, VectorPath};
