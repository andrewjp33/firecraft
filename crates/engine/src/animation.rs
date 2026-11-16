//! Animation symbols: Modify > Animation (Animate Selection, Settings, Remove Animation), the
//! Property inspector's animation fields and the frames an animation needs. Measured in
//! Fireworks MX on 2026-11-15 (docs/behavior/symbols.md#animation-symbols).

use firecraft_doc::{Animation, ElementKind, Frame, SymbolKind};

use crate::{DocState, ElementRef};

/// The Animate dialog's values (Fireworks MX's Modify > Animation > Animate Selection and
/// Settings): what the user sees, as opposed to the stored [`Animation`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimateSettings {
    pub frames: u32,
    /// Distance in pixels.
    pub movement: f64,
    /// Degrees counter-clockwise from the right (screen up is 90): Fireworks MX stored Move 50 at
    /// Direction 30 as `ANXf` 43.30127, `ANYf` -25.
    pub direction: f64,
    /// Percent at the last frame.
    pub scale: f64,
    /// Percent at the first and last frame.
    pub start_opacity: f64,
    pub end_opacity: f64,
    /// Degrees (not negative); `ccw` turns it counter-clockwise.
    pub rotation: f64,
    pub ccw: bool,
}

impl Default for AnimateSettings {
    /// Fireworks MX's dialog for a new animation: 5 frames, Move 72, Direction 0.
    fn default() -> Self {
        AnimateSettings {
            frames: 5,
            movement: 72.0,
            direction: 0.0,
            scale: 100.0,
            start_opacity: 100.0,
            end_opacity: 100.0,
            rotation: 0.0,
            ccw: false,
        }
    }
}

/// Opacity percent -> the stored per mille of an 8-bit alpha (50% = alpha 128 = 501).
fn per_mille(percent: f64) -> u32 {
    let alpha = (percent.clamp(0.0, 100.0) * 255.0 / 100.0).round();
    (alpha * 1000.0 / 255.0).floor() as u32
}

/// The stored per mille -> opacity percent as Fireworks shows it.
fn percent(per_mille: u32) -> f64 {
    let alpha = (per_mille.min(1000) as f64 * 255.0 / 1000.0).round();
    (alpha * 100.0 / 255.0).round()
}

impl AnimateSettings {
    /// What the dialog shows for an animation: movement and direction as whole numbers, as
    /// Fireworks MX does (40, 20 shows as Move 45, Direction 333).
    pub fn of(a: &Animation) -> Self {
        let movement = a.dx.hypot(a.dy);
        let direction = if movement > 0.0 {
            (-a.dy).atan2(a.dx).to_degrees().rem_euclid(360.0).round() % 360.0
        } else {
            0.0
        };
        AnimateSettings {
            frames: a.frames.max(1),
            movement: movement.round(),
            direction,
            scale: a.scale,
            start_opacity: percent(a.start_opacity),
            end_opacity: percent(a.end_opacity),
            rotation: a.rotation.abs(),
            ccw: a.rotation < 0.0,
        }
    }

    /// The stored animation. `was`: the animation these settings were shown for; its exact
    /// movement is kept when Move and Direction weren't changed (they are shown rounded), and
    /// likewise its opacities.
    pub fn apply(&self, was: Option<&Animation>) -> Animation {
        let shown = was.map(Self::of);
        let (dx, dy) = match (was, shown) {
            (Some(a), Some(s)) if s.movement == self.movement && s.direction == self.direction => (a.dx, a.dy),
            _ => {
                let (sin, cos) = self.direction.to_radians().sin_cos();
                (self.movement * cos, -self.movement * sin)
            }
        };
        let opacity = |now: f64, then: Option<f64>, stored: Option<u32>| match (then, stored) {
            (Some(t), Some(v)) if t == now => v,
            _ => per_mille(now),
        };
        let rotation = self.rotation.abs();
        Animation {
            frames: self.frames.clamp(1, 10_000),
            dx,
            dy,
            scale: self.scale.clamp(0.0, 10_000.0),
            rotation: if self.ccw { -rotation } else { rotation },
            start_opacity: opacity(
                self.start_opacity,
                shown.map(|s| s.start_opacity),
                was.map(|a| a.start_opacity),
            ),
            end_opacity: opacity(self.end_opacity, shown.map(|s| s.end_opacity), was.map(|a| a.end_opacity)),
        }
    }
}

impl DocState {
    /// The selected animation symbol instance (one instance of an animation symbol) and its
    /// animation.
    pub fn selected_animation(&self) -> Option<(ElementRef, Animation)> {
        let [r] = self.selection.as_slice() else { return None };
        let ElementKind::Instance { symbol, anim, .. } = &self.element(*r)?.kind else {
            return None;
        };
        let sym = self.doc.symbols.iter().find(|s| s.id == *symbol)?;
        (sym.kind == SymbolKind::Animation).then_some((*r, *anim))
    }

    /// Whether Modify > Animation > Animate Selection applies: objects selected (not inside
    /// groups, no pixel selection), and not already an animation symbol (Fireworks MX greys it
    /// out then and offers Settings instead).
    pub fn can_animate_selection(&self) -> bool {
        !self.selection.is_empty()
            && self.pixels.is_none()
            && !self.selection_nested()
            && self.selected_animation().is_none()
    }

    /// Modify > Animation > Animate Selection: the selection becomes an animation symbol (one
    /// undo step, "Convert to Symbol", as in Fireworks MX) whose instance has these settings.
    /// Returns how many frames the document lacks for the whole animation (Fireworks then asks
    /// whether to add them: [`add_frames`](Self::add_frames)).
    pub fn animate_selection(&mut self, settings: &AnimateSettings) -> Option<usize> {
        if !self.can_animate_selection() {
            return None;
        }
        self.convert_to_symbol("Symbol", SymbolKind::Animation)?;
        let r = *self.selection.first()?;
        let anim = settings.apply(None);
        if let Some(ElementKind::Instance { anim: a, .. }) = self.element_mut(r).map(|e| &mut e.kind) {
            *a = anim;
        }
        self.mark_changed();
        Some(self.frames_missing(anim.frames))
    }

    /// Modify > Animation > Settings and the Property inspector's fields: change the selected
    /// animation (one undo step, "Instance Properties" as Fireworks MX names it). Returns how
    /// many frames the document lacks for it.
    pub fn set_animation(&mut self, anim: Animation) -> usize {
        let Some((r, was)) = self.selected_animation() else { return 0 };
        if was != anim {
            self.edit("Instance Properties", |s| {
                if let Some(ElementKind::Instance { anim: a, .. }) = s.element_mut(r).map(|e| &mut e.kind) {
                    *a = anim;
                }
            });
        }
        self.frames_missing(anim.frames)
    }

    /// [`set_animation`](Self::set_animation) during a live edit (dragging a Property inspector
    /// value): no undo step of its own.
    pub fn set_animation_live(&mut self, anim: Animation) {
        let Some((r, _)) = self.selected_animation() else { return };
        if let Some(ElementKind::Instance { anim: a, .. }) = self.element_mut(r).map(|e| &mut e.kind) {
            *a = anim;
        }
        self.mark_changed();
    }

    /// Frames the document lacks for the selected animation (after a live edit).
    pub fn animation_frames_missing(&self) -> usize {
        self.selected_animation().map_or(0, |(_, a)| self.frames_missing(a.frames))
    }

    /// The selected animation's whole motion: the rectangle around every frame's placement (the
    /// Property inspector's W/H/X/Y, as Fireworks MX shows them).
    pub fn animation_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let (r, a) = self.selected_animation()?;
        let el = self.element(r)?;
        let rects: Vec<_> = (0..a.frames.max(1) as usize)
            .filter_map(|k| el.animation_step(k).bounds())
            .collect();
        let x0 = rects.iter().map(|b| b.0).fold(f64::MAX, f64::min);
        let y0 = rects.iter().map(|b| b.1).fold(f64::MAX, f64::min);
        let x1 = rects.iter().map(|b| b.0 + b.2).fold(f64::MIN, f64::max);
        let y1 = rects.iter().map(|b| b.1 + b.3).fold(f64::MIN, f64::max);
        (!rects.is_empty()).then_some((x0, y0, x1 - x0, y1 - y0))
    }

    /// The selected animation's motion path: the centre of its placement in each frame, first to
    /// last (Fireworks MX draws it green, blue..., red).
    pub fn animation_path(&self) -> Vec<(f64, f64)> {
        let Some((r, a)) = self.selected_animation() else { return Vec::new() };
        let Some(el) = self.element(r) else { return Vec::new() };
        (0..a.frames.max(1) as usize)
            .filter_map(|k| el.animation_step(k).bounds().map(|(x, y, w, h)| (x + w / 2.0, y + h / 2.0)))
            .collect()
    }

    /// Modify > Animation > Remove Animation: the symbol becomes a graphic symbol again ("Symbol
    /// Properties"). Its instances keep their settings, so turning it back into an animation
    /// symbol brings them back (Fireworks MX keeps `ANNi` and the rest on the instance).
    pub fn remove_animation(&mut self) {
        let Some((r, _)) = self.selected_animation() else { return };
        let Some(id) = self.instance_symbol(r) else { return };
        self.edit("Symbol Properties", |s| {
            if let Some(sym) = std::sync::Arc::make_mut(&mut s.doc.symbols).iter_mut().find(|x| x.id == id) {
                sym.kind = SymbolKind::Graphic;
            }
        });
    }

    /// Frames the document lacks for an animation of `frames` frames starting in the current
    /// frame.
    fn frames_missing(&self, frames: u32) -> usize {
        (self.doc.current_frame + frames.max(1) as usize).saturating_sub(self.doc.frames.len().max(1))
    }

    /// Add `n` empty frames at the end (Fireworks MX's "Add Frames"; their delay is the default
    /// 0.07 s). Layers that aren't shared get an empty cell for each.
    pub fn add_frames(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.edit("Add Frames", |s| {
            let total = s.doc.frames.len().max(1) + n;
            while s.doc.frames.len() < total {
                let k = s.doc.frames.len() + 1;
                s.doc.frames.push(Frame {
                    name: format!("Frame {k}"),
                    ..Frame::default()
                });
            }
            for layer in s.doc.layers.iter_mut().filter(|l| !l.shared) {
                if layer.cells.len() < total {
                    layer.cells.resize_with(total, Vec::new);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::Document;
    use firecraft_raster::Rgba;

    #[test]
    fn dialog_values_convert_as_fireworks_mx_stores_them() {
        // Fireworks MX: Frames 8, Move 50, Direction 30, Scale 150, Opacity 100 -> 50, Rotate 45
        // CCW stored ANNi 8, ANXf 43.30127, ANYf -25, ASCf 150, ANRi -45, AEOi 501.
        let s = AnimateSettings {
            frames: 8,
            movement: 50.0,
            direction: 30.0,
            scale: 150.0,
            end_opacity: 50.0,
            rotation: 45.0,
            ccw: true,
            ..AnimateSettings::default()
        };
        let a = s.apply(None);
        assert_eq!(a.frames, 8);
        assert!((a.dx - 43.30127).abs() < 1e-5 && (a.dy + 25.0).abs() < 1e-9, "{a:?}");
        assert_eq!((a.scale, a.rotation, a.start_opacity, a.end_opacity), (150.0, -45.0, 1000, 501));
        // And back: Fireworks shows 40, 20 as Move 45, Direction 333.
        let shown = AnimateSettings::of(&Animation {
            dx: 40.0,
            dy: 20.0,
            ..a
        });
        assert_eq!((shown.movement, shown.direction), (45.0, 333.0));
        assert_eq!((shown.end_opacity, shown.rotation, shown.ccw), (50.0, 45.0, true));
        // OK without changing Move/Direction keeps the exact movement.
        let was = Animation {
            dx: 40.0,
            dy: 20.0,
            ..a
        };
        let kept = AnimateSettings::of(&was).apply(Some(&was));
        assert_eq!((kept.dx, kept.dy), (40.0, 20.0));
        assert_eq!(kept, was);
        // 20% is stored as 200 (alpha 51).
        assert_eq!(per_mille(20.0), 200);
    }

    fn square_doc() -> DocState {
        let mut d = DocState::new(Document::new(200, 150, 72.0, None));
        d.add_element(
            firecraft_doc::Element::new(ElementKind::Bitmap(firecraft_doc::Bitmap {
                x: 20,
                y: 20,
                image: firecraft_raster::RgbaImage::new(20, 20, Rgba::new(255, 0, 0, 255)),
            })),
            "Add",
        );
        d
    }

    #[test]
    fn animate_selection_makes_an_animation_symbol_and_asks_for_frames() {
        let mut d = square_doc();
        assert!(d.can_animate_selection());
        let missing = d.animate_selection(&AnimateSettings {
            frames: 8,
            ..AnimateSettings::default()
        });
        assert_eq!(missing, Some(7), "one frame, the animation needs eight");
        assert_eq!(d.history.next_undo(), Some("Convert to Symbol"));
        let (r, a) = d.selected_animation().expect("an animation symbol");
        assert_eq!((a.frames, a.dx, a.dy), (8, 72.0, 0.0));
        assert!(!d.can_animate_selection(), "Settings instead");
        d.add_frames(7);
        assert_eq!(d.history.next_undo(), Some("Add Frames"));
        assert_eq!(d.doc.frames.len(), 8);
        assert!(d.doc.layers.iter().filter(|l| !l.shared).all(|l| l.cells.len() == 8));
        // The new frames are empty: only the animation shows there.
        assert_eq!(d.doc.layers[r.layer].cells[3].len(), 0);
        // Settings / Property inspector.
        let missing = d.set_animation(Animation { frames: 10, ..a });
        assert_eq!(missing, 2);
        assert_eq!(d.history.next_undo(), Some("Instance Properties"));
        // Remove Animation: a graphic symbol, the instance keeps its settings.
        d.remove_animation();
        assert_eq!(d.history.next_undo(), Some("Symbol Properties"));
        assert!(d.selected_animation().is_none());
        let Some(ElementKind::Instance { anim, .. }) = d.element(r).map(|e| &e.kind) else {
            panic!("not an instance")
        };
        assert_eq!(anim.frames, 10);
        assert!(d.can_animate_selection());
    }

    #[test]
    fn the_inspector_shows_the_whole_motion() {
        // Fireworks MX: a 20 x 20 square moved 40, 20 over its frames shows W 60, H 40.
        let mut d = square_doc();
        d.animate_selection(&AnimateSettings {
            frames: 5,
            movement: 40f64.hypot(20.0),
            direction: (-20f64).atan2(40.0).to_degrees(),
            ..AnimateSettings::default()
        });
        let (x, y, w, h) = d.animation_bounds().expect("bounds");
        assert!((x - 20.0).abs() < 1e-6 && (y - 20.0).abs() < 1e-6, "{x} {y}");
        assert!((w - 60.0).abs() < 1e-6 && (h - 40.0).abs() < 1e-6, "{w} {h}");
        let path = d.animation_path();
        assert_eq!(path.len(), 5);
        assert_eq!(path[0], (30.0, 30.0));
        assert_eq!(path[4], (70.0, 50.0));
    }
}
