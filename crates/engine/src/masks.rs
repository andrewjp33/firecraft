//! Mask commands: the link between an object and its mask, Disable/Enable Mask, the mask mode
//! and "Show fill and stroke" (Property inspector), and Delete Mask. Measured in Fireworks MX
//! through its script API (`linkElementMask`, `enableElementMask`, `setElementMaskMode`,
//! `setElementMaskShowAttrs`, `removeElementMask`); see docs/behavior/masks.md.

use firecraft_doc::{Element, ElementKind, MaskFlags, MaskMode, RenderCtx};
use firecraft_raster::{BlendMode, Rgba, RgbaImage};

use crate::{DocState, ElementRef};

/// How Delete Mask removes a mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskRemoval {
    /// The masked look becomes a bitmap (Fireworks MX's "apply": an image of the mask's
    /// bounds).
    Apply,
    /// The mask goes; the content stays as it is (several objects as a group, as in MX).
    Discard,
    /// Mask and content become separate objects again, the mask on top, as before they were
    /// grouped as a mask (FireCraft).
    BreakApart,
}

impl DocState {
    /// The masked object `r` refers to: the object itself, or the owner of a selected mask.
    pub fn mask_owner(&self, r: ElementRef) -> Option<ElementRef> {
        let owner = if r.is_mask() { r.parent()? } else { r };
        matches!(self.element(owner)?.kind, ElementKind::Masked { .. }).then_some(owner)
    }

    /// The masked object of the selection, if exactly one object (or its mask) is selected.
    pub fn selected_mask_owner(&self) -> Option<ElementRef> {
        match self.selection.as_slice() {
            [r] => self.mask_owner(*r),
            _ => None,
        }
    }

    /// The mask's mode and flags.
    pub fn mask_settings(&self, owner: ElementRef) -> Option<(MaskMode, MaskFlags, bool)> {
        match &self.element(owner)?.kind {
            ElementKind::Masked { mode, flags, mask, .. } => {
                Some((*mode, *flags, matches!(mask.kind, ElementKind::Bitmap(_))))
            }
            _ => None,
        }
    }

    fn change_mask(&mut self, owner: ElementRef, label: &str, f: impl FnOnce(&mut MaskMode, &mut MaskFlags)) {
        let Some((mut mode, mut flags, _)) = self.mask_settings(owner) else {
            return;
        };
        let before = (mode, flags);
        f(&mut mode, &mut flags);
        if (mode, flags) == before {
            return;
        }
        self.edit(label, |s| {
            if let Some(Element {
                kind: ElementKind::Masked { mode: m, flags: fl, .. },
                ..
            }) = s.element_mut(owner)
            {
                *m = mode;
                *fl = flags;
            }
        });
    }

    /// Link or unlink the mask: linked, moving the object moves its mask too.
    pub fn set_mask_linked(&mut self, owner: ElementRef, on: bool) {
        let label = if on { "Link Mask" } else { "Unlink Mask" };
        self.change_mask(owner, label, |_, f| f.linked = on);
    }

    /// Modify > Mask > Disable Mask / Enable Mask.
    pub fn set_mask_enabled(&mut self, owner: ElementRef, on: bool) {
        let label = if on { "Enable Mask" } else { "Disable Mask" };
        self.change_mask(owner, label, |_, f| f.enabled = on);
    }

    /// The Property inspector's "Show fill and stroke".
    pub fn set_mask_show_attrs(&mut self, owner: ElementRef, on: bool) {
        self.change_mask(owner, "Mask Fill and Stroke", |_, f| f.show_attrs = on);
    }

    /// The Property inspector's mask mode: path outline or grayscale for a vector mask, alpha
    /// channel or grayscale for a bitmap mask.
    pub fn set_mask_mode(&mut self, owner: ElementRef, mode: MaskMode) {
        let Some((_, _, bitmap)) = self.mask_settings(owner) else { return };
        // A bitmap has no path outline, a path no alpha channel of its own here.
        let mode = match (mode, bitmap) {
            (MaskMode::PathOutline, true) => MaskMode::Alpha,
            (MaskMode::Alpha, false) => MaskMode::PathOutline,
            (m, _) => m,
        };
        self.change_mask(owner, "Mask Mode", |m, _| *m = mode);
    }

    /// Modify > Mask > Delete Mask, removed `how`; the result is selected.
    pub fn delete_mask(&mut self, owner: ElementRef, how: MaskRemoval) {
        let Some(el) = self.element(owner).cloned() else { return };
        let ctx = RenderCtx {
            symbols: &self.doc.symbols,
            frame: self.frame(),
            math: self.doc.color_math,
            mx_perspective: self.doc.mx_perspective,
            ..RenderCtx::EMPTY
        };
        let Some(replacement) = removed(&el, how, &ctx) else { return };
        let n = replacement.len();
        self.edit("Delete Mask", |s| {
            let at = owner.position();
            let Some(list) = s.list_mut(owner) else { return };
            if at >= list.len() {
                return;
            }
            list.splice(at..=at, replacement);
            s.selection = (at..at + n).map(|i| owner.sibling(i)).collect();
            s.pixels = None;
        });
    }
}

/// What a masked object becomes when its mask is removed `how`, bottom first.
fn removed(el: &Element, how: MaskRemoval, ctx: &RenderCtx) -> Option<Vec<Element>> {
    let ElementKind::Masked { content, mask, .. } = &el.kind else {
        return None;
    };
    // The object's own look (effects, opacity, blending) stays on what replaces it.
    let plain_look = el.effects.is_empty() && el.opacity >= 100.0 && el.blend == BlendMode::Normal;
    let carry = |mut e: Element| {
        e.effects = el.effects.clone();
        e.opacity = el.opacity;
        e.blend = el.blend;
        e.name = el.name.clone().or(e.name);
        e.visible = el.visible;
        e.locked = el.locked;
        e
    };
    Some(match how {
        MaskRemoval::Apply => {
            let mut flat = el.clone();
            flat.effects.clear();
            flat.opacity = 100.0;
            flat.blend = BlendMode::Normal;
            flat.visible = true;
            let (x, y, w, h) = flat.bounds()?;
            const EPS: f64 = 1e-6;
            let (x0, y0) = ((x + EPS).floor() as i64, (y + EPS).floor() as i64);
            let (x1, y1) = ((x + w - EPS).ceil() as i64, (y + h - EPS).ceil() as i64);
            if x1 <= x0 || y1 <= y0 || (x1 - x0) * (y1 - y0) > 100_000_000 {
                return None;
            }
            let mut image = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32, Rgba::TRANSPARENT);
            flat.render_onto(&mut image, x0, y0, ctx);
            let bitmap = Element::new(ElementKind::Bitmap(firecraft_doc::Bitmap { x: x0, y: y0, image }));
            vec![carry(bitmap)]
        }
        MaskRemoval::Discard => match content.as_slice() {
            [one] if plain_look && el.name.is_none() => vec![one.clone()],
            _ => vec![carry(Element::new(ElementKind::Group(content.clone())))],
        },
        MaskRemoval::BreakApart => {
            let mut parts = content.clone();
            let mut m = (**mask).clone();
            m.visible = true;
            parts.push(m);
            if plain_look {
                parts
            } else {
                // Keep the object's look on its parts together.
                vec![carry(Element::new(ElementKind::Group(parts)))]
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{Bitmap, Document};

    fn bitmap(x: i64, w: u32, c: Rgba, name: &str) -> Element {
        let mut e = Element::new(ElementKind::Bitmap(Bitmap {
            x,
            y: 0,
            image: RgbaImage::new(w, 10, c),
        }));
        e.name = Some(name.into());
        e
    }

    /// Two red bitmaps (0..20, 10..30) masked by a white one (5..15), selected none.
    fn doc() -> DocState {
        let mut d = DocState::new(Document::new(40, 10, 72.0, None));
        let masked = Element::new(ElementKind::Masked {
            content: vec![
                bitmap(0, 20, Rgba::new(255, 0, 0, 255), "a"),
                bitmap(10, 20, Rgba::new(255, 0, 0, 255), "b"),
            ],
            mask: Box::new(bitmap(5, 10, Rgba::WHITE, "m")),
            mode: MaskMode::Grayscale,
            cache: None,
            cache_key: 0,
            flags: MaskFlags::default(),
        });
        d.add_element(masked, "Add");
        d.selection.clear();
        d
    }

    fn top(d: &DocState) -> Vec<String> {
        d.doc.layers[0].elements(0).iter().map(Element::label).collect()
    }

    #[test]
    fn unlinked_moves_leave_the_mask() {
        let mut d = doc();
        let o = ElementRef::new(0, 0);
        d.set_mask_linked(o, false);
        d.selection = vec![o];
        d.move_selection(3.0, 0.0, "Move");
        let m = o.mask().and_then(|m| d.element(m)).and_then(Element::bounds);
        let c = o.child(0).and_then(|c| d.element(c)).and_then(Element::bounds);
        assert_eq!((m.map(|b| b.0), c.map(|b| b.0)), (Some(5.0), Some(3.0)));
        // Linked again, both move.
        d.set_mask_linked(o, true);
        d.move_selection(3.0, 0.0, "Move");
        let m = o.mask().and_then(|m| d.element(m)).and_then(Element::bounds);
        assert_eq!(m.map(|b| b.0), Some(8.0));
        assert_eq!(d.history.labels().last(), Some("Move"));
    }

    #[test]
    fn settings_change_as_undo_steps() {
        let mut d = doc();
        let o = ElementRef::new(0, 0);
        d.set_mask_enabled(o, false);
        assert_eq!(d.element(o).and_then(Element::bounds), Some((0.0, 0.0, 30.0, 10.0)));
        // A bitmap mask has no path outline: it asks for its alpha channel.
        d.set_mask_mode(o, MaskMode::PathOutline);
        assert_eq!(d.mask_settings(o).map(|s| s.0), Some(MaskMode::Alpha));
        d.undo();
        d.undo();
        assert_eq!(d.mask_settings(o).map(|s| (s.0, s.1.enabled)), Some((MaskMode::Grayscale, true)));
    }

    #[test]
    fn delete_mask_three_ways() {
        let o = ElementRef::new(0, 0);
        // Apply: one bitmap of what showed, the mask's bounds.
        let mut d = doc();
        d.selection = vec![o.mask().expect("mask")];
        d.delete_mask(o, MaskRemoval::Apply);
        assert_eq!(top(&d), ["Bitmap"]);
        assert_eq!(d.element(o).and_then(Element::bounds), Some((5.0, 0.0, 10.0, 10.0)));
        assert_eq!(d.selection, [o]);
        // Discard: the content, as a group of its two objects.
        let mut d = doc();
        d.delete_mask(o, MaskRemoval::Discard);
        assert_eq!(top(&d), ["Group"]);
        // Break apart: the content objects and the mask, the mask on top.
        let mut d = doc();
        d.delete_mask(o, MaskRemoval::BreakApart);
        assert_eq!(top(&d), ["a", "b", "m"]);
        assert_eq!(d.selection.len(), 3);
        d.undo();
        assert_eq!(top(&d), ["Masked Group"]);
    }
}
