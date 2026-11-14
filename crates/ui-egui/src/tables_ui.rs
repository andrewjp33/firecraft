//! The Levels and Curves settings popups (Live Effects and the Filters menu): channel choice,
//! the object's histogram, Fireworks' input/output level handles and gamma, the curve editor
//! with its points, and Auto.
//!
//! Point editing follows Fireworks (as reconstructed in FirePixel's `fp-ui/src/dialogs/curves.rs`):
//! a click near the curve adds a point at least 4 levels from its neighbours; dragging an inner
//! point past a neighbour or out of the graph removes it; the end points stay and keep 4 levels
//! from their neighbour.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use firecraft_doc::EffectEntry;
use firecraft_raster::filters;

/// The popups' state: channel, the dragged/selected curve point, the object's histogram.
#[derive(Clone, Debug, Default)]
pub struct TableEditor {
    /// 0 = RGB, 1..3 = red, green, blue.
    pub channel: usize,
    /// R, G and B histograms of the object's pixels as they reach the effect.
    pub hist: Option<Box<[[u64; 256]; 3]>>,
    selected: Option<usize>,
    drag: Option<Drag>,
}

#[derive(Clone, Debug)]
struct Drag {
    idx: usize,
    /// Taken out of the list while dragged past a neighbour or out of the graph.
    removed: bool,
}

const CHANNELS: [&str; 4] = ["rgb", "red", "green", "blue"];
const CHANNEL_NAMES: [&str; 4] = ["RGB", "Red", "Green", "Blue"];
/// Minimum distance (levels) between neighbouring curve points.
const GAP: f32 = 4.0;
/// Hit box half-size of a curve point, in pixels.
const HIT: f32 = 4.0;
const GRAPH: f32 = 256.0;

impl TableEditor {
    /// Fresh state for a popup over an object whose pixels reaching the effect are `input`.
    pub fn new(input: Option<&firecraft_raster::RgbaImage>) -> Self {
        TableEditor {
            hist: input.map(|i| Box::new(filters::histograms(i))),
            ..Self::default()
        }
    }

    /// The histogram shown for the current channel (RGB: the three added together).
    fn shown(&self) -> Option<[u64; 256]> {
        let h = self.hist.as_ref()?;
        Some(match self.channel {
            0 => std::array::from_fn(|i| h[0][i] + h[1][i] + h[2][i]),
            c => h[c - 1],
        })
    }
}

fn channel_choice(ui: &mut Ui, st: &mut TableEditor) {
    ui.horizontal(|ui| {
        ui.label("Channel:");
        egui::ComboBox::from_id_salt("table_channel")
            .selected_text(CHANNEL_NAMES[st.channel.min(3)])
            .show_ui(ui, |ui| {
                for (i, n) in CHANNEL_NAMES.iter().enumerate() {
                    if ui.selectable_label(st.channel == i, *n).clicked() {
                        st.channel = i;
                        st.selected = None;
                        st.drag = None;
                    }
                }
            });
    });
}

/// The histogram, scaled so the tallest bars (the mean of the five largest) fill the height.
fn paint_histogram(ui: &Ui, r: Rect, hist: Option<[u64; 256]>, color: Color32) {
    let Some(h) = hist else { return };
    let mut top = h;
    top.sort_unstable_by(|a, b| b.cmp(a));
    let scale = (top.iter().take(5).sum::<u64>() / 5).max(1) as f32;
    let p = ui.painter_at(r);
    for (i, &v) in h.iter().enumerate() {
        if v == 0 {
            continue;
        }
        let x = r.left() + (i as f32 + 0.5) * r.width() / 256.0;
        let len = (v as f32 / scale).min(1.0) * r.height();
        p.line_segment([pos2(x, r.bottom()), pos2(x, r.bottom() - len)], Stroke::new(r.width() / 256.0, color));
    }
}

// ---------------------------------------------------------------- Levels

#[derive(Clone, Copy)]
struct Levels {
    low: f64,
    high: f64,
    out_low: f64,
    out_high: f64,
    gamma: f64,
}

fn read_levels(e: &EffectEntry, c: usize) -> Levels {
    let n = |k: &str, d: f64| {
        e.get(&format!("{k}_{}", CHANNELS[c]))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(d)
    };
    Levels {
        low: n("source_low", 0.0),
        high: n("source_high", 255.0),
        out_low: n("dest_low", 0.0),
        out_high: n("dest_high", 255.0),
        gamma: n("gamma", 1.0),
    }
}

fn number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

fn write_levels(e: &mut EffectEntry, c: usize, l: Levels) {
    let ch = CHANNELS[c];
    e.set(&format!("source_low_{ch}"), number(l.low.round(), 0));
    e.set(&format!("source_high_{ch}"), number(l.high.round(), 0));
    e.set(&format!("dest_low_{ch}"), number(l.out_low.round(), 0));
    e.set(&format!("dest_high_{ch}"), number(l.out_high.round(), 0));
    e.set(&format!("gamma_{ch}"), number(l.gamma, 2));
}

/// The grey handle's place between black and white for a gamma: where the input maps to mid-grey.
fn gamma_to_pos(g: f64, low: f64, high: f64) -> f64 {
    low + (high - low) * 0.5f64.powf(g.clamp(0.1, 9.99))
}

fn pos_to_gamma(p: f64, low: f64, high: f64) -> f64 {
    let t = ((p - low) / (high - low)).clamp(0.001, 0.999);
    (t.ln() / 0.5f64.ln()).clamp(0.1, 9.99)
}

/// A handle (triangle) on a slider track; returns the drag's new value when it moves.
fn handle(ui: &mut Ui, track: Rect, value: f64, fill: Color32, id: &str) -> Option<f64> {
    let x = track.left() + value as f32 / 255.0 * track.width();
    let r = Rect::from_center_size(pos2(x, track.center().y), vec2(11.0, track.height()));
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::drag());
    let tip = pos2(x, track.top());
    ui.painter().add(egui::Shape::convex_polygon(
        vec![tip, pos2(x + 5.0, track.bottom()), pos2(x - 5.0, track.bottom())],
        fill,
        Stroke::new(1.0, Color32::from_gray(60)),
    ));
    if resp.dragged()
        && let Some(p) = resp.interact_pointer_pos()
    {
        return Some(((p.x - track.left()) / track.width() * 255.0).clamp(0.0, 255.0) as f64);
    }
    None
}

/// The Levels popup; true when the effect changed.
pub fn levels(ui: &mut Ui, e: &mut EffectEntry, st: &mut TableEditor) -> bool {
    channel_choice(ui, st);
    let c = st.channel.min(3);
    let mut l = read_levels(e, c);
    let before = (l.low, l.high, l.out_low, l.out_high, l.gamma);
    ui.horizontal(|ui| {
        ui.label("Input Levels:");
        let hi = l.high;
        ui.add(egui::DragValue::new(&mut l.low).range(0.0..=(hi - 2.0).max(0.0)).max_decimals(0));
        ui.add(egui::DragValue::new(&mut l.gamma).range(0.1..=9.99).speed(0.01).fixed_decimals(2));
        let lo = l.low;
        ui.add(egui::DragValue::new(&mut l.high).range((lo + 2.0).min(255.0)..=255.0).max_decimals(0));
    });
    let (r, _) = ui.allocate_exact_size(vec2(GRAPH, 110.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
    paint_histogram(ui, r.shrink(1.0), st.shown(), Color32::from_gray(40));
    let (track, _) = ui.allocate_exact_size(vec2(GRAPH, 12.0), Sense::hover());
    let mid = gamma_to_pos(l.gamma, l.low, l.high);
    if let Some(v) = handle(ui, track, l.low, Color32::BLACK, "lv_low") {
        l.low = v.round().min(l.high - 2.0).max(0.0);
    }
    if let Some(v) = handle(ui, track, mid, Color32::from_gray(128), "lv_mid") {
        l.gamma = (pos_to_gamma(v, l.low, l.high) * 100.0).round() / 100.0;
    }
    if let Some(v) = handle(ui, track, l.high, Color32::WHITE, "lv_high") {
        l.high = v.round().max(l.low + 2.0).min(255.0);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Output Levels:");
        ui.add(egui::DragValue::new(&mut l.out_low).range(0.0..=255.0).max_decimals(0));
        ui.add(egui::DragValue::new(&mut l.out_high).range(0.0..=255.0).max_decimals(0));
    });
    let (track, _) = ui.allocate_exact_size(vec2(GRAPH, 12.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_size(track.min, vec2(track.width(), 4.0)),
        0.0,
        Color32::from_gray(200),
    );
    if let Some(v) = handle(ui, track, l.out_low, Color32::BLACK, "lv_out_low") {
        l.out_low = v.round();
    }
    if let Some(v) = handle(ui, track, l.out_high, Color32::WHITE, "lv_out_high") {
        l.out_high = v.round();
    }
    let mut changed = before != (l.low, l.high, l.out_low, l.out_high, l.gamma);
    if changed {
        write_levels(e, c, l);
    }
    ui.add_space(4.0);
    if ui
        .add_enabled(st.hist.is_some(), egui::Button::new("Auto"))
        .on_hover_text("Stretch each of red, green and blue to the full range")
        .clicked()
        && let Some(h) = st.hist.as_ref()
    {
        write_levels(
            e,
            0,
            Levels {
                low: 0.0,
                high: 255.0,
                out_low: 0.0,
                out_high: 255.0,
                gamma: 1.0,
            },
        );
        for c in 0..3 {
            let (lo, hi) = filters::histogram_bounds(&h[c]);
            write_levels(
                e,
                c + 1,
                Levels {
                    low: lo as f64,
                    high: hi as f64,
                    out_low: 0.0,
                    out_high: 255.0,
                    gamma: 1.0,
                },
            );
        }
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------- Curves

fn points_key(c: usize) -> String {
    format!("{}_points", CHANNELS[c])
}

fn read_points(e: &EffectEntry, c: usize) -> Vec<(f32, f32)> {
    match e.points(&points_key(c)) {
        Some(p) if !p.is_empty() => p,
        _ => vec![(0.0, 0.0), (255.0, 255.0)],
    }
}

/// A new Curves effect's point lists: straight lines.
pub fn init_curves(e: &mut EffectEntry) {
    for c in 0..4 {
        if e.points(&points_key(c)).is_none_or(|p| p.is_empty()) {
            e.set_points(&points_key(c), &[(0.0, 0.0), (255.0, 255.0)]);
        }
    }
}

fn to_px(r: Rect, p: (f32, f32)) -> Pos2 {
    pos2(
        r.left() + p.0 / 255.0 * r.width(),
        r.bottom() - p.1 / 255.0 * r.height(),
    )
}

fn to_curve(r: Rect, p: Pos2) -> (f32, f32) {
    (
        ((p.x - r.left()) / r.width() * 255.0).round(),
        ((r.bottom() - p.y) / r.height() * 255.0).round(),
    )
}

/// The Curves popup; true when the effect changed.
pub fn curves(ui: &mut Ui, e: &mut EffectEntry, st: &mut TableEditor) -> bool {
    channel_choice(ui, st);
    let c = st.channel.min(3);
    let mut pts = read_points(e, c);
    let before = pts.clone();
    let (r, resp) = ui.allocate_exact_size(vec2(GRAPH, GRAPH), Sense::click_and_drag());
    let painter = ui.painter_at(r.expand(1.0));
    painter.rect_filled(r, 0.0, Color32::WHITE);
    paint_histogram(ui, r, st.shown(), Color32::from_gray(225));
    for i in 1..4 {
        let t = i as f32 / 4.0;
        let g = Stroke::new(1.0, Color32::from_gray(205));
        painter.line_segment([pos2(r.left() + t * r.width(), r.top()), pos2(r.left() + t * r.width(), r.bottom())], g);
        painter.line_segment([pos2(r.left(), r.top() + t * r.height()), pos2(r.right(), r.top() + t * r.height())], g);
    }
    painter.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.0, Color32::from_gray(215)));
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);

    // Mouse: press on a point picks it, near the curve adds one; dragging moves it.
    // Where the button went down (a drag is reported only once the mouse has moved).
    let origin = ui.input(|i| i.pointer.press_origin());
    if resp.drag_started()
        && let Some(p) = origin.or(resp.interact_pointer_pos())
    {
        let hit = pts.iter().position(|&q| {
            let s = to_px(r, q);
            (s.x - p.x).abs() <= HIT && (s.y - p.y).abs() <= HIT
        });
        let (x, y) = to_curve(r, p);
        if let Some(i) = hit {
            st.selected = Some(i);
            st.drag = Some(Drag { idx: i, removed: false });
        } else {
            let tab = filters::curve_table(&pts);
            let near = (x - GAP).max(0.0) as usize..=(x + GAP).min(255.0) as usize;
            let on_curve = near.into_iter().any(|i| (tab[i] as f32 - y).abs() <= GAP);
            if let Some(k) = pts.iter().position(|q| q.0 >= x)
                && k > 0
                && on_curve
                && pts[k - 1].0 + GAP < x
                && x < pts[k].0 - GAP
            {
                pts.insert(k, (x, y));
                st.selected = Some(k);
                st.drag = Some(Drag { idx: k, removed: false });
            }
        }
    } else if resp.dragged()
        && let (Some(p), Some(mut dr)) = (resp.interact_pointer_pos(), st.drag.clone())
    {
        let (x, y) = to_curve(r, p);
        let i = dr.idx;
        let n = pts.len();
        if !dr.removed && (i == 0 || i + 1 == n) {
            let mut nx = x.clamp(0.0, 255.0);
            if n >= 2 {
                nx = if i == 0 { nx.min(pts[1].0 - GAP) } else { nx.max(pts[n - 2].0 + GAP) };
            }
            pts[i] = (nx, y.clamp(0.0, 255.0));
        } else if i > 0 {
            let (left, right) = if dr.removed {
                (pts[i - 1].0, pts.get(i).map_or(255.0, |q| q.0))
            } else {
                (pts[i - 1].0, pts[i + 1].0)
            };
            let inside = (0.0..=255.0).contains(&x) && (0.0..=255.0).contains(&y);
            let valid = inside && left + GAP < x && x < right - GAP;
            match (dr.removed, valid) {
                (false, true) => pts[i] = (x, y),
                (false, false) => {
                    pts.remove(i);
                    dr.removed = true;
                    st.selected = None;
                }
                (true, true) => {
                    pts.insert(i, (x, y));
                    dr.removed = false;
                    st.selected = Some(i);
                }
                _ => {}
            }
        }
        st.drag = Some(dr);
    } else if resp.drag_stopped() {
        st.drag = None;
    }

    // The curve and its points.
    let tab = filters::curve_table(&pts);
    let line: Vec<Pos2> = (0..256).map(|i| to_px(r, (i as f32, tab[i] as f32))).collect();
    painter.add(egui::Shape::line(line, Stroke::new(1.5, Color32::BLACK)));
    for (i, &q) in pts.iter().enumerate() {
        let s = Rect::from_center_size(to_px(r, q), vec2(6.0, 6.0));
        if st.selected == Some(i) {
            painter.rect_filled(s, 0.0, Color32::BLACK);
        } else {
            painter.rect_filled(s, 0.0, Color32::WHITE);
            painter.rect_stroke(s, 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Inside);
        }
    }

    // The selected point's values.
    ui.horizontal(|ui| {
        let sel = st.selected.filter(|&i| i < pts.len());
        let (mut x, mut y) = sel.map_or((0.0, 0.0), |i| pts[i]);
        ui.label("Input:");
        let ix = ui.add_enabled(sel.is_some(), egui::DragValue::new(&mut x).range(0.0..=255.0).max_decimals(0));
        ui.label("Output:");
        let iy = ui.add_enabled(sel.is_some(), egui::DragValue::new(&mut y).range(0.0..=255.0).max_decimals(0));
        if (ix.changed() || iy.changed())
            && let Some(i) = sel
        {
            let left = (i > 0).then(|| pts[i - 1].0);
            let right = pts.get(i + 1).map(|q| q.0);
            let x = x.round().clamp(left.unwrap_or(0.0), right.unwrap_or(255.0));
            if left.is_none_or(|l| l + GAP < x) && right.is_none_or(|rr| x < rr - GAP) {
                pts[i] = (x, y.round());
            }
        }
    });
    let mut changed = pts != before;
    if changed {
        e.set_points(&points_key(c), &pts);
    }
    if ui
        .add_enabled(st.hist.is_some(), egui::Button::new("Auto"))
        .on_hover_text("Stretch each of red, green and blue to the full range")
        .clicked()
        && let Some(h) = st.hist.as_ref()
    {
        e.set_points(&points_key(0), &[(0.0, 0.0), (255.0, 255.0)]);
        for c in 0..3 {
            let (lo, hi) = filters::histogram_bounds(&h[c]);
            e.set_points(&points_key(c + 1), &[(lo as f32, 0.0), (hi as f32, 255.0)]);
        }
        st.selected = None;
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_handle_round_trip() {
        for g in [0.5, 1.0, 2.0, 4.0] {
            let p = gamma_to_pos(g, 10.0, 240.0);
            assert!((pos_to_gamma(p, 10.0, 240.0) - g).abs() < 1e-6);
        }
        assert_eq!(gamma_to_pos(1.0, 0.0, 255.0), 127.5);
    }

    #[test]
    fn numbers_trimmed() {
        assert_eq!(number(1.0, 2), "1");
        assert_eq!(number(2.5, 2), "2.5");
        assert_eq!(number(30.0, 0), "30");
    }
}
