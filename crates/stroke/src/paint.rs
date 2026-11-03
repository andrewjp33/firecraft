//! From FirePixel's `fp-vector/src/paint.rs` (Apache-2.0): the 8-bit product the stroke engine
//! uses (FirePixel's fill painting isn't part of FireCraft's stroke crate).

use crate::support::color::mul255;

/// `mul255` als `u8`.
#[inline]
pub fn mul(a: u8, b: u8) -> u8 {
    mul255(a as u32, b as u32) as u8
}
