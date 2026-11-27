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
    /// A small white arrowhead: the Subselection tool over a path's point or handle, or
    /// dragging one (Fireworks MX).
    SubselectPoint,
    /// Black arrow with a small square behind it.
    SelectBehind,
    /// Four-way arrow (Fireworks MX's move cursor).
    Move,
    /// White arrow over a small dashed box: dragging a marquee or wand selection (Fireworks MX).
    MovePixels,
    /// Double arrows: horizontal, vertical, NW-SE, NE-SW.
    ResizeH,
    ResizeV,
    ResizeNwSe,
    ResizeNeSw,
    Rotate,
    Crosshair,
    /// The Crop tool: two interlocking corner brackets.
    Crop,
    /// The marquee tools: a thin cross, with a badge for adding (Shift), subtracting (Alt) or
    /// intersecting (Shift+Alt). The vector shape tools (Line, Rectangle, Ellipse) show it
    /// plain.
    Marquee(Badge),
    /// The Magic Wand: a wand with a sparkle at its tip, the hotspot; badges as the marquee's.
    MagicWand(Badge),
    /// The Pen: a nib pointing up, the hotspot at its tip, with a badge for what a click does.
    Pen(Badge),
}

/// A small mark at a cursor's side saying what a click will do (in the style of Fireworks').
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Badge {
    None,
    /// Add to the selection; the Pen: add a point on the segment.
    Plus,
    /// Subtract from the selection; the Pen: delete the point.
    Minus,
    /// Intersect with the selection; the Pen: start a new path.
    Cross,
    /// The Pen: close the path (on its first point).
    Ring,
    /// The Pen: convert a curve point, or carry on from an end point.
    Caret,
    /// The Pen: join another path's end.
    Join,
}

impl Badge {
    /// The selection tools' badge for the modifiers held: Shift adds, Alt subtracts, both
    /// intersect.
    pub fn for_selection(mods: egui::Modifiers) -> Self {
        match (mods.shift, mods.alt) {
            (true, true) => Badge::Cross,
            (true, false) => Badge::Plus,
            (false, true) => Badge::Minus,
            (false, false) => Badge::None,
        }
    }
}

impl Cur {
    /// The system cursor closest to this one.
    fn system(self) -> egui::CursorIcon {
        match self {
            Cur::Pointer | Cur::Subselect | Cur::SubselectPoint | Cur::SelectBehind => egui::CursorIcon::Default,
            Cur::Move | Cur::MovePixels => egui::CursorIcon::Move,
            Cur::ResizeH => egui::CursorIcon::ResizeHorizontal,
            Cur::ResizeV => egui::CursorIcon::ResizeVertical,
            Cur::ResizeNwSe => egui::CursorIcon::ResizeNwSe,
            Cur::ResizeNeSw => egui::CursorIcon::ResizeNeSw,
            Cur::Rotate => egui::CursorIcon::Default,
            Cur::Crosshair | Cur::Crop | Cur::Marquee(_) | Cur::MagicWand(_) | Cur::Pen(_) => {
                egui::CursorIcon::Crosshair
            }
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
            let Design { parts, accents, backdrop, hot, light } = design(cur);
            let fill_dark = if light { !dark } else { dark };
            (
                raster(&parts, &accents, &backdrop, n as usize, scale, fill_dark).into(),
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

/// A cursor's drawing: its shapes, filled black with a white edge (or, `light`, white with a
/// black edge, as Subselect), accents on top in the other tone, and the hotspot.
struct Design {
    parts: Vec<Part>,
    accents: Vec<Part>,
    /// Under everything, solid in the accents' edge tone (white for black accents): a shape and
    /// how far (screen pixels) the solid reaches past it, a halo.
    backdrop: Vec<(Part, f32)>,
    hot: (f32, f32),
    light: bool,
}

/// The drawing of each cursor. Shapes meant to be 1 unit thick run along pixel centres
/// (x.5), so they stay crisp at the 24 px size; the hotspot is then that pixel.
fn design(cur: Cur) -> Design {
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
    // Arrows out from the hotspot pixel's centre along each unit direction in `dirs`: thin
    // shafts with small solid heads (the move cursor four ways, the resize cursors two).
    let arms = |dirs: &[(f32, f32)]| {
        const REACH: f32 = 7.5; // centre to tip
        const SHAFT: f32 = 0.5; // half thickness: 1 px
        const HEAD: f32 = 3.0; // head length
        const WIDE: f32 = 2.5; // head half width
        // Diagonal heads are right-angled, their legs along the pixel axes, so they stay crisp.
        const CORNER: f32 = 6.5; // centre to a diagonal head's tip, along each axis
        const LEG: f32 = 5.0;
        let c = (12.5, 12.5);
        let mut parts = Vec::new();
        for &(ux, uy) in dirs {
            if ux != 0.0 && uy != 0.0 {
                let (sx, sy) = (ux.signum(), uy.signum());
                let tip = (c.0 + sx * CORNER, c.1 + sy * CORNER);
                parts.push(Part::Poly(vec![tip, (tip.0 - sx * LEG, tip.1), (tip.0, tip.1 - sy * LEG)]));
                let (h, w) = (SHAFT * std::f32::consts::SQRT_2, CORNER - 1.5);
                parts.push(Part::Poly(vec![
                    (c.0 - h / 2.0 * sx, c.1 + h / 2.0 * sy),
                    (c.0 + (w - h / 2.0) * sx, c.1 + (w + h / 2.0) * sy),
                    (c.0 + (w + h / 2.0) * sx, c.1 + (w - h / 2.0) * sy),
                    (c.0 + h / 2.0 * sx, c.1 - h / 2.0 * sy),
                ]));
                continue;
            }
            let at = |along: f32, across: f32| (c.0 + ux * along - uy * across, c.1 + uy * along + ux * across);
            parts.push(Part::Poly(vec![at(0.0, -SHAFT), at(REACH - HEAD, -SHAFT), at(REACH - HEAD, SHAFT), at(0.0, SHAFT)]));
            parts.push(Part::Poly(vec![at(REACH, 0.0), at(REACH - HEAD, -WIDE), at(REACH - HEAD, WIDE)]));
        }
        parts
    };
    let d = std::f32::consts::FRAC_1_SQRT_2;
    let bar = |x0: f32, y0: f32, x1: f32, y1: f32| Part::Poly(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    // A straight band `half` either side of the line from `a` to `b`.
    let line = |a: (f32, f32), b: (f32, f32), half: f32| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy).max(1e-6);
        let (nx, ny) = (-dy / len * half, dx / len * half);
        Part::Poly(vec![(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)])
    };
    // A badge, 5 pixels across, centred on the pixel at (`x`, `y`): whole pixels, so its small
    // diagonals stay crisp at 24 px (they scale with the cursor).
    let badge = |b: Badge, x: f32, y: f32| -> Vec<Part> {
        let px = |dx: f32, dy: f32| bar(x + dx, y + dy, x + dx + 1.0, y + dy + 1.0);
        let across = bar(x - 2.0, y, x + 3.0, y + 1.0);
        match b {
            Badge::None => Vec::new(),
            Badge::Plus => vec![across, bar(x, y - 2.0, x + 1.0, y + 3.0)],
            Badge::Minus => vec![across],
            Badge::Cross => (-2..=2).flat_map(|k| [px(k as f32, k as f32), px(k as f32, -k as f32)]).collect(),
            // A small round ring: a 5 x 5 square without its corners.
            Badge::Ring => vec![
                bar(x - 1.0, y - 2.0, x + 2.0, y - 1.0),
                bar(x - 1.0, y + 2.0, x + 2.0, y + 3.0),
                bar(x - 2.0, y - 1.0, x - 1.0, y + 2.0),
                bar(x + 2.0, y - 1.0, x + 3.0, y + 2.0),
            ],
            // An upturned V, 5 wide and 3 high.
            Badge::Caret => vec![px(0.0, -1.0), px(-1.0, 0.0), px(1.0, 0.0), px(-2.0, 1.0), px(2.0, 1.0)],
            // A line into a small ring.
            Badge::Join => vec![
                bar(x - 3.0, y, x, y + 1.0),
                bar(x + 1.0, y - 1.0, x + 2.0, y),
                bar(x + 1.0, y + 1.0, x + 2.0, y + 2.0),
                px(0.0, 0.0),
                px(2.0, 0.0),
            ],
        }
    };
    // A short arrowhead, no tail, its tip at (2, 2) (the hotspot).
    let small_head = || Part::Poly(vec![(2.0, 2.0), (2.0, 13.0), (5.0, 10.0), (10.0, 10.0)]);
    let plain = |parts: Vec<Part>, hot: (f32, f32)| Design {
        parts,
        accents: Vec::new(),
        backdrop: Vec::new(),
        hot,
        light: false,
    };
    match cur {
        Cur::Pointer => plain(vec![arrow(2.0, 2.0)], (2.0, 2.0)),
        Cur::Subselect => Design {
            light: true,
            ..plain(vec![arrow(2.0, 2.0)], (2.0, 2.0))
        },
        // A short white arrowhead (no tail) in a white halo, the hotspot at its tip.
        Cur::SubselectPoint => Design {
            parts: vec![small_head()],
            accents: Vec::new(),
            backdrop: vec![(small_head(), 2.0)],
            hot: (2.0, 2.0),
            light: true,
        },
        Cur::SelectBehind => plain(vec![bar(12.0, 14.0, 19.0, 21.0), arrow(2.0, 2.0)], (2.0, 2.0)),
        Cur::ResizeH => plain(arms(&[(1.0, 0.0), (-1.0, 0.0)]), (12.0, 12.0)),
        Cur::ResizeV => plain(arms(&[(0.0, 1.0), (0.0, -1.0)]), (12.0, 12.0)),
        Cur::ResizeNwSe => plain(arms(&[(d, d), (-d, -d)]), (12.0, 12.0)),
        Cur::ResizeNeSw => plain(arms(&[(d, -d), (-d, d)]), (12.0, 12.0)),
        Cur::Crosshair => plain(
            vec![
                bar(3.0, 11.1, 9.5, 12.9),
                bar(14.5, 11.1, 21.0, 12.9),
                bar(11.1, 3.0, 12.9, 9.5),
                bar(11.1, 14.5, 12.9, 21.0),
            ],
            (12.0, 12.0),
        ),
        // The marquee tools: a thin 1 px cross through the hotspot pixel's centre.
        // A badge sits below right of the cross.
        Cur::Marquee(b) => {
            const ARM: f32 = 5.5; // centre to each end
            let c = 12.5;
            let mut parts = vec![bar(c - ARM, c - 0.5, c + ARM, c + 0.5), bar(c - 0.5, c - ARM, c + 0.5, c + ARM)];
            parts.extend(badge(b, 19.0, 19.0));
            plain(parts, (12.0, 12.0))
        }
        // The Magic Wand: a stick down to the right from a small sparkle (a dot with four short
        // rays); the hotspot is the dot.
        // A badge sits below the sparkle, left of the stick.
        Cur::MagicWand(b) => {
            const S: f32 = 7.5; // the sparkle's centre (both axes)
            const RAY: (f32, f32) = (2.5, 4.5); // its rays, from and to (a dot in the middle)
            const STICK: (f32, f32) = (3.5, 12.5); // the stick along the diagonal, from and to
            let mut parts = vec![
                bar(S - 0.5, S - RAY.1, S + 0.5, S - RAY.0),
                bar(S - 0.5, S + RAY.0, S + 0.5, S + RAY.1),
                bar(S - RAY.1, S - 0.5, S - RAY.0, S + 0.5),
                bar(S + RAY.0, S - 0.5, S + RAY.1, S + 0.5),
            ];
            parts.push(bar(S - 0.5, S - 0.5, S + 0.5, S + 0.5));
            parts.push(line((S + STICK.0, S + STICK.0), (S + STICK.1, S + STICK.1), 1.1));
            parts.extend(badge(b, 4.0, 17.0));
            plain(parts, (S - 0.5, S - 0.5))
        }
        // The Pen: a white nib, outlined black (and haloed white), pointing up to the hotspot,
        // a slit down its middle and a black collar below; a badge below right of it.
        Cur::Pen(b) => {
            const X: f32 = 6.5; // the nib's middle (a pixel centre: the slit stays crisp)
            const TIP: f32 = 2.0;
            const WIDE: f32 = 3.5; // half width at the shoulders
            const SHOULDER: f32 = 8.5;
            const BASE: f32 = 12.0; // where the collar starts
            const COLLAR: f32 = 4.0; // its height
            let nib = || {
                Part::Poly(vec![
                    (X, TIP),
                    (X + WIDE, SHOULDER),
                    (X + WIDE - 1.0, BASE),
                    (X - WIDE + 1.0, BASE),
                    (X - WIDE, SHOULDER),
                ])
            };
            let mut accents = vec![
                bar(X - 0.5, TIP + 2.0, X + 0.5, SHOULDER + 0.5),
                bar(X - WIDE, BASE, X + WIDE, BASE + COLLAR),
            ];
            accents.extend(badge(b, 15.0, 17.0));
            // A white halo past the nib's black outline, so it shows on dark pixels too.
            Design {
                parts: vec![nib()],
                accents,
                backdrop: vec![(nib(), 2.0)],
                hot: (X - 0.5, TIP - 1.0),
                light: true,
            }
        }
        // Four-way arrow in the style of Fireworks' move cursor: 1-unit shafts with small
        // solid heads, centred on the hotspot pixel.
        Cur::Move => plain(arms(&[(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]), (12.0, 12.0)),
        // Dragging a selection: a small white arrow over a white box with a black dashed edge,
        // all in a white halo so it reads on dark pixels too (in the style of Fireworks').
        Cur::MovePixels => {
            const L: f32 = 8.0; // the box: left, top, right, bottom
            const T: f32 = 12.0;
            const R: f32 = 16.0;
            const B: f32 = 17.0;
            let mut accents = Vec::new();
            for (x0, x1) in [(L, L + 2.0), (L + 3.0, R - 3.0), (R - 2.0, R)] {
                accents.push(bar(x0, T, x1, T + 1.0));
                accents.push(bar(x0, B - 1.0, x1, B));
            }
            let mid = (T + B) / 2.0;
            accents.push(bar(L, mid - 0.5, L + 1.0, mid + 0.5));
            accents.push(bar(R - 1.0, mid - 0.5, R, mid + 0.5));
            // The short arrowhead, its lower right clear of the box; its black edge gets a white
            // halo past it.
            Design {
                parts: vec![small_head()],
                accents,
                backdrop: vec![(bar(L, T, R, B), 0.0), (small_head(), 2.0)],
                hot: (2.0, 2.0),
                light: true,
            }
        }
        // Crop: two interlocking corner brackets (the usual crop symbol, as Fireworks'), with
        // a thin diagonal tick across the frame they make; the hotspot is the tick's middle.
        Cur::Crop => {
            const X: f32 = 6.0; // outer left / top of the top-left bracket
            const Y: f32 = 7.0;
            const T: f32 = 2.0; // thickness
            const A: f32 = 6.0; // the frame's inside
            const E: f32 = 3.0; // how far each bracket runs past the frame
            let (fx, fy) = (X + T, Y + T); // the frame's inside, top-left
            let parts = vec![
                bar(X, Y, X + T, Y + T + A + T + E),
                bar(X, Y, X + T + A + T + E, Y + T),
                bar(fx + A, Y - E, fx + A + T, fy + A + T),
                bar(X - E, fy + A, fx + A + T, fy + A + T),
                line((fx + 1.5, fy + 1.5), (fx + A - 0.5, fy + A - 0.5), 0.5),
            ];
            plain(parts, (fx + A / 2.0, fy + A / 2.0))
        }
        Cur::Rotate => {
            let (c, r, a1) = ((12.0, 12.0), 6.2, 52f32.to_radians());
            let end = (c.0 + r * a1.cos(), c.1 + r * a1.sin());
            let (t, n) = ((-a1.sin(), a1.cos()), (a1.cos(), a1.sin()));
            let head = Part::Poly(vec![
                (end.0 + 4.2 * t.0, end.1 + 4.2 * t.1),
                (end.0 + 3.8 * n.0 - 1.2 * t.0, end.1 + 3.8 * n.1 - 1.2 * t.1),
                (end.0 - 3.8 * n.0 - 1.2 * t.0, end.1 - 3.8 * n.1 - 1.2 * t.1),
            ]);
            plain(vec![Part::Arc(c, r, 1.05, (-200f32).to_radians(), a1), head], (12.0, 12.0))
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
/// fill black with a white outline (`dark`) or white with a black outline; `accents` go on top
/// in the other tone, the backdrop under everything in the accents' edge tone.
fn raster(parts: &[Part], accents: &[Part], backdrop: &[(Part, f32)], n: usize, scale: f32, dark: bool) -> Vec<u8> {
    let outline = 1.0;
    let (fill, edge) = if dark { (0.0f32, 255.0f32) } else { (255.0, 0.0) };
    let nearest = |set: &[Part], x: f32, y: f32| set.iter().map(|p| dist(p, x, y)).fold(f32::MAX, f32::min) * scale;
    let mut out = vec![0u8; n * n * 4];
    const SS: usize = 4;
    for py in 0..n {
        for px in 0..n {
            let (mut sum, mut covered) = (0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / scale;
                    let (d, a) = (nearest(parts, x, y), nearest(accents, x, y));
                    let k = backdrop
                        .iter()
                        .map(|(p, halo)| dist(p, x, y) * scale - halo)
                        .fold(f32::MAX, f32::min);
                    let v = if a <= 0.0 {
                        edge
                    } else if d <= 0.0 {
                        fill
                    } else if d.min(a) <= outline {
                        if a < d { fill } else { edge }
                    } else if k <= 0.0 {
                        fill
                    } else {
                        continue;
                    };
                    sum += v;
                    covered += 1.0;
                }
            }
            if covered > 0.0 {
                let v = (sum / covered).round() as u8;
                if let Some(p) = out.get_mut((py * n + px) * 4..(py * n + px) * 4 + 4) {
                    p.copy_from_slice(&[v, v, v, (covered / (SS * SS) as f32 * 255.0).round() as u8]);
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

    #[test]
    fn crop_cursor() {
        // Two 2 px brackets around a 6 px frame, the hotspot on the tick across it.
        let img = image(Cur::Crop, true, 24);
        assert_eq!(img.hotspot, [11, 12]);
        assert!(at(&img, 11, 12)[0] < 90, "the tick, under the hotspot");
        assert_eq!(at(&img, 6, 12), [0, 255], "the left bracket");
        assert_eq!(at(&img, 15, 12), [0, 255], "the right bracket");
        assert_eq!(at(&img, 10, 13)[0], 255, "white beside the tick");
    }

    #[test]
    fn move_cursors() {
        // Four-way arrow: crisp 1 px shafts crossing at the hotspot.
        let img = image(Cur::Move, true, 24);
        assert_eq!(img.hotspot, [12, 12]);
        assert_eq!(at(&img, 12, 12), [0, 255]);
        assert_eq!(at(&img, 9, 12), [0, 255], "the horizontal shaft");
        assert_eq!(at(&img, 9, 11), [255, 255], "white beside it: 1 px thick");
        assert_eq!(at(&img, 9, 13), [255, 255]);
        assert_eq!(at(&img, 12, 6), [0, 255], "the top arrowhead");
        // Over a selection: a white arrow, black dashes below it, hotspot at the tip.
        let img = image(Cur::MovePixels, true, 24);
        assert_eq!(img.hotspot, [2, 2]);
        assert_eq!(at(&img, 4, 8), [255, 255], "the arrow is white inside");
        assert_eq!(at(&img, 15, 12), [0, 255], "a dash of the box");
        assert_eq!(at(&img, 13, 16), [255, 255], "a gap between dashes");
        assert_eq!(at(&img, 12, 14), [255, 255], "the box is white inside");
        assert_eq!(at(&img, 0, 6), [255, 255], "a white halo outside the arrow's black edge");
        // The marquee cross: 1 px, the hotspot where the arms cross.
        let img = image(Cur::Marquee(Badge::None), true, 24);
        assert_eq!(img.hotspot, [12, 12]);
        assert_eq!(at(&img, 12, 12), [0, 255]);
        assert_eq!(at(&img, 8, 12), [0, 255]);
        assert_eq!(at(&img, 8, 11), [255, 255], "1 px arm");
        // The wand: the hotspot on the sparkle's dot, the stick down to the right.
        let img = image(Cur::MagicWand(Badge::None), true, 24);
        assert_eq!(img.hotspot, [7, 7]);
        assert_eq!(at(&img, 7, 7), [0, 255]);
        assert_eq!(at(&img, 15, 15), [0, 255], "the stick");
        // Over a path's point: the small arrowhead alone, white inside, its tip the hotspot.
        let img = image(Cur::SubselectPoint, true, 24);
        assert_eq!(img.hotspot, [2, 2]);
        assert_eq!(at(&img, 3, 6), [255, 255]);
        assert_eq!(at(&img, 12, 14)[1], 0, "no box");
        // Badges: the marquee's below right of the cross, the wand's below the sparkle, the
        // Pen's below right of the nib; the hotspot doesn't move.
        let plus = image(Cur::Marquee(Badge::Plus), true, 24);
        assert_eq!(plus.hotspot, [12, 12]);
        assert_eq!(at(&plus, 19, 19), [0, 255], "the plus's middle");
        assert_eq!(at(&plus, 19, 17), [0, 255], "its upright");
        let minus = image(Cur::Marquee(Badge::Minus), true, 24);
        assert_eq!(at(&minus, 19, 17)[1], 0, "a minus has no upright");
        let cross = image(Cur::Marquee(Badge::Cross), true, 24);
        assert_eq!(at(&cross, 17, 17), [0, 255]);
        assert_eq!(at(&cross, 21, 17), [0, 255]);
        assert_eq!(at(&image(Cur::MagicWand(Badge::Minus), true, 24), 4, 17), [0, 255]);
        let pen = image(Cur::Pen(Badge::None), true, 24);
        assert_eq!(pen.hotspot, [6, 1]);
        assert_eq!(at(&pen, 6, 1), [0, 255], "the nib's tip");
        assert_eq!(at(&pen, 5, 9)[0], 255, "white inside the nib");
        assert_eq!(at(&pen, 6, 13), [0, 255], "the collar");
        let ring = image(Cur::Pen(Badge::Ring), true, 24);
        assert_eq!(at(&ring, 15, 15), [0, 255], "the ring's top");
        assert_ne!(at(&ring, 15, 17), [0, 255], "open in its middle");
        // The resize arrows are as thin as the move cursor's.
        let img = image(Cur::ResizeH, true, 24);
        assert_eq!(at(&img, 9, 12), [0, 255]);
        assert_eq!(at(&img, 9, 11), [255, 255], "1 px shaft");
        // It scales with the cursor size.
        assert_eq!(image(Cur::MovePixels, true, 48).hotspot, [4, 4]);
    }
}
