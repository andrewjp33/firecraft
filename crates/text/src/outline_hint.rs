//! Rasterangleichung der Glyphenkonturen für „Text ▸ In Pfade konvertieren“ (TW-56b, re 33 §1).
//!
//! Soll-Regel: Senkrechte Stammkanten und waagrechte Zonen (Grundlinie, x-Höhe, Versalhöhe) liegen
//! nach dem Umwandeln auf ganzen Pixeln, die Stammbreite bleibt mindestens 1 px, und jede so
//! gerasterte Kante sitzt danach 1/16 px zur Tinte hin versetzt (n + 0,0625 bzw. n + 0,9375).
//! Kurven und Schrägen werden nicht gerastert, sie folgen den Kanten (stückweise lineare
//! Abbildung je Achse, wie das Nachziehen nicht berührter Punkte beim Hinting).
//!
//! Gerade, achsparallele Strecken einer Glyphe sind die Kanten. Auf welcher Seite die Tinte liegt,
//! ergibt ein Nonzero-Test knapp neben der Kantenmitte. Gedrehter Text hat keine achsparallelen
//! Strecken und bleibt deshalb unverändert.

use crate::support::{BezierContour, Point};

/// Versatz einer gerasterten Kante zur Tinte hin (1/16 px).
pub const EDGE_INSET: f32 = 1.0 / 16.0;
/// Kürzeste Strecke, die als Kante zählt (px).
const MIN_EDGE_LEN: f32 = 0.3;
/// Größter Abstand zweier Kanten, die zusammen einen Stamm bilden (px).
const MAX_STEM: f32 = 4.0;
/// Abstand, bis zu dem ein Rundungsextrem einer Kante folgt (px).
const EXTREME_SNAP: f32 = 0.25;
const EPS: f32 = 1e-3;
/// Henkel so nah an der Achse gelten als Tangente des Extrems (px).
const FLAT_TOL: f32 = 0.02;

#[derive(Clone, Copy, Debug)]
struct Edge {
    /// Lage auf der Achse (x für senkrechte, y für waagrechte Kanten).
    pos: f32,
    /// Tinte liegt auf der größeren Koordinate.
    ink_after: bool,
    /// Ausdehnung entlang der Kante.
    span: (f32, f32),
}

/// Gleicht die Konturen einer Glyphe an das Pixelraster an (siehe Modulkommentar).
pub fn hint_glyph(contours: &mut [BezierContour]) {
    let polys = flatten(contours);
    if polys.is_empty() {
        return;
    }
    for axis in [Axis::X, Axis::Y] {
        let edges = find_edges(contours, &polys, axis);
        if edges.is_empty() {
            continue;
        }
        let map = edge_map(&edges);
        // Rundungsextreme (senkrechte bzw. waagrechte Tangente) knapp neben einer Kante mit derselben
        // Tintenseite richten sich nach dieser Kante (Punkt des „i“, Überhang runder Bögen an
        // Grundlinie/x-Höhe), der Henkel bleibt dabei tangential
        let snaps: Vec<Vec<Option<f32>>> = contours
            .iter()
            .map(|c| {
                c.nodes
                    .iter()
                    .map(|n| {
                        let v = axis.get(n.pt);
                        let flat = (axis.get(n.ctrl_in) - v).abs() < FLAT_TOL
                            && (axis.get(n.ctrl_out) - v).abs() < FLAT_TOL;
                        let curved = (axis.other(n.ctrl_in) - axis.other(n.pt)).abs() > EPS
                            || (axis.other(n.ctrl_out) - axis.other(n.pt)).abs() > EPS;
                        if !flat || !curved {
                            return None;
                        }
                        let ink_after = probe(&polys, axis, v, axis.other(n.pt), 0.05)?;
                        edges
                            .iter()
                            .filter(|e| e.ink_after == ink_after && (e.pos - v).abs() <= EXTREME_SNAP)
                            .min_by(|a, b| (a.pos - v).abs().total_cmp(&(b.pos - v).abs()))
                            .map(|e| map_coord(&map, e.pos))
                    })
                    .collect()
            })
            .collect();
        for (c, snap) in contours.iter_mut().zip(snaps) {
            for (n, s) in c.nodes.iter_mut().zip(snap) {
                for p in [&mut n.pt, &mut n.ctrl_in, &mut n.ctrl_out] {
                    let v = axis.get(*p);
                    axis.set(p, s.unwrap_or_else(|| map_coord(&map, v)));
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn get(self, p: Point) -> f32 {
        match self {
            Axis::X => p.x,
            Axis::Y => p.y,
        }
    }
    fn other(self, p: Point) -> f32 {
        match self {
            Axis::X => p.y,
            Axis::Y => p.x,
        }
    }
    fn set(self, p: &mut Point, v: f32) {
        match self {
            Axis::X => p.x = v,
            Axis::Y => p.y = v,
        }
    }
}

fn segment_is_line(c: &BezierContour, i: usize) -> Option<(Point, Point)> {
    let n = c.nodes.len();
    let j = if i + 1 < n {
        i + 1
    } else if c.closed {
        0
    } else {
        return None;
    };
    let (a, b) = (&c.nodes[i], &c.nodes[j]);
    let on = |h: Point| {
        (h.x - a.pt.x).abs() < EPS && (h.y - a.pt.y).abs() < EPS
            || (h.x - b.pt.x).abs() < EPS && (h.y - b.pt.y).abs() < EPS
    };
    (on(a.ctrl_out) && on(b.ctrl_in)).then_some((a.pt, b.pt))
}

fn find_edges(contours: &[BezierContour], polys: &[Vec<Point>], axis: Axis) -> Vec<Edge> {
    let mut out = Vec::new();
    for c in contours {
        for i in 0..c.nodes.len() {
            let Some((a, b)) = segment_is_line(c, i) else {
                continue;
            };
            if (axis.get(a) - axis.get(b)).abs() > EPS || (axis.other(a) - axis.other(b)).abs() < MIN_EDGE_LEN
            {
                continue;
            }
            let pos = axis.get(a);
            let mid = (axis.other(a) + axis.other(b)) / 2.0;
            let Some(after) = probe(polys, axis, pos, mid, 0.05) else {
                continue;
            };
            let (s0, s1) = (axis.other(a).min(axis.other(b)), axis.other(a).max(axis.other(b)));
            out.push(Edge {
                pos,
                ink_after: after,
                span: (s0, s1),
            });
        }
    }
    out
}

/// Tintenseite an `pos` (Achse) / `other`: `Some(true)` = Tinte auf der größeren Koordinate,
/// `None` = beide oder keine Seite.
fn probe(polys: &[Vec<Point>], axis: Axis, pos: f32, other: f32, d: f32) -> Option<bool> {
    let at = |v: f32| {
        let mut p = Point::new(other, other);
        axis.set(&mut p, v);
        inside(polys, p)
    };
    let (before, after) = (at(pos - d), at(pos + d));
    (before != after).then_some(after)
}

/// Zielwerte der Kanten: auf ganze Pixel, Stamm ≥ 1 px, danach 1/16 px zur Tinte.
fn edge_map(edges: &[Edge]) -> Vec<(f32, f32)> {
    let mut target: Vec<f32> = edges.iter().map(|e| e.pos.round()).collect();
    // Stammbreite mindestens 1 px: zu jeder Kante mit Tinte danach die nächste passende Gegenkante
    for (i, l) in edges.iter().enumerate() {
        if !l.ink_after {
            continue;
        }
        let partner = edges
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                !r.ink_after
                    && r.pos > l.pos
                    && r.pos - l.pos <= MAX_STEM
                    && r.span.0 < l.span.1
                    && l.span.0 < r.span.1
            })
            .min_by(|a, b| a.1.pos.total_cmp(&b.1.pos));
        if let Some((j, _)) = partner
            && target[j] < target[i] + 1.0
        {
            target[j] = target[i] + 1.0;
        }
    }
    let mut map: Vec<(f32, f32)> = edges
        .iter()
        .zip(target)
        .map(|(e, t)| {
            (
                e.pos,
                if e.ink_after {
                    t + EDGE_INSET
                } else {
                    t - EDGE_INSET
                },
            )
        })
        .collect();
    map.sort_by(|a, b| a.0.total_cmp(&b.0));
    map.dedup_by(|b, a| (b.0 - a.0).abs() < EPS);
    // Abbildung monoton halten
    for i in 1..map.len() {
        if map[i].1 < map[i - 1].1 {
            map[i].1 = map[i - 1].1;
        }
    }
    map
}

fn map_coord(map: &[(f32, f32)], v: f32) -> f32 {
    let (first, last) = (map[0], map[map.len() - 1]);
    if v <= first.0 + EPS {
        return if (v - first.0).abs() < EPS {
            first.1
        } else {
            v + first.1 - first.0
        };
    }
    if v >= last.0 - EPS {
        return if (v - last.0).abs() < EPS {
            last.1
        } else {
            v + last.1 - last.0
        };
    }
    for w in map.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (v - b.0).abs() < EPS {
            return b.1;
        }
        if v < b.0 {
            let t = (v - a.0) / (b.0 - a.0);
            return a.1 + t * (b.1 - a.1);
        }
    }
    v
}

fn flatten(contours: &[BezierContour]) -> Vec<Vec<Point>> {
    const STEPS: usize = 8;
    contours
        .iter()
        .filter(|c| c.nodes.len() >= 2)
        .map(|c| {
            let n = c.nodes.len();
            let mut pts = Vec::with_capacity(n * STEPS);
            for i in 0..n {
                let j = (i + 1) % n;
                let (a, b) = (&c.nodes[i], &c.nodes[j]);
                let (p0, p1, p2, p3) = (a.pt, a.ctrl_out, b.ctrl_in, b.pt);
                for k in 0..STEPS {
                    let t = k as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    let f = |q0: f32, q1: f32, q2: f32, q3: f32| {
                        u * u * u * q0 + 3.0 * u * u * t * q1 + 3.0 * u * t * t * q2 + t * t * t * q3
                    };
                    pts.push(Point::new(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)));
                }
            }
            pts
        })
        .collect()
}

/// Nonzero-Regel über alle Konturen der Glyphe.
fn inside(polys: &[Vec<Point>], p: Point) -> bool {
    let mut wind = 0i32;
    for poly in polys {
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if a.y <= p.y {
                if b.y > p.y && cross(a, b, p) > 0.0 {
                    wind += 1;
                }
            } else if b.y <= p.y && cross(a, b, p) < 0.0 {
                wind -= 1;
            }
        }
    }
    wind != 0
}

fn cross(a: Point, b: Point, p: Point) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::BezierNode;

    fn rect(l: f32, t: f32, r: f32, b: f32) -> BezierContour {
        let c = |x, y| BezierNode::corner(Point::new(x, y));
        BezierContour::new(vec![c(r, b), c(r, t), c(l, t), c(l, b)], true)
    }

    /// Stamm des „i“ aus TX2 (re 33 §1): ungehintet 104,868…106,408 → 105,0625…105,9375.
    #[test]
    fn stem_of_i_lands_on_pixel_with_sixteenth_inset() {
        let mut g = vec![rect(104.868, 169.8, 106.408, 177.1)];
        hint_glyph(&mut g);
        let xs: Vec<f32> = g[0].nodes.iter().map(|n| n.pt.x).collect();
        let ys: Vec<f32> = g[0].nodes.iter().map(|n| n.pt.y).collect();
        assert_eq!(xs.iter().copied().fold(f32::MAX, f32::min), 105.0625);
        assert_eq!(xs.iter().copied().fold(f32::MIN, f32::max), 105.9375);
        assert_eq!(ys.iter().copied().fold(f32::MAX, f32::min), 170.0625);
        assert_eq!(ys.iter().copied().fold(f32::MIN, f32::max), 176.9375);
    }

    /// Dünner Stamm in einem Pixel: Breite bleibt 1 px.
    #[test]
    fn thin_stem_keeps_one_pixel() {
        let mut g = vec![rect(10.2, 0.0, 10.4, 5.0)];
        hint_glyph(&mut g);
        let xs: Vec<f32> = g[0].nodes.iter().map(|n| n.pt.x).collect();
        assert_eq!(xs.iter().copied().fold(f32::MAX, f32::min), 10.0625);
        assert_eq!(xs.iter().copied().fold(f32::MIN, f32::max), 10.9375);
    }

    /// Loch (Gegenform): Kanten rücken ebenfalls zur Tinte hin, das Loch wird größer.
    #[test]
    fn counter_edges_move_toward_ink() {
        let outer = rect(0.0, 0.0, 10.0, 10.0);
        let mut inner = rect(3.0, 3.0, 7.0, 7.0);
        inner.nodes.reverse();
        let mut g = vec![outer, inner];
        hint_glyph(&mut g);
        let xs: Vec<f32> = g[1].nodes.iter().map(|n| n.pt.x).collect();
        assert_eq!(xs.iter().copied().fold(f32::MAX, f32::min), 2.9375);
        assert_eq!(xs.iter().copied().fold(f32::MIN, f32::max), 7.0625);
    }
}
