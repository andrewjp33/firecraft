//! Tool icons drawn with the painter (FireCraft's own artwork; no third-party assets).

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use firecraft_engine::Tool;

fn dashed_rect(p: &Painter, r: Rect, c: Color32) {
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    p.extend(Shape::dashed_line(&pts, Stroke::new(1.0, c), 2.0, 2.0));
}

/// Paint `tool`'s icon into `r` (about 16x16).
pub fn paint(p: &Painter, r: Rect, tool: Tool, enabled: bool) {
    let c = if enabled {
        Color32::from_gray(20)
    } else {
        Color32::from_gray(150)
    };
    let s = Stroke::new(1.2, c);
    let r = r.shrink(2.0);
    let (x0, y0, w, h) = (r.left(), r.top(), r.width(), r.height());
    let at = |fx: f32, fy: f32| pos2(x0 + fx * w, y0 + fy * h);
    match tool {
        Tool::Pointer | Tool::Subselect | Tool::SelectBehind => {
            let pts = vec![
                at(0.25, 0.05),
                at(0.25, 0.85),
                at(0.45, 0.65),
                at(0.6, 0.95),
                at(0.72, 0.9),
                at(0.58, 0.6),
                at(0.85, 0.6),
            ];
            if tool == Tool::SelectBehind {
                // A pointer in front of a square (the object behind).
                p.rect_filled(
                    Rect::from_min_max(at(0.5, 0.05), at(0.95, 0.5)),
                    0.0,
                    Color32::from_gray(150),
                );
                p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
            } else if tool == Tool::Pointer {
                p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
            } else {
                p.add(Shape::closed_line(pts, s));
            }
        }
        Tool::Skew => {
            // A slanted box.
            let q = [at(0.3, 0.15), at(0.95, 0.15), at(0.7, 0.85), at(0.05, 0.85)];
            p.add(Shape::closed_line(q.to_vec(), s));
            p.line_segment([at(0.45, 0.02), at(0.8, 0.02)], s);
        }
        Tool::Distort => {
            // An irregular quadrilateral with corner dots.
            let q = [at(0.1, 0.2), at(0.85, 0.05), at(0.95, 0.9), at(0.25, 0.75)];
            let fill = s.color;
            p.add(Shape::closed_line(q.to_vec(), s));
            for c in q {
                p.rect_filled(Rect::from_center_size(c, egui::vec2(3.0, 3.0)), 0.0, fill);
            }
        }
        Tool::Scale => {
            p.rect_stroke(
                Rect::from_min_max(at(0.1, 0.35), at(0.65, 0.9)),
                0.0,
                s,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(0.4, 0.6), at(0.9, 0.1)], s);
            p.line_segment([at(0.65, 0.1), at(0.9, 0.1)], s);
            p.line_segment([at(0.9, 0.1), at(0.9, 0.35)], s);
        }
        Tool::NineSlice => {
            p.rect_stroke(
                Rect::from_min_max(at(0.1, 0.1), at(0.9, 0.9)),
                0.0,
                s,
                egui::StrokeKind::Inside,
            );
            for f in [0.37, 0.63] {
                p.extend(Shape::dashed_line(
                    &[at(f, 0.1), at(f, 0.9)],
                    Stroke::new(1.0, Color32::from_rgb(0, 120, 220)),
                    1.5,
                    1.5,
                ));
                p.extend(Shape::dashed_line(
                    &[at(0.1, f), at(0.9, f)],
                    Stroke::new(1.0, Color32::from_rgb(0, 120, 220)),
                    1.5,
                    1.5,
                ));
            }
        }
        Tool::Crop => {
            p.line_segment([at(0.25, 0.0), at(0.25, 0.75)], s);
            p.line_segment([at(0.0, 0.25), at(0.75, 0.25)], s);
            p.line_segment([at(0.25, 0.75), at(1.0, 0.75)], s);
            p.line_segment([at(0.75, 0.25), at(0.75, 1.0)], s);
        }
        Tool::Marquee => dashed_rect(p, Rect::from_min_max(at(0.1, 0.2), at(0.9, 0.8)), c),
        Tool::OvalMarquee => {
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0 * std::f32::consts::TAU;
                    at(0.5 + 0.42 * t.cos(), 0.5 + 0.3 * t.sin())
                })
                .collect();
            p.extend(Shape::dashed_line(&pts, Stroke::new(1.0, c), 2.0, 2.0));
        }
        Tool::Lasso => {
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let t = i as f32 / 20.0 * std::f32::consts::TAU;
                    at(0.5 + 0.4 * t.cos(), 0.4 + 0.25 * t.sin())
                })
                .collect();
            p.add(Shape::line(pts, s));
            p.line_segment([at(0.3, 0.6), at(0.25, 0.95)], s);
        }
        Tool::MagicWand => {
            p.line_segment([at(0.15, 0.9), at(0.6, 0.45)], Stroke::new(2.0, c));
            let star = at(0.72, 0.3);
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4 * 2.0;
                p.line_segment(
                    [star, star + vec2(a.cos(), a.sin()) * 4.0],
                    Stroke::new(1.0, Color32::from_rgb(230, 170, 0)),
                );
            }
        }
        Tool::Brush => {
            p.line_segment([at(0.85, 0.1), at(0.4, 0.6)], Stroke::new(1.6, c));
            p.circle_filled(at(0.3, 0.72), 3.0, Color32::from_rgb(60, 60, 200));
        }
        Tool::Pencil => {
            p.line_segment(
                [at(0.85, 0.1), at(0.2, 0.75)],
                Stroke::new(3.0, Color32::from_rgb(230, 190, 60)),
            );
            p.line_segment([at(0.2, 0.75), at(0.1, 0.9)], s);
        }
        Tool::Eraser => {
            let pts = vec![at(0.1, 0.65), at(0.5, 0.25), at(0.9, 0.45), at(0.5, 0.85)];
            p.add(Shape::convex_polygon(pts, Color32::from_rgb(250, 200, 200), s));
        }
        Tool::Blur => {
            p.circle(at(0.5, 0.6), w * 0.25, Color32::from_rgb(120, 160, 230), s);
        }
        Tool::RubberStamp => {
            p.rect_filled(Rect::from_min_max(at(0.2, 0.7), at(0.8, 0.9)), 1.0, c);
            p.line_segment([at(0.5, 0.2), at(0.5, 0.7)], Stroke::new(3.0, c));
        }
        Tool::Eyedropper => {
            p.line_segment([at(0.15, 0.9), at(0.65, 0.4)], Stroke::new(1.5, c));
            p.circle_filled(at(0.75, 0.25), 3.0, c);
        }
        Tool::PaintBucket => {
            let pts = vec![at(0.2, 0.45), at(0.55, 0.15), at(0.85, 0.5), at(0.5, 0.85)];
            p.add(Shape::convex_polygon(pts, Color32::from_gray(220), s));
            p.circle_filled(at(0.15, 0.75), 2.5, Color32::from_rgb(40, 120, 220));
        }
        Tool::Line => {
            p.line_segment([at(0.1, 0.9), at(0.9, 0.1)], s);
        }
        Tool::Pen => {
            let pts = vec![at(0.5, 0.05), at(0.75, 0.55), at(0.5, 0.95), at(0.25, 0.55)];
            p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
        }
        Tool::Rectangle => {
            p.rect_stroke(
                Rect::from_min_max(at(0.1, 0.2), at(0.9, 0.8)),
                0.0,
                s,
                egui::StrokeKind::Inside,
            );
        }
        Tool::Ellipse => {
            p.add(egui::epaint::EllipseShape::stroke(
                at(0.5, 0.5),
                vec2(w * 0.42, h * 0.3),
                s,
            ));
        }
        Tool::Text => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(15.0),
                c,
            );
        }
        Tool::Freeform | Tool::Knife => {
            let pts = vec![
                at(0.1, 0.8),
                at(0.3, 0.3),
                at(0.5, 0.7),
                at(0.7, 0.2),
                at(0.9, 0.6),
            ];
            p.add(Shape::line(pts, s));
        }
        Tool::Hotspot => {
            let rr = Rect::from_min_max(at(0.1, 0.15), at(0.9, 0.85));
            p.rect_filled(rr, 0.0, Color32::from_rgb(0, 200, 230));
            p.rect_stroke(rr, 0.0, s, egui::StrokeKind::Inside);
        }
        Tool::Slice => {
            let rr = Rect::from_min_max(at(0.1, 0.15), at(0.9, 0.85));
            p.rect_filled(rr, 0.0, Color32::from_rgb(120, 220, 120));
            p.line_segment(
                [at(0.1, 0.5), at(0.9, 0.5)],
                Stroke::new(1.0, Color32::from_rgb(200, 0, 0)),
            );
        }
        Tool::Hand => {
            p.circle_stroke(at(0.5, 0.6), w * 0.28, s);
            for k in 0..4 {
                let x = 0.3 + k as f32 * 0.13;
                p.line_segment([at(x, 0.15), at(x, 0.45)], s);
            }
        }
        Tool::Zoom => {
            p.circle_stroke(at(0.4, 0.4), w * 0.28, s);
            p.line_segment([at(0.6, 0.6), at(0.95, 0.95)], Stroke::new(2.0, c));
        }
    }
}

/// Small UI glyphs, painted (the suite's font set carries no symbol/emoji faces).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    TriangleUp,
    TriangleDown,
    TriangleRight,
    Eye,
    Lock,
    Plus,
    Minus,
    Trash,
    Folder,
    Globe,
    Pencil,
    Bucket,
    Swap,
    Defaults,
    NoColor,
    Opacity,
    First,
    Play,
    Stop,
    Last,
}

pub fn paint_glyph(p: &Painter, r: Rect, g: Glyph, c: Color32) {
    let s = Stroke::new(1.2, c);
    let at = |fx: f32, fy: f32| pos2(r.left() + fx * r.width(), r.top() + fy * r.height());
    match g {
        Glyph::TriangleUp => {
            p.add(Shape::convex_polygon(
                vec![at(0.2, 0.7), at(0.8, 0.7), at(0.5, 0.25)],
                c,
                Stroke::NONE,
            ));
        }
        Glyph::TriangleDown => {
            p.add(Shape::convex_polygon(
                vec![at(0.2, 0.3), at(0.8, 0.3), at(0.5, 0.75)],
                c,
                Stroke::NONE,
            ));
        }
        Glyph::TriangleRight | Glyph::Play => {
            p.add(Shape::convex_polygon(
                vec![at(0.3, 0.2), at(0.75, 0.5), at(0.3, 0.8)],
                c,
                Stroke::NONE,
            ));
        }
        Glyph::Eye => {
            p.add(egui::epaint::EllipseShape::stroke(
                at(0.5, 0.5),
                vec2(r.width() * 0.4, r.height() * 0.22),
                s,
            ));
            p.circle_filled(at(0.5, 0.5), r.height() * 0.13, c);
        }
        Glyph::Lock => {
            p.rect_filled(Rect::from_min_max(at(0.25, 0.45), at(0.75, 0.85)), 1.0, c);
            p.circle_stroke(at(0.5, 0.42), r.width() * 0.17, s);
        }
        Glyph::Plus | Glyph::Minus => {
            p.line_segment([at(0.2, 0.5), at(0.8, 0.5)], Stroke::new(1.5, c));
            if g == Glyph::Plus {
                p.line_segment([at(0.5, 0.2), at(0.5, 0.8)], Stroke::new(1.5, c));
            }
        }
        Glyph::Trash => {
            p.rect_stroke(
                Rect::from_min_max(at(0.28, 0.32), at(0.72, 0.88)),
                0.0,
                s,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(0.18, 0.25), at(0.82, 0.25)], s);
            p.line_segment([at(0.42, 0.15), at(0.58, 0.15)], s);
        }
        Glyph::Folder => {
            p.rect_filled(
                Rect::from_min_max(at(0.1, 0.3), at(0.9, 0.85)),
                1.0,
                Color32::from_rgb(240, 200, 90),
            );
            p.rect_filled(
                Rect::from_min_max(at(0.1, 0.2), at(0.45, 0.35)),
                1.0,
                Color32::from_rgb(220, 170, 60),
            );
        }
        Glyph::Globe => {
            p.circle_stroke(
                at(0.5, 0.5),
                r.width() * 0.38,
                Stroke::new(1.0, Color32::from_rgb(40, 110, 200)),
            );
            p.line_segment(
                [at(0.12, 0.5), at(0.88, 0.5)],
                Stroke::new(1.0, Color32::from_rgb(40, 110, 200)),
            );
            p.add(egui::epaint::EllipseShape::stroke(
                at(0.5, 0.5),
                vec2(r.width() * 0.16, r.height() * 0.38),
                Stroke::new(1.0, Color32::from_rgb(40, 110, 200)),
            ));
        }
        Glyph::Pencil => paint(p, r.expand(2.0), Tool::Pencil, true),
        Glyph::Bucket => paint(p, r.expand(2.0), Tool::PaintBucket, true),
        Glyph::Swap => {
            p.line_segment([at(0.25, 0.75), at(0.75, 0.25)], s);
            p.line_segment([at(0.75, 0.25), at(0.5, 0.25)], s);
            p.line_segment([at(0.75, 0.25), at(0.75, 0.5)], s);
            p.line_segment([at(0.25, 0.75), at(0.5, 0.75)], s);
            p.line_segment([at(0.25, 0.75), at(0.25, 0.5)], s);
        }
        Glyph::Defaults => {
            p.rect_filled(
                Rect::from_min_max(at(0.1, 0.1), at(0.6, 0.6)),
                0.0,
                Color32::BLACK,
            );
            p.rect_filled(
                Rect::from_min_max(at(0.4, 0.4), at(0.9, 0.9)),
                0.0,
                Color32::from_gray(153),
            );
        }
        Glyph::NoColor => {
            p.rect_stroke(
                Rect::from_min_max(at(0.15, 0.15), at(0.85, 0.85)),
                0.0,
                Stroke::new(1.0, c),
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(0.15, 0.85), at(0.85, 0.15)], Stroke::new(1.5, Color32::RED));
        }
        Glyph::Opacity => {
            p.rect_stroke(
                Rect::from_min_max(at(0.15, 0.15), at(0.85, 0.85)),
                0.0,
                s,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(Rect::from_min_max(at(0.15, 0.15), at(0.5, 0.5)), 0.0, c);
            p.rect_filled(Rect::from_min_max(at(0.5, 0.5), at(0.85, 0.85)), 0.0, c);
        }
        Glyph::Stop => {
            p.rect_filled(Rect::from_min_max(at(0.25, 0.25), at(0.75, 0.75)), 0.0, c);
        }
        Glyph::First | Glyph::Last => {
            let (bar, tip, base) = if g == Glyph::First {
                (0.25, 0.3, 0.75)
            } else {
                (0.75, 0.7, 0.25)
            };
            p.line_segment([at(bar, 0.2), at(bar, 0.8)], Stroke::new(1.5, c));
            p.add(Shape::convex_polygon(
                vec![at(base, 0.2), at(tip, 0.5), at(base, 0.8)],
                c,
                Stroke::NONE,
            ));
        }
    }
}

/// A frameless glyph button.
pub fn glyph_button(ui: &mut egui::Ui, g: Glyph, size: f32, enabled: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(
        vec2(size, size),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if resp.hovered() && enabled {
        ui.painter().rect_stroke(
            r,
            1.0,
            Stroke::new(1.0, Color32::from_gray(150)),
            egui::StrokeKind::Inside,
        );
    }
    let c = if enabled {
        Color32::from_gray(30)
    } else {
        Color32::from_gray(160)
    };
    paint_glyph(ui.painter(), r.shrink(2.0), g, c);
    resp
}

/// A non-interactive glyph.
pub fn glyph(ui: &mut egui::Ui, g: Glyph, size: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    paint_glyph(ui.painter(), r.shrink(2.0), g, Color32::from_gray(30));
}
