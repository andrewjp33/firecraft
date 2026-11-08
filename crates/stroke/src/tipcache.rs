//! Brush tips kept between calls. A live stroke redraws small areas many times with the same
//! tip; building a large tip and its sub-pixel shifted copies each time took most of the time.
//! The tips depend only on the key, so keeping them cannot change a result.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::brush::Tip;

/// What a brush's tips are computed from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TipKey {
    pub natural: bool,
    pub dp: f64,
    pub round: bool,
    pub soft: u32,
    pub bell: bool,
    pub aspect: u32,
    pub angle: u32,
    pub min_size: u32,
    pub m: usize,
    pub angle_mode: bool,
}

/// The tips of one brush.
#[derive(Default)]
pub(crate) struct Tips {
    /// By size/angle stage.
    pub stages: HashMap<usize, Tip>,
    /// By (stage, sub-pixel x, sub-pixel y).
    pub shifted: HashMap<(usize, usize, usize), Tip>,
}

/// Don't keep more than this; a huge brush's copies are rebuilt instead.
const MAX_KEPT_BYTES: usize = 128 << 20;

thread_local! {
    static KEPT: RefCell<Option<(TipKey, Tips)>> = const { RefCell::new(None) };
}

/// The tips kept for `key` (empty if another brush was used last).
pub(crate) fn take(key: TipKey) -> Tips {
    KEPT.with(|k| match k.borrow_mut().take() {
        Some((kept, tips)) if kept == key => tips,
        _ => Tips::default(),
    })
}

/// Keep `tips` for the next call.
pub(crate) fn keep(key: TipKey, tips: Tips) {
    let bytes: usize = tips
        .shifted
        .values()
        .chain(tips.stages.values())
        .map(|t| t.data.len())
        .sum();
    if bytes <= MAX_KEPT_BYTES {
        KEPT.with(|k| *k.borrow_mut() = Some((key, tips)));
    }
}
