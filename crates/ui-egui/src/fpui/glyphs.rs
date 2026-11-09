//! Kleine, eigene Vektor-Symbole der Inspektor- und Bedienfeldknöpfe (keine Grafiken des Originals):
//! Füllarten, Strichposition, Pfadoperationen, Textausrichtung, Stil-Knöpfe, Filter + / − u. a.
//! Farben aus dem [`Theme`]; im OLED-Design mit Akzentfarben.

use super::theme::Theme;
use egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    FillNone,
    FillSolid,
    FillGradient,
    FillPattern,
    FillDither,
    StrokeInside,
    StrokeCenter,
    StrokeOutside,
    OpNormal,
    OpUnion,
    OpIntersect,
    OpPunch,
    OpCrop,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignJustify,
    AlignStretch,
    Bold,
    Italic,
    Underline,
    StyleNew,
    StyleRedefine,
    StyleClear,
    StyleUnlink,
    Trash,
    Plus,
    Minus,
    PaintBucket,
    Pencil,
    /// Edit the export image of a slice: pixel image of the original (red arc over a dimension line).
    EditImage,
    Opacity,
    Tracking,
    Leading,
    Indent,
    SpaceBefore,
    SpaceAfter,
    HScale,
    Baseline,
    RotateCw,
    RotateCcw,
    RoundTip,
    SquareTip,
    Eye,
    Lock,
    Folder,
    NewLayer,
    NewSubLayer,
    Duplicate,
    Mask,
    Play,
    First,
    Last,
    Prev,
    Next,
    Onion,
    Loop,
    Distribute,
    Save,
    Copy,
    Redo,
    Eyedropper,
    Picker,
    Check,
    Gear,
    /// ⓘ (Einstellungen eines Live-Effekts)
    Info,
    TextT,
    Bitmap,
    Link,
}

/// Aufklapp-Chevron (Windows-8-Kombifeld).
pub fn chevron_down(p: &Painter, r: Rect, col: Color32) {
    let c = r.center();
    p.line(
        vec![
            pos2(c.x - 3.5, c.y - 1.5),
            pos2(c.x, c.y + 2.0),
            pos2(c.x + 3.5, c.y - 1.5),
        ],
        Stroke::new(1.2, col),
    );
}

fn sq(c: Pos2, w: f32, h: f32) -> Rect {
    Rect::from_center_size(c, vec2(w, h))
}

/// Zeichnet ein Symbol zentriert in `r`.
pub fn paint(p: &Painter, r: Rect, g: Glyph, t: &Theme, enabled: bool) {
    let c = &t.colors;
    let fg = if enabled { c.icon_fg } else { c.icon_disabled };
    let red = if enabled { c.icon_red } else { c.icon_disabled };
    let blue = if enabled { c.icon_blue } else { c.icon_disabled };
    let paper = c.icon_paper;
    let s1 = Stroke::new(1.0, fg);
    let m = r.center();
    let k = (r.width().min(r.height()) / 16.0).clamp(0.6, 1.6);
    let z = |x: f32, y: f32| pos2(m.x + x * k, m.y + y * k);
    match g {
        Glyph::FillNone => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            p.rect_filled(b, 0.0, paper);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
            p.line_segment([b.left_bottom(), b.right_top()], Stroke::new(1.5, red));
        }
        Glyph::FillSolid => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            p.rect_filled(b, 0.0, paper);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
        Glyph::FillGradient => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            super::theme::vgradient(p, b, paper, fg);
            let mut mesh = egui::epaint::Mesh::default();
            mesh.colored_vertex(b.left_top(), paper);
            mesh.colored_vertex(b.right_top(), fg);
            mesh.colored_vertex(b.left_bottom(), paper);
            mesh.colored_vertex(b.right_bottom(), fg);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(1, 2, 3);
            p.add(Shape::mesh(mesh));
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
        kind @ (Glyph::FillPattern | Glyph::FillDither) => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            p.rect_filled(b, 0.0, paper);
            let cells = if matches!(kind, Glyph::FillDither) { 8 } else { 4 };
            let cell = b.width() / cells as f32;
            for j in 0..cells {
                for i in 0..cells {
                    if (i + j) % 2 == 0 {
                        p.rect_filled(
                            Rect::from_min_size(
                                pos2(b.left() + i as f32 * cell, b.top() + j as f32 * cell),
                                vec2(cell, cell),
                            ),
                            0.0,
                            fg,
                        );
                    }
                }
            }
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
        Glyph::StrokeInside | Glyph::StrokeCenter | Glyph::StrokeOutside => {
            let b = sq(m, 10.0 * k, 10.0 * k);
            p.rect_filled(b, 0.0, paper);
            let (inset, w) = match g {
                Glyph::StrokeInside => (1.5 * k, 2.0 * k),
                Glyph::StrokeCenter => (0.0, 2.0 * k),
                _ => (-1.5 * k, 2.0 * k),
            };
            p.rect_stroke(b.shrink(inset), 0.0, Stroke::new(w, fg), egui::StrokeKind::Middle);
            p.rect_stroke(b, 0.0, Stroke::new(1.0, blue), egui::StrokeKind::Middle);
        }
        Glyph::OpNormal | Glyph::OpUnion | Glyph::OpIntersect | Glyph::OpPunch | Glyph::OpCrop => {
            let a = Rect::from_min_size(z(-6.0, -6.0), vec2(8.0 * k, 8.0 * k));
            let b = Rect::from_min_size(z(-2.0, -2.0), vec2(8.0 * k, 8.0 * k));
            let fill = if enabled { c.icon_light } else { c.frame_bg };
            match g {
                Glyph::OpNormal => {
                    p.rect_filled(b, 0.0, paper);
                    p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
                }
                Glyph::OpUnion => {
                    p.rect_filled(a, 0.0, fill);
                    p.rect_filled(b, 0.0, fill);
                    p.rect_stroke(a, 0.0, s1, egui::StrokeKind::Inside);
                    p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
                    p.rect_filled(a.intersect(b).shrink(0.5), 0.0, fill);
                }
                Glyph::OpIntersect => {
                    p.rect_stroke(a, 0.0, s1, egui::StrokeKind::Inside);
                    p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
                    p.rect_filled(a.intersect(b), 0.0, fg);
                }
                Glyph::OpPunch => {
                    p.rect_filled(a, 0.0, fill);
                    p.rect_stroke(a, 0.0, s1, egui::StrokeKind::Inside);
                    p.rect_filled(a.intersect(b), 0.0, c.frame_bg);
                    p.rect_stroke(
                        b,
                        0.0,
                        Stroke::new(1.0, c.icon_disabled),
                        egui::StrokeKind::Inside,
                    );
                }
                _ => {
                    p.rect_stroke(
                        a,
                        0.0,
                        Stroke::new(1.0, c.icon_disabled),
                        egui::StrokeKind::Inside,
                    );
                    p.rect_filled(a.intersect(b), 0.0, fill);
                    p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
                }
            }
        }
        Glyph::AlignLeft
        | Glyph::AlignCenter
        | Glyph::AlignRight
        | Glyph::AlignJustify
        | Glyph::AlignStretch => {
            let widths = match g {
                Glyph::AlignJustify | Glyph::AlignStretch => [10.0, 10.0, 10.0, 6.0],
                _ => [10.0, 6.0, 10.0, 6.0],
            };
            for (i, w) in widths.iter().enumerate() {
                let y = m.y + (i as f32 * 3.0 - 4.5) * k;
                let w = if g == Glyph::AlignStretch { 10.0 } else { *w } * k;
                let x0 = match g {
                    Glyph::AlignCenter => m.x - w / 2.0,
                    Glyph::AlignRight => m.x + 5.0 * k - w,
                    _ => m.x - 5.0 * k,
                };
                p.hline(x0..=x0 + w, y, s1);
            }
        }
        Glyph::Italic if false => {
            // kursives „K“ (deutsche Oberfläche wie im Original, PARITAET_A W16): schräge Striche,
            // egui hat keine kursive Schrift
            let sk = |x: f32, y: f32| z(x - y * 0.3, y);
            let s = Stroke::new(1.3, fg);
            p.line_segment([sk(-2.5, -4.5), sk(-2.5, 4.5)], s);
            p.line_segment([sk(3.5, -4.5), sk(-2.0, 0.5)], s);
            p.line_segment([sk(-0.6, -0.8), sk(3.8, 4.5)], s);
        }
        Glyph::Bold | Glyph::Italic | Glyph::Underline | Glyph::TextT => {
            // Kurzzeichen je Sprache (deutsch F/K wie im Original, englisch B/I)
            let (b, i) = ("B".to_owned(), "I".to_owned());
            let (txt, font) = match g {
                Glyph::Bold => (b.as_str(), t.bold(11.0 * k.max(1.0))),
                Glyph::Italic => (i.as_str(), egui::FontId::proportional(11.0 * k.max(1.0))),
                Glyph::Underline => ("U", t.font(11.0 * k.max(1.0))),
                _ => ("T", t.bold(12.0 * k.max(1.0))),
            };
            p.text(m, Align2::CENTER_CENTER, txt, font, fg);
            if g == Glyph::Italic && i == "I" {
                p.line_segment([z(1.5, -4.5), z(-1.5, 4.5)], Stroke::new(1.3, fg));
            }
            if g == Glyph::Underline {
                p.hline(z(-4.0, 6.0).x..=z(4.0, 6.0).x, z(0.0, 6.0).y, s1);
            }
        }
        Glyph::StyleNew | Glyph::Duplicate | Glyph::NewLayer => {
            let b = Rect::from_min_size(z(-4.0, -5.0), vec2(9.0 * k, 10.0 * k));
            p.rect_filled(b, 0.0, paper);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
            p.line(
                vec![
                    b.right_top() + vec2(-3.0 * k, 0.0),
                    b.right_top() + vec2(0.0, 3.0 * k),
                ],
                s1,
            );
            if g == Glyph::NewLayer {
                p.hline(b.left() + 2.0..=b.right() - 2.0, b.center().y, s1);
            }
        }
        Glyph::StyleRedefine => {
            let b = Rect::from_min_size(z(-5.0, -4.0), vec2(8.0 * k, 9.0 * k));
            p.rect_filled(b, 0.0, paper);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
            p.line(
                vec![z(2.0, -1.0), z(6.0, -1.0), z(4.5, -3.0)],
                Stroke::new(1.2, blue),
            );
        }
        Glyph::StyleClear => {
            let b = Rect::from_min_size(z(-5.0, -4.0), vec2(8.0 * k, 9.0 * k));
            p.rect_filled(b, 0.0, paper);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
            p.line_segment([z(1.0, 1.0), z(6.0, 6.0)], Stroke::new(1.3, red));
            p.line_segment([z(6.0, 1.0), z(1.0, 6.0)], Stroke::new(1.3, red));
        }
        Glyph::StyleUnlink | Glyph::Link => {
            p.circle_stroke(z(-2.5, 0.0), 3.0 * k, s1);
            p.circle_stroke(z(2.5, 0.0), 3.0 * k, s1);
            if g == Glyph::StyleUnlink {
                p.line_segment([z(-1.0, 5.0), z(1.0, -5.0)], Stroke::new(1.3, red));
            }
        }
        Glyph::Trash => {
            let b = Rect::from_min_max(z(-4.0, -3.0), z(4.0, 6.0));
            p.rect_stroke(b, 1.0, s1, egui::StrokeKind::Inside);
            p.hline(z(-5.5, -4.5).x..=z(5.5, -4.5).x, z(0.0, -4.5).y, s1);
            p.hline(z(-1.5, -6.0).x..=z(1.5, -6.0).x, z(0.0, -6.0).y, s1);
            for x in [-1.5f32, 1.5] {
                p.vline(z(x, 0.0).x, z(0.0, -1.0).y..=z(0.0, 4.0).y, s1);
            }
        }
        Glyph::Plus => {
            p.hline(z(-5.0, 0.0).x..=z(5.0, 0.0).x, m.y, Stroke::new(2.0, fg));
            p.vline(m.x, z(0.0, -5.0).y..=z(0.0, 5.0).y, Stroke::new(2.0, fg));
        }
        Glyph::Minus => {
            p.hline(z(-5.0, 0.0).x..=z(5.0, 0.0).x, m.y, Stroke::new(2.0, fg));
        }
        Glyph::PaintBucket => {
            let pts = vec![z(-5.0, -1.0), z(0.0, -6.0), z(5.0, -1.0), z(0.0, 4.0)];
            p.add(Shape::convex_polygon(pts, paper, s1));
            p.circle_filled(z(5.5, 3.5), 1.8 * k, if enabled { c.icon_blue } else { fg });
            p.line_segment([z(-1.0, -4.5), z(-3.0, -7.0)], s1);
        }
        Glyph::Pencil => {
            p.line_segment(
                [z(-5.0, 5.0), z(4.0, -4.0)],
                Stroke::new(3.0 * k, if enabled { c.icon_yellow } else { fg }),
            );
            p.line_segment([z(-6.0, 6.0), z(-4.5, 4.5)], Stroke::new(2.0 * k, fg));
            p.line_segment([z(3.0, -5.0), z(5.0, -3.0)], Stroke::new(1.0, fg));
        }
        Glyph::EditImage => {
            // TW-76: Pixelbild des Originals (Segment-Inspektor, 15 × 10, `inspector-web/01`):
            // roter Bogen über einer Maßlinie mit Endpunkten
            const ROWS: [&str; 10] = [
                "......adDDDDda.",
                ".k...adRrrrrrRa",
                ".g...RRa....aRg",
                ".g..arg......rd",
                ".g..grd......gd",
                "GkG.dmk..gk..dD",
                "kbkkkrkkkbk..dD",
                "kwG.gwgaawk..wr",
                ".g..ggg..aa..gg",
                ".k.............",
            ];
            let px = k.max(1.0).round();
            let o = pos2((m.x - 7.5 * px).round(), (m.y - 5.0 * px).round());
            for (y, row) in ROWS.iter().enumerate() {
                for (x, ch) in row.chars().enumerate() {
                    let col = match ch {
                        'k' => Color32::from_rgb(0x04, 0x04, 0x04),
                        'g' => Color32::from_rgb(0x66, 0x66, 0x66),
                        'G' => Color32::from_rgb(0x4d, 0x4d, 0x4d),
                        'a' => Color32::from_rgb(0xa0, 0xa0, 0xa4),
                        'r' => Color32::from_rgb(0xff, 0x50, 0x50),
                        'R' => Color32::from_rgb(0xcc, 0x33, 0x33),
                        'd' => Color32::from_rgb(0x66, 0x33, 0x33),
                        'D' => Color32::from_rgb(0x33, 0x00, 0x00),
                        'm' => Color32::from_rgb(0x66, 0x00, 0x00),
                        'b' => Color32::from_rgb(0x99, 0xcc, 0xff),
                        'w' => Color32::WHITE,
                        _ => continue,
                    };
                    let col = if enabled { col } else { c.icon_disabled };
                    p.rect_filled(
                        Rect::from_min_size(pos2(o.x + x as f32 * px, o.y + y as f32 * px), vec2(px, px)),
                        0.0,
                        col,
                    );
                }
            }
        }
        Glyph::Opacity => {
            let b = sq(m, 10.0 * k, 10.0 * k);
            let h = Rect::from_min_max(b.min, pos2(b.center().x, b.bottom()));
            p.rect_filled(b, 0.0, paper);
            p.rect_filled(h, 0.0, fg);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
        Glyph::Tracking => {
            p.text(
                z(-2.0, 0.0),
                Align2::CENTER_CENTER,
                "AV",
                t.font(8.0 * k.max(1.0)),
                fg,
            );
            p.line(vec![z(-6.0, 6.0), z(6.0, 6.0)], s1);
        }
        Glyph::Leading => {
            p.text(m, Align2::CENTER_CENTER, "A", t.font(7.0 * k.max(1.0)), fg);
            p.vline(z(5.0, 0.0).x, z(0.0, -5.0).y..=z(0.0, 5.0).y, s1);
        }
        Glyph::Indent => {
            for (i, x) in [1.0f32, -4.0, -4.0].iter().enumerate() {
                let y = m.y + (i as f32 * 3.5 - 3.5) * k;
                p.hline(z(*x, 0.0).x..=z(5.0, 0.0).x, y, s1);
            }
        }
        Glyph::SpaceBefore | Glyph::SpaceAfter => {
            let y_gap = if g == Glyph::SpaceBefore { -4.0 } else { 4.0 };
            p.hline(
                z(-5.0, 0.0).x..=z(5.0, 0.0).x,
                z(0.0, y_gap).y,
                Stroke::new(1.0, blue),
            );
            p.hline(z(-5.0, 0.0).x..=z(5.0, 0.0).x, m.y, s1);
            p.hline(z(-5.0, 0.0).x..=z(5.0, 0.0).x, z(0.0, -y_gap).y, s1);
        }
        Glyph::HScale => {
            p.text(m, Align2::CENTER_CENTER, "T", t.font(8.0 * k.max(1.0)), fg);
            p.line(vec![z(-6.0, 6.0), z(6.0, 6.0)], s1);
        }
        Glyph::Baseline => {
            p.text(
                z(-1.0, -1.0),
                Align2::CENTER_CENTER,
                "A",
                t.font(8.0 * k.max(1.0)),
                fg,
            );
            p.text(
                z(4.0, -3.0),
                Align2::CENTER_CENTER,
                "a",
                t.font(6.0 * k.max(1.0)),
                blue,
            );
        }
        Glyph::RotateCw | Glyph::RotateCcw => {
            let pts: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let a = std::f32::consts::PI * (0.2 + 1.4 * i as f32 / 10.0);
                    pos2(m.x + 4.5 * k * a.cos(), m.y - 4.5 * k * a.sin())
                })
                .collect();
            let tip = if g == Glyph::RotateCw { pts[0] } else { pts[10] };
            p.line(pts, s1);
            p.circle_filled(tip, 1.6 * k, fg);
        }
        Glyph::RoundTip => {
            p.circle_filled(m, 4.5 * k, fg);
        }
        Glyph::SquareTip => {
            p.rect_filled(sq(m, 8.0 * k, 8.0 * k), 0.0, fg);
        }
        Glyph::Eye => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / 16.0;
                    pos2(m.x + 6.0 * k * a.cos(), m.y + 3.2 * k * a.sin())
                })
                .collect();
            p.add(Shape::closed_line(pts, s1));
            p.circle_filled(m, 2.0 * k, fg);
        }
        Glyph::Lock => {
            let b = Rect::from_min_max(z(-4.0, -1.0), z(4.0, 5.0));
            p.rect_filled(b, 1.0, fg);
            let arc: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 8.0;
                    pos2(m.x + 2.8 * k * a.cos(), m.y - 1.0 * k - 3.2 * k * a.sin())
                })
                .collect();
            p.line(arc, Stroke::new(1.3, fg));
        }
        Glyph::Folder => {
            let b = Rect::from_min_max(z(-6.0, -3.0), z(6.0, 5.0));
            p.rect_filled(b, 1.0, if enabled { c.icon_yellow } else { paper });
            p.rect_stroke(b, 1.0, s1, egui::StrokeKind::Inside);
            p.rect_filled(
                Rect::from_min_max(z(-6.0, -5.0), z(-1.0, -3.0)),
                1.0,
                if enabled { c.icon_yellow } else { paper },
            );
        }
        Glyph::NewSubLayer => {
            let b = Rect::from_min_max(z(-6.0, -3.0), z(3.0, 4.0));
            p.rect_stroke(b, 1.0, s1, egui::StrokeKind::Inside);
            p.hline(z(0.0, 0.0).x..=z(6.0, 0.0).x, z(0.0, 6.0).y, s1);
            p.vline(z(3.0, 0.0).x, z(0.0, 3.0).y..=z(0.0, 6.0).y, s1);
        }
        Glyph::Mask => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            p.rect_filled(b, 0.0, fg);
            p.circle_filled(m, 3.5 * k, paper);
        }
        Glyph::Play => {
            p.add(Shape::convex_polygon(
                vec![z(-3.0, -5.0), z(4.0, 0.0), z(-3.0, 5.0)],
                fg,
                Stroke::NONE,
            ));
        }
        Glyph::First | Glyph::Last | Glyph::Prev | Glyph::Next => {
            let dir = if matches!(g, Glyph::Last | Glyph::Next) {
                1.0
            } else {
                -1.0
            };
            p.add(Shape::convex_polygon(
                vec![z(-3.0 * dir, -4.0), z(3.0 * dir, 0.0), z(-3.0 * dir, 4.0)],
                fg,
                Stroke::NONE,
            ));
            if matches!(g, Glyph::First | Glyph::Last) {
                p.vline(
                    z(4.0 * dir, 0.0).x,
                    z(0.0, -4.0).y..=z(0.0, 4.0).y,
                    Stroke::new(1.5, fg),
                );
            }
        }
        Glyph::Onion => {
            for (i, a) in [80u8, 160, 255].iter().enumerate() {
                let col = super::theme::with_alpha(fg, *a);
                p.rect_stroke(
                    Rect::from_min_size(
                        z(-6.0 + i as f32 * 3.0, -5.0 + i as f32 * 2.0),
                        vec2(7.0 * k, 7.0 * k),
                    ),
                    0.0,
                    Stroke::new(1.0, col),
                    egui::StrokeKind::Inside,
                );
            }
        }
        Glyph::Loop => {
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::TAU * (0.1 + 0.8 * i as f32 / 12.0);
                    pos2(m.x + 5.0 * k * a.cos(), m.y + 4.0 * k * a.sin())
                })
                .collect();
            let end = pts[12];
            p.line(pts, s1);
            p.circle_filled(end, 1.6 * k, fg);
        }
        Glyph::Distribute => {
            for i in 0..3 {
                p.rect_stroke(
                    Rect::from_min_size(z(-6.0 + i as f32 * 4.0, -3.0), vec2(3.0 * k, 6.0 * k)),
                    0.0,
                    s1,
                    egui::StrokeKind::Inside,
                );
            }
            p.add(Shape::convex_polygon(
                vec![z(3.0, 4.0), z(6.0, 6.0), z(3.0, 8.0)],
                fg,
                Stroke::NONE,
            ));
        }
        Glyph::Save => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            p.rect_filled(b, 1.0, fg);
            p.rect_filled(
                Rect::from_min_max(
                    b.min + vec2(2.5 * k, 1.0),
                    pos2(b.right() - 2.5 * k, b.top() + 4.0 * k),
                ),
                0.0,
                paper,
            );
        }
        Glyph::Copy => {
            p.rect_stroke(
                Rect::from_min_size(z(-5.0, -5.0), vec2(7.0 * k, 8.0 * k)),
                0.0,
                s1,
                egui::StrokeKind::Inside,
            );
            let b = Rect::from_min_size(z(-2.0, -2.0), vec2(7.0 * k, 8.0 * k));
            p.rect_filled(b, 0.0, paper);
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
        Glyph::Redo => {
            let pts: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 - i as f32 / 8.0);
                    pos2(m.x + 4.0 * k * a.cos(), m.y + 1.5 * k - 4.0 * k * a.sin())
                })
                .collect();
            let end = pts[8];
            p.line(pts, Stroke::new(1.3, fg));
            p.add(Shape::convex_polygon(
                vec![
                    end + vec2(-3.0 * k, 0.0),
                    end + vec2(3.0 * k, 0.0),
                    end + vec2(0.0, 3.0 * k),
                ],
                fg,
                Stroke::NONE,
            ));
        }
        Glyph::Eyedropper => {
            p.line_segment([z(-5.0, 5.0), z(2.0, -2.0)], Stroke::new(2.0 * k, fg));
            p.circle_filled(z(3.5, -3.5), 2.5 * k, fg);
        }
        Glyph::Picker => {
            let b = sq(m, 11.0 * k, 11.0 * k);
            let colors = [c.icon_red, c.icon_yellow, c.icon_green, c.icon_blue];
            for (i, col) in colors.iter().enumerate() {
                let q = Rect::from_min_size(
                    pos2(
                        b.left() + (i % 2) as f32 * b.width() / 2.0,
                        b.top() + (i / 2) as f32 * b.height() / 2.0,
                    ),
                    b.size() / 2.0,
                );
                p.rect_filled(q, 0.0, if enabled { *col } else { c.icon_disabled });
            }
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
        Glyph::Check => {
            p.line(
                vec![z(-4.0, 0.0), z(-1.0, 3.5), z(4.5, -3.5)],
                Stroke::new(1.6, fg),
            );
        }
        Glyph::Gear => {
            p.circle_stroke(m, 3.5 * k, Stroke::new(2.0, fg));
            for i in 0..8 {
                let a = std::f32::consts::TAU * i as f32 / 8.0;
                p.line_segment(
                    [
                        pos2(m.x + 4.5 * k * a.cos(), m.y + 4.5 * k * a.sin()),
                        pos2(m.x + 6.5 * k * a.cos(), m.y + 6.5 * k * a.sin()),
                    ],
                    Stroke::new(1.6, fg),
                );
            }
        }
        Glyph::Info => {
            // ⓘ: blauer Kreis mit weißem „i“ (gesperrt grau)
            let fill = if enabled { blue } else { c.icon_disabled };
            p.circle_filled(m, 6.0 * k, fill);
            p.circle_filled(z(0.0, -3.0), 1.1 * k, paper);
            p.line_segment([z(0.0, -0.8), z(0.0, 3.8)], Stroke::new(1.8 * k, paper));
        }
        Glyph::Bitmap => {
            let b = sq(m, 11.0 * k, 9.0 * k);
            p.rect_filled(b, 0.0, paper);
            p.add(Shape::convex_polygon(
                vec![
                    b.left_bottom(),
                    b.left_bottom() + vec2(4.0 * k, -5.0 * k),
                    b.left_bottom() + vec2(8.0 * k, 0.0),
                ],
                if enabled { c.icon_green } else { fg },
                Stroke::NONE,
            ));
            p.circle_filled(
                b.right_top() + vec2(-2.5 * k, 2.5 * k),
                1.3 * k,
                if enabled { c.icon_yellow } else { fg },
            );
            p.rect_stroke(b, 0.0, s1, egui::StrokeKind::Inside);
        }
    }
}
