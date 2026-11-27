//! Live marquee (Fireworks CS3 and later; measured in Fireworks CS6 on 2026-11-26): with the
//! Property inspector's "Live marquee" ticked, changing the Edge of the Marquee or Oval Marquee,
//! or the Magic Wand's Tolerance or Edge, redoes the selection the tool just made with the new
//! settings. Each change is a new undo step with the tool's own name ("Select Pixels", "Magic
//! Wand Tool").
//!
//! What CS6 does, and FireCraft with it:
//! * Only the newest selection step is redone, combined with the selection before it as it was
//!   (a marquee added with Shift: only the added part changes).
//! * Only by the tool that made it: the Marquee's Edge leaves an Oval Marquee or Magic Wand
//!   selection alone.
//! * Anything recorded after it ends it, Move included, and so do Undo and Redo: changing the
//!   settings then changes only the tool's settings.
//!
//! FireCraft addition: while a slider or number is dragged, the changes amend one step instead
//! of making a step for every value passed.

use firecraft_doc::ElementKind;
use firecraft_raster::mask::Combine;
use firecraft_raster::wand::magic_wand;
use firecraft_raster::Mask;

use crate::{DocState, ElementRef, PixelSelection, ToolOptions};

/// The tool whose settings changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveTool {
    Marquee,
    OvalMarquee,
    MagicWand,
}

/// What the selection step did, to do it again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LiveOp {
    /// A marquee over the pixel-grid box `rect`.
    Marquee { rect: (f64, f64, f64, f64), oval: bool },
    /// A Magic Wand click at canvas (`x`, `y`).
    Wand { x: f64, y: f64 },
}

impl LiveOp {
    fn tool(self) -> LiveTool {
        match self {
            Self::Marquee { oval: false, .. } => LiveTool::Marquee,
            Self::Marquee { oval: true, .. } => LiveTool::OvalMarquee,
            Self::Wand { .. } => LiveTool::MagicWand,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LiveSelection {
    op: LiveOp,
    target: Option<ElementRef>,
    /// The selection before the step and how the step combined with it.
    base: Option<PixelSelection>,
    combine: Combine,
    /// The selection the step left, and the history right after it: both must still hold.
    after: Option<PixelSelection>,
    serial: u64,
    /// The newest step is a live redo (a drag may amend it).
    redone: bool,
}

impl DocState {
    /// Remember the selection step just done (`op`) for Live marquee.
    pub(crate) fn remember_live(
        &mut self,
        op: LiveOp,
        target: Option<ElementRef>,
        base: Option<PixelSelection>,
        combine: Combine,
    ) {
        self.live_selection = Some(LiveSelection {
            op,
            target,
            base,
            combine,
            after: self.pixels.clone(),
            serial: self.history.serial(),
            redone: false,
        });
    }

    /// Whether `tool`'s last selection can still be redone (nothing happened since).
    pub fn live_selection_for(&self, tool: LiveTool) -> bool {
        self.live_selection.as_ref().is_some_and(|l| {
            l.op.tool() == tool && l.serial == self.history.serial() && l.after == self.pixels
        })
    }

    /// Live marquee: `tool`'s settings changed in `opts`; redo its last selection with them.
    /// `amend`: the change continues a drag of the same control, so a step this made for the
    /// drag's earlier values takes the new result instead of a new step being added. `false` if
    /// there is nothing to redo.
    pub fn live_reselect(&mut self, tool: LiveTool, opts: &ToolOptions, amend: bool) -> bool {
        if !self.live_selection_for(tool) {
            return false;
        }
        let Some(live) = self.live_selection.take() else {
            return false;
        };
        let redo = match live.op {
            LiveOp::Marquee { rect, oval } => {
                let (x, y, mask) = self.marquee_mask(rect, oval, opts.marquee_edge);
                Some(("Select Pixels", x, y, mask))
            }
            LiveOp::Wand { x, y } => self.wand_mask(live.target, x, y, opts).map(|(bx, by, mask)| ("Magic Wand Tool", bx, by, mask)),
        };
        let Some((label, x, y, mask)) = redo else {
            return false;
        };
        let next = Self::combined(live.base.clone(), live.target, x, y, mask, live.combine);
        let redone = if amend && live.redone {
            // The step made earlier in this drag holds the selection from before the drag.
            self.apply_pixels(next, live.target);
            self.revision = self.revision.wrapping_add(1);
            true
        } else {
            self.select_step(label, |s| s.apply_pixels(next, live.target))
        };
        self.live_selection = Some(LiveSelection {
            after: self.pixels.clone(),
            serial: self.history.serial(),
            redone,
            ..live
        });
        true
    }

    /// The Magic Wand's mask for a click at canvas (`x`, `y`) on bitmap `target`: the bitmap's
    /// canvas position and the mask.
    pub(crate) fn wand_mask(&self, target: Option<ElementRef>, x: f64, y: f64, opts: &ToolOptions) -> Option<(i64, i64, Mask)> {
        let Some(ElementKind::Bitmap(b)) = self.element(target?).map(|e| &e.kind) else {
            return None;
        };
        let (px, py) = ((x - b.x as f64).floor() as i64, (y - b.y as f64).floor() as i64);
        Some((b.x, b.y, magic_wand(&b.image, px, py, opts.wand())))
    }
}

#[cfg(test)]
mod tests {
    use firecraft_doc::Document;
    use firecraft_raster::mask::Combine;
    use firecraft_raster::{Edge, Rgba, RgbaImage};

    use super::LiveTool;
    use crate::{DocState, MarqueeStyle, ToolOptions};

    /// 100 x 100: white with a red disc (radius 30) in the middle and a red-ish band at the
    /// bottom that a high tolerance takes in.
    fn doc() -> DocState {
        let mut img = RgbaImage::new(100, 100, Rgba::WHITE);
        for y in 0..100 {
            for x in 0..100 {
                if (x as f64 - 49.5).powi(2) + (y as f64 - 49.5).powi(2) < 900.0 {
                    img.set(x, y, Rgba::new(255, 0, 0, 255));
                } else if y >= 90 {
                    img.set(x, y, Rgba::new(255, 60, 60, 255));
                }
            }
        }
        DocState::new(Document::from_image(img, None))
    }

    fn selected(d: &DocState, x: i64, y: i64) -> u8 {
        d.pixels.as_ref().map_or(0, |p| p.mask.get(x - p.x, y - p.y))
    }

    #[test]
    fn marquee_edge_redoes_the_marquee() {
        let mut d = doc();
        let mut o = ToolOptions { marquee_edge: Edge::Hard, ..Default::default() };
        d.marquee((10.0, 10.0, 40.0, 40.0), false, &o, Combine::Replace);
        assert_eq!(selected(&d, 10, 10), 255);
        assert_eq!(selected(&d, 9, 20), 0);
        let steps = d.history.labels().count();
        o.marquee_edge = Edge::Feather(10);
        assert!(d.live_reselect(LiveTool::Marquee, &o, false));
        assert!(selected(&d, 10, 10) < 255 && selected(&d, 9, 20) > 0, "feathered now");
        // A new step, with the tool's own name (Fireworks CS6).
        assert_eq!(d.history.labels().count(), steps + 1);
        assert_eq!(d.history.next_undo(), Some("Select Pixels"));
        // Another change redoes it again; undo goes back one change at a time.
        o.marquee_edge = Edge::Hard;
        assert!(d.live_reselect(LiveTool::Marquee, &o, false));
        assert_eq!(selected(&d, 9, 20), 0);
        d.undo();
        assert!(selected(&d, 9, 20) > 0);
    }

    #[test]
    fn only_the_added_marquee_changes() {
        let mut d = doc();
        let mut o = ToolOptions { marquee_edge: Edge::Hard, ..Default::default() };
        d.marquee((5.0, 5.0, 25.0, 25.0), false, &o, Combine::Replace);
        d.marquee((60.0, 60.0, 80.0, 80.0), false, &o, Combine::Add);
        o.marquee_edge = Edge::Feather(4);
        assert!(d.live_reselect(LiveTool::Marquee, &o, false));
        assert_eq!(selected(&d, 5, 5), 255, "the first marquee keeps its hard edge");
        assert_eq!(selected(&d, 4, 15), 0);
        assert!(selected(&d, 59, 70) > 0, "the added one is feathered");
    }

    #[test]
    fn only_the_tool_that_made_it() {
        let mut d = doc();
        let o = ToolOptions { marquee_edge: Edge::Hard, ..Default::default() };
        d.marquee((10.0, 10.0, 40.0, 40.0), true, &o, Combine::Replace);
        assert!(!d.live_reselect(LiveTool::Marquee, &o, false));
        assert!(!d.live_reselect(LiveTool::MagicWand, &o, false));
        assert!(d.live_reselect(LiveTool::OvalMarquee, &o, false));
    }

    #[test]
    fn move_undo_and_redo_end_it() {
        let mut d = doc();
        let mut o = ToolOptions { marquee_edge: Edge::Hard, ..Default::default() };
        d.marquee((10.0, 10.0, 40.0, 40.0), false, &o, Combine::Replace);
        d.move_pixel_selection(5, 0);
        o.marquee_edge = Edge::Feather(3);
        assert!(!d.live_reselect(LiveTool::Marquee, &o, false));
        d.undo();
        assert!(!d.live_reselect(LiveTool::Marquee, &o, false), "not after Undo (CS6)");
        d.redo();
        assert!(!d.live_reselect(LiveTool::Marquee, &o, false));
    }

    #[test]
    fn wand_tolerance_redoes_from_the_same_click() {
        let mut d = doc();
        let mut o = ToolOptions { tolerance: 0, wand_edge: Edge::Hard, ..Default::default() };
        d.magic_wand(50.0, 50.0, &o, Combine::Replace);
        assert_eq!(selected(&d, 50, 95), 0);
        o.tolerance = 80;
        assert!(d.live_reselect(LiveTool::MagicWand, &o, false));
        assert_eq!(selected(&d, 50, 50), 255);
        assert_eq!(selected(&d, 50, 95), 0, "the band isn't connected to the disc");
        assert_eq!(d.history.next_undo(), Some("Magic Wand Tool"));
        assert!(!d.live_reselect(LiveTool::Marquee, &o, false));
    }

    #[test]
    fn a_drag_amends_one_step() {
        let mut d = doc();
        let mut o = ToolOptions { marquee_edge: Edge::Hard, ..Default::default() };
        d.marquee((10.0, 10.0, 40.0, 40.0), false, &o, Combine::Replace);
        let steps = d.history.labels().count();
        for r in 1..=6 {
            o.marquee_edge = Edge::Feather(r);
            assert!(d.live_reselect(LiveTool::Marquee, &o, true));
        }
        assert_eq!(d.history.labels().count(), steps + 1);
        assert!(selected(&d, 5, 20) > 0, "feather 6 reaches 5 pixels out");
        d.undo();
        assert_eq!(selected(&d, 9, 20), 0, "one undo: back to the hard edge");
    }

    fn style(s: MarqueeStyle, n: (u32, u32)) -> ToolOptions {
        ToolOptions { marquee_style: s, marquee_ratio: n, marquee_size: n, ..Default::default() }
    }

    #[test]
    fn fixed_ratio_as_measured() {
        let b = DocState::marquee_drag_box;
        // 1:2, the height follows the pointer (Fireworks MX and CS6, 2026-11-26).
        let o = style(MarqueeStyle::FixedRatio, (1, 2));
        assert_eq!(b((13.0, 11.0), (113.0, 51.0), &o), (13.0, 11.0, 33.0, 51.0));
        assert_eq!(b((113.0, 111.0), (93.0, 21.0), &o), (68.0, 21.0, 113.0, 111.0));
        assert_eq!(b((123.0, 93.0), (23.0, 73.0), &o), (113.0, 73.0, 123.0, 93.0));
        // 2:1, the width follows it.
        let o = style(MarqueeStyle::FixedRatio, (2, 1));
        assert_eq!(b((23.0, 13.0), (123.0, 33.0), &o), (23.0, 13.0, 123.0, 63.0));
        assert_eq!(b((23.0, 13.0), (53.0, 73.0), &o), (23.0, 13.0, 53.0, 28.0));
        assert_eq!(b((23.0, 13.0), (26.0, 14.0), &o), (23.0, 13.0, 26.0, 15.0));
        // 3:1 rounds: 7 wide is 2 high, 8 wide 3 high.
        let o = style(MarqueeStyle::FixedRatio, (3, 1));
        assert_eq!(b((13.0, 11.0), (20.0, 51.0), &o), (13.0, 11.0, 20.0, 13.0));
        assert_eq!(b((13.0, 11.0), (21.0, 51.0), &o), (13.0, 11.0, 21.0, 14.0));
        // 1:1, the height.
        let o = style(MarqueeStyle::FixedRatio, (1, 1));
        assert_eq!(b((13.0, 11.0), (113.0, 31.0), &o), (13.0, 11.0, 33.0, 31.0));
        assert_eq!(b((13.0, 11.0), (33.0, 91.0), &o), (13.0, 11.0, 93.0, 91.0));
    }

    #[test]
    fn fixed_size_sits_under_the_pointer() {
        let o = style(MarqueeStyle::FixedSize, (64, 64));
        assert_eq!(DocState::marquee_drag_box((153.0, 126.0), (103.4, 76.9), &o), (103.0, 76.0, 167.0, 140.0));
        // Cut to the canvas when it's made (MX: let go at 183,136 on 200 x 150 gave 17 x 14).
        let mut d = DocState::new(Document::from_image(RgbaImage::new(200, 150, Rgba::WHITE), None));
        let r = DocState::marquee_drag_box((183.0, 136.0), (183.0, 136.0), &o);
        d.marquee(r, false, &o, Combine::Replace);
        let p = d.pixels.as_ref().expect("selected");
        let (x, y, w, h) = p.mask.bounds().expect("not empty");
        assert_eq!((i64::from(x) + p.x, i64::from(y) + p.y, w, h), (183, 136, 17, 14));
    }
}
