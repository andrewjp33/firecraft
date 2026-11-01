//! FireCraft's tool cursors.
//!
//! Like Fireworks, which shipped its own cursors (a black Pointer, a white Subselection arrow,
//! ...), FireCraft draws its tool cursors itself so they look the same on every system:
//! [`CursorStyle::Fireworks`], the default. They are bitmaps handed to the OS through egui's
//! custom cursor support (no lag, unlike cursors painted into the frame), sized to the system
//! cursor size (`XCURSOR_SIZE`, GNOME's `cursor-size`, else 24; 32 px bitmaps on Windows) so
//! they sit with other applications' cursors. [`CursorStyle::System`] uses the system's own
//! cursors where one exists; its rotate cursor follows the theme's dark/light style (a theme
//! name with "white", "snow" or "light" is light).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Preferences > Tool cursors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorStyle {
    #[default]
    Fireworks,
    System,
}

impl CursorStyle {
    pub const ALL: [CursorStyle; 2] = [CursorStyle::Fireworks, CursorStyle::System];

    pub fn label(self) -> &'static str {
        match self {
            CursorStyle::Fireworks => "Fireworks (FireCraft's own)",
            CursorStyle::System => "System",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            CursorStyle::Fireworks => "fireworks",
            CursorStyle::System => "system",
        }
    }

    pub fn from_key(s: &str) -> Self {
        if s == "system" {
            CursorStyle::System
        } else {
            CursorStyle::Fireworks
        }
    }
}

/// The cursors FireCraft provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cur {
    /// Black arrow.
    Pointer,
    /// White arrow.
    Subselect,
    /// Black arrow with a small square behind it.
    SelectBehind,
    Move,
    /// Double arrows: horizontal, vertical, NW-SE, NE-SW.
    ResizeH,
    ResizeV,
    ResizeNwSe,
    ResizeNeSw,
    Rotate,
    Crosshair,
}

impl Cur {
    /// The system cursor closest to this one.
    fn system(self) -> egui::CursorIcon {
        match self {
            Cur::Pointer | Cur::Subselect | Cur::SelectBehind => egui::CursorIcon::Default,
            Cur::Move => egui::CursorIcon::Move,
            Cur::ResizeH => egui::CursorIcon::ResizeHorizontal,
            Cur::ResizeV => egui::CursorIcon::ResizeVertical,
            Cur::ResizeNwSe => egui::CursorIcon::ResizeNwSe,
            Cur::ResizeNeSw => egui::CursorIcon::ResizeNeSw,
            Cur::Rotate => egui::CursorIcon::Default,
            Cur::Crosshair => egui::CursorIcon::Crosshair,
        }
    }
}

/// Show `cur` for this frame. Returns true when the caller must paint a rotate cursor itself
/// (integrations without bitmap cursors: the web).
pub fn set(ctx: &egui::Context, cur: Cur, style: CursorStyle) -> bool {
    let bitmaps = !cfg!(target_arch = "wasm32");
    match (style, bitmaps) {
        (_, false) if cur == Cur::Rotate => {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            true
        }
        (_, false) | (CursorStyle::System, true) if cur != Cur::Rotate => {
            ctx.set_cursor_icon(cur.system());
            false
        }
        _ => {
            let (theme_dark, size) = system_style();
            // Fireworks style: black cursors (white for Subselect); System: the theme's tone.
            let dark = style == CursorStyle::Fireworks || theme_dark;
            ctx.set_cursor_icon(cur.system());
            ctx.set_cursor_image(Some(image(cur, dark, size)));
            false
        }
    }
}

/// The system cursor style: (dark, nominal size in pixels). Looked up once.
fn system_style() -> (bool, u32) {
    static STYLE: OnceLock<(bool, u32)> = OnceLock::new();
    *STYLE.get_or_init(|| {
        if cfg!(windows) {
            return (false, 32);
        }
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let theme = env("XCURSOR_THEME")
            .or_else(|| gsetting("cursor-theme"))
            .unwrap_or_default()
            .to_lowercase();
        let light = ["white", "snow", "light"].iter().any(|w| theme.contains(w));
        let size = env("XCURSOR_SIZE")
            .or_else(|| gsetting("cursor-size"))
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|s| (12..=256).contains(s))
            .unwrap_or(24);
        (!light, size)
    })
}

/// `gsettings get org.gnome.desktop.interface <key>`, unquoted (Linux desktops only).
fn gsetting(key: &str) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", key])
        .output()
        .ok()?;
    let v = String::from_utf8(out.stdout).ok()?;
    let v = v.trim().trim_matches('\'').to_owned();
    (!v.is_empty()).then_some(v)
}

/// The bitmap for a cursor (cached: the same `Arc` each time, which egui-winit dedupes on).
pub fn image(cur: Cur, dark: bool, size: u32) -> egui::CustomCursorImage {
    static CACHE: OnceLock<Mutex<HashMap<(Cur, bool, u32), (Arc<[u8]>, [u16; 2])>>> = OnceLock::new();
    let n = size.clamp(12, 256);
    // Windows draws its 32 px cursors at about the size of a 24 px X cursor.
    let scale = if cfg!(windows) { 1.0 } else { n as f32 / 24.0 };
    let mut map = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let (rgba, hot) = map
        .entry((cur, dark, n))
        .or_insert_with(|| {
            let (parts, hot, light_fill) = design(cur);
            let fill_dark = if light_fill { !dark } else { dark };
            (
                raster(&parts, n as usize, scale, fill_dark).into(),
                [(hot.0 * scale).round() as u16, (hot.1 * scale).round() as u16],
            )
        })
        .clone();
    egui::CustomCursorImage {
        rgba,
        size: [n as u16, n as u16],
        hotspot: hot,
    }
}

/// A shape on the 24-unit design grid.
enum Part {
    Poly(Vec<(f32, f32)>),
    /// Centre, radius, half thickness, start and end angle (radians, y down).
    Arc((f32, f32), f32, f32, f32, f32),
}

/// Shapes, hotspot and whether the fill is the light colour (Subselect) for a cursor.
fn design(cur: Cur) -> (Vec<Part>, (f32, f32), bool) {
    let arrow = |dx: f32, dy: f32| {
        Part::Poly(
            [
                (0.0, 0.0),
                (0.0, 16.0),
                (4.0, 12.5),
                (6.8, 19.0),
                (9.4, 17.9),
                (6.7, 11.6),
                (11.5, 11.6),
            ]
            .iter()
            .map(|&(x, y)| (x + dx, y + dy))
            .collect(),
        )
    };
    // A double arrow along `angle` through (12, 12), 18 units long.
    let double = |angle: f32| {
        let pts = [
            (-9.0, 0.0),
            (-4.5, -4.5),
            (-4.5, -1.6),
            (4.5, -1.6),
            (4.5, -4.5),
            (9.0, 0.0),
            (4.5, 4.5),
            (4.5, 1.6),
            (-4.5, 1.6),
            (-4.5, 4.5),
        ];
        let (s, c) = angle.to_radians().sin_cos();
        Part::Poly(
            pts.iter()
                .map(|&(x, y)| (12.0 + x * c - y * s, 12.0 + x * s + y * c))
                .collect(),
        )
    };
    let bar = |x0: f32, y0: f32, x1: f32, y1: f32| Part::Poly(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    match cur {
        Cur::Pointer => (vec![arrow(2.0, 2.0)], (2.0, 2.0), false),
        Cur::Subselect => (vec![arrow(2.0, 2.0)], (2.0, 2.0), true),
        Cur::SelectBehind => (
            vec![bar(12.0, 14.0, 19.0, 21.0), arrow(2.0, 2.0)],
            (2.0, 2.0),
            false,
        ),
        Cur::Move => (vec![double(0.0), double(90.0)], (12.0, 12.0), false),
        Cur::ResizeH => (vec![double(0.0)], (12.0, 12.0), false),
        Cur::ResizeV => (vec![double(90.0)], (12.0, 12.0), false),
        Cur::ResizeNwSe => (vec![double(45.0)], (12.0, 12.0), false),
        Cur::ResizeNeSw => (vec![double(135.0)], (12.0, 12.0), false),
        Cur::Crosshair => (
            vec![
                bar(3.0, 11.1, 9.5, 12.9),
                bar(14.5, 11.1, 21.0, 12.9),
                bar(11.1, 3.0, 12.9, 9.5),
                bar(11.1, 14.5, 12.9, 21.0),
            ],
            (12.0, 12.0),
            false,
        ),
        Cur::Rotate => {
            let (c, r, a1) = ((12.0, 12.0), 6.2, 52f32.to_radians());
            let end = (c.0 + r * a1.cos(), c.1 + r * a1.sin());
            let (t, n) = ((-a1.sin(), a1.cos()), (a1.cos(), a1.sin()));
            let head = Part::Poly(vec![
                (end.0 + 4.2 * t.0, end.1 + 4.2 * t.1),
                (end.0 + 3.8 * n.0 - 1.2 * t.0, end.1 + 3.8 * n.1 - 1.2 * t.1),
                (end.0 - 3.8 * n.0 - 1.2 * t.0, end.1 - 3.8 * n.1 - 1.2 * t.1),
            ]);
            (
                vec![Part::Arc(c, r, 1.05, (-200f32).to_radians(), a1), head],
                (12.0, 12.0),
                false,
            )
        }
    }
}

/// Signed distance from (x, y) to a part (negative inside).
fn dist(p: &Part, x: f32, y: f32) -> f32 {
    match p {
        Part::Arc(c, r, half, a0, a1) => {
            let (dx, dy) = (x - c.0, y - c.1);
            let a = a0 + (dy.atan2(dx) - a0).rem_euclid(std::f32::consts::TAU);
            let d = if a <= *a1 {
                ((dx * dx + dy * dy).sqrt() - r).abs()
            } else {
                let (p0, p1) = (
                    (c.0 + r * a0.cos(), c.1 + r * a0.sin()),
                    (c.0 + r * a1.cos(), c.1 + r * a1.sin()),
                );
                (x - p0.0).hypot(y - p0.1).min((x - p1.0).hypot(y - p1.1))
            };
            d - half
        }
        Part::Poly(pts) => {
            // Distance to the outline, negative when inside (crossing test).
            let mut inside = false;
            let mut best = f32::MAX;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                    inside = !inside;
                }
                let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                let t = (((x - a.0) * ex + (y - a.1) * ey) / (ex * ex + ey * ey).max(1e-9)).clamp(0.0, 1.0);
                best = best.min((x - a.0 - t * ex).hypot(y - a.1 - t * ey));
            }
            if inside { -best } else { best }
        }
    }
}

/// Rasterise parts (design units x `scale`) into an `n` x `n` RGBA bitmap, anti-aliased:
/// fill black with a white outline (`dark`) or white with a black outline.
fn raster(parts: &[Part], n: usize, scale: f32, dark: bool) -> Vec<u8> {
    let outline = 1.0;
    let (fill, edge) = if dark { (0u8, 255u8) } else { (255u8, 0u8) };
    let mut out = vec![0u8; n * n * 4];
    const SS: usize = 4;
    for py in 0..n {
        for px in 0..n {
            let (mut body, mut rim) = (0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / scale;
                    let d = parts.iter().map(|p| dist(p, x, y)).fold(f32::MAX, f32::min) * scale;
                    if d <= 0.0 {
                        body += 1.0;
                    } else if d <= outline {
                        rim += 1.0;
                    }
                }
            }
            let cover = (body + rim) / (SS * SS) as f32;
            if cover > 0.0 {
                let v = ((fill as f32 * body + edge as f32 * rim) / (body + rim)).round() as u8;
                if let Some(p) = out.get_mut((py * n + px) * 4..(py * n + px) * 4 + 4) {
                    p.copy_from_slice(&[v, v, v, (cover * 255.0).round() as u8]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(img: &egui::CustomCursorImage, x: usize, y: usize) -> [u8; 2] {
        let n = img.size[0] as usize;
        [img.rgba[(y * n + x) * 4], img.rgba[(y * n + x) * 4 + 3]]
    }

    #[test]
    fn fireworks_cursor_set() {
        // Pointer: black inside the arrow; Subselect: white.
        assert_eq!(at(&image(Cur::Pointer, true, 24), 4, 10), [0, 255]);
        assert_eq!(at(&image(Cur::Subselect, true, 24), 4, 10), [255, 255]);
        // Rotate ring at radius 6.2 left of the centre: black.
        assert_eq!(at(&image(Cur::Rotate, true, 24), 5, 12), [0, 255]);
        // Sized like a 24 px theme arrow (13 x 20).
        let p = image(Cur::Pointer, true, 24);
        let rows: Vec<usize> = (0..24 * 24)
            .filter(|i| p.rgba[i * 4 + 3] > 40)
            .map(|i| i / 24)
            .collect();
        let h = rows.iter().max().unwrap() - rows.iter().min().unwrap() + 1;
        assert!((18..=23).contains(&h), "height {h}");
        // Scales with the system size; cached (same Arc).
        assert_eq!(image(Cur::Move, true, 48).size, [48, 48]);
        assert!(Arc::ptr_eq(
            &image(Cur::Move, true, 24).rgba,
            &image(Cur::Move, true, 24).rgba
        ));
    }
}
