//! An animation symbol's motion path on the canvas: drawn as Fireworks MX draws it and edited by
//! dragging its points (measured in Fireworks MX, docs/behavior/symbols.md#animation-symbols).

use egui::{Color32, Pos2, Shape, Stroke, vec2};
use firecraft_doc::Element;
use firecraft_engine::{DocState, PathPoint};

/// Fireworks MX's marker colours: start, the frames between, end.
const START: Color32 = Color32::from_rgb(0, 255, 0);
const FRAME: Color32 = Color32::from_rgb(0, 168, 255);
const END: Color32 = Color32::from_rgb(255, 0, 0);

/// How near (screen points) the pointer must be to a marker to take it.
const GRIP: f32 = 5.0;

/// A motion path point being dragged.
pub struct PathDrag {
    doc: u64,
    point: PathPoint,
    press: (f64, f64),
    /// The instance when the drag began (each step is applied to it afresh).
    orig: Element,
    moved: bool,
}

/// Draw the selected animation's motion path: a dotted line through its placements and a
/// diamond at each, outlined in white (Fireworks MX: 6 pixels across, green start, blue frames,
/// red end).
pub fn draw(p: &egui::Painter, d: &DocState, to_screen: &dyn Fn(f64, f64) -> Pos2) {
    if d.in_bitmap_mode() {
        return;
    }
    let path = d.animation_path();
    if path.len() < 2 {
        return;
    }
    let pts: Vec<Pos2> = path.iter().map(|&(x, y)| to_screen(x, y)).collect();
    p.extend(Shape::dashed_line(&pts, Stroke::new(1.0, FRAME), 2.0, 3.0));
    let diamond = |c: Pos2, r: f32| vec![c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)];
    let last = pts.len() - 1;
    // The frames between first, then the ends on top.
    let order = (1..last).chain([0, last]);
    for i in order {
        let c = pts[i];
        let color = match i {
            0 => START,
            i if i == last => END,
            _ => FRAME,
        };
        p.add(Shape::convex_polygon(diamond(c, 4.5), Color32::WHITE, Stroke::NONE));
        p.add(Shape::convex_polygon(diamond(c, 3.5), color, Stroke::NONE));
    }
}

/// Drag the selected animation's motion path points with the Pointer. Returns whether the
/// pointer is busy with them (the canvas leaves the press alone then).
pub fn handles(
    drag: &mut Option<PathDrag>,
    d: &mut DocState,
    resp: &egui::Response,
    (origin, z): (Pos2, f32),
) -> bool {
    let ctx = resp.ctx.clone();
    let to_doc = |q: Pos2| (((q.x - origin.x) / z) as f64, ((q.y - origin.y) / z) as f64);
    // A drag from another document (the active one changed under it) can't continue here.
    if drag.as_ref().is_some_and(|active| active.doc != d.id) {
        *drag = None;
    }
    let Some((r, _)) = d.selected_animation().filter(|_| !d.in_bitmap_mode()) else {
        if drag.take().is_some() {
            d.discard_live_edit();
        }
        return false;
    };
    let tol = (GRIP / z.max(1e-3)) as f64;
    let (pressed, down, origin_at, now, shift) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.press_origin(),
            i.pointer.interact_pos(),
            i.modifiers.shift,
        )
    });
    if pressed
        && resp.hovered()
        && drag.is_none()
        && let Some(s) = origin_at.or(now)
    {
        let at = to_doc(s);
        if let Some(point) = d.motion_path_point_at(at.0, at.1, tol)
            && let Some(orig) = d.element(r).cloned()
        {
            // Fireworks MX names the step after the instance's properties.
            d.begin_live_edit("Instance Properties");
            *drag = Some(PathDrag {
                doc: d.id,
                point,
                press: at,
                orig,
                moved: false,
            });
        }
    }
    let Some(active) = drag.as_mut() else {
        return false;
    };
    if down && let Some(s) = now {
        let cur = to_doc(s);
        if cur != active.press || active.moved {
            active.moved = true;
            d.drag_motion_path(&active.orig, active.point, active.press, cur, shift);
            ctx.request_repaint();
        }
    }
    if !down {
        if active.moved {
            d.commit_live_edit();
        } else {
            d.discard_live_edit();
        }
        *drag = None;
    }
    true
}
