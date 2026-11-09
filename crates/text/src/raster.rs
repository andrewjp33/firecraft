//! Rasterizer für Glyphen: Konturen → Strecken → **Punktabtastung N×N** je Pixel (09 §6.1/§10).
//!
//! Abtastpunkte liegen bei `(i + (m + 0,5)/N, j + (k + 0,5)/N)`, Regel Non-Zero, die
//! hochaufgelöste Maske ist binär. Gezählt wird die Zahl der getroffenen Unterpixel (0…N²); die
//! Tonkurve ([`crate::antialias`]) macht daraus Alpha. Kurven werden vorher fein unterteilt
//! (Toleranz 0,05 px wie die Python-Referenz `render_check2.py`). Das Hinting der Konturen macht
//! [`crate::render`] vor der Abtastung (FX-T).

use crate::font::outline::{P2, Segment, flatten_segment};

/// Strecke in Pixelkoordinaten relativ zum Rasterursprung (y nach unten).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Abflachungstoleranz der Rasterung in px.
pub const RASTER_TOLERANCE: f64 = 0.05;

/// Affine Abbildung Font-Einheiten → Pixel: `X = e + a·x + c·y`, `Y = f + b·x + d·y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    /// Einheitsabbildung.
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Punkt abbilden. Reihenfolge der Rechnung wie die Python-Referenz (`ox + sx·x`).
    #[inline]
    pub fn apply(&self, p: P2) -> P2 {
        if self.b == 0.0 && self.c == 0.0 {
            [self.e + self.a * p[0], self.f + self.d * p[1]]
        } else {
            [
                self.e + self.a * p[0] + self.c * p[1],
                self.f + self.b * p[0] + self.d * p[1],
            ]
        }
    }

    /// Verschoben um (dx, dy).
    pub fn translated(&self, dx: f64, dy: f64) -> Affine {
        Affine {
            e: self.e + dx,
            f: self.f + dy,
            ..*self
        }
    }

    /// Erst `self`, dann `o` (o ∘ self).
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            a: o.a * self.a + o.c * self.b,
            b: o.b * self.a + o.d * self.b,
            c: o.a * self.c + o.c * self.d,
            d: o.b * self.c + o.d * self.d,
            e: o.a * self.e + o.c * self.f + o.e,
            f: o.b * self.e + o.d * self.f + o.f,
        }
    }
}

/// Fügt die abgeflachten Strecken einer Kontur hinzu: erst abbilden, dann abflachen (wie die
/// Referenz, die die Kontrollpunkte transformiert und in Pixeln unterteilt).
pub fn push_contour(edges: &mut Vec<Edge>, contour: &[Segment], m: &Affine, tol: f64) {
    for s in contour {
        let t = s.map(|p| m.apply(p));
        flatten_segment(&t, tol, &mut |a, b| {
            edges.push(Edge {
                x0: a[0],
                y0: a[1],
                x1: b[0],
                y1: b[1],
            })
        });
    }
}

/// Fügt ein Polygon (geschlossen) als Strecken hinzu.
pub fn push_polygon(edges: &mut Vec<Edge>, pts: &[P2]) {
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        edges.push(Edge {
            x0: a[0],
            y0: a[1],
            x1: b[0],
            y1: b[1],
        });
    }
}

/// Zählt je Pixel die Abtastpunkte innerhalb der Konturen (Non-Zero). Ergebnis zeilenweise,
/// `w × h` Werte 0…n².
///
/// Je Unterzeile y_s = (s + 0,5)/n werden die Schnittpunkte aller Strecken mit `y_lo ≤ y_s < y_hi`
/// bestimmt (`x = x0 + t·(x1 − x0)` mit `t = (y_s − y0)/(y1 − y0)`), sortiert und mit der
/// Windungsrichtung aufsummiert; ein Abtastpunkt ist innen, wenn die Summe der Schnittpunkte mit
/// `x ≤ x_s` ungleich 0 ist.
pub fn sample_counts(edges: &[Edge], w: usize, h: usize, n: u32) -> Vec<u16> {
    let n = n.max(1) as usize;
    let mut counts = vec![0u16; w * h];
    if edges.is_empty() || w == 0 || h == 0 {
        return counts;
    }
    // Strecken nach unterer y-Grenze sortiert, nicht waagerechte nur
    struct E {
        ylo: f64,
        yhi: f64,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        dir: i32,
    }
    let mut es: Vec<E> = edges
        .iter()
        .filter(|e| {
            e.y0 != e.y1 && e.x0.is_finite() && e.x1.is_finite() && e.y0.is_finite() && e.y1.is_finite()
        })
        .map(|e| E {
            ylo: e.y0.min(e.y1),
            yhi: e.y0.max(e.y1),
            x0: e.x0,
            y0: e.y0,
            dx: e.x1 - e.x0,
            dy: e.y1 - e.y0,
            dir: if e.y1 > e.y0 { 1 } else { -1 },
        })
        .collect();
    es.sort_by(|a, b| a.ylo.total_cmp(&b.ylo));
    let nf = n as f64;
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0usize;
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for sy in 0..h * n {
        let ys = (sy as f64 + 0.5) / nf;
        while next < es.len() && es[next].ylo <= ys {
            active.push(next);
            next += 1;
        }
        active.retain(|&i| es[i].yhi > ys);
        if active.is_empty() {
            continue;
        }
        xs.clear();
        for &i in &active {
            let e = &es[i];
            if e.ylo <= ys && e.yhi > ys {
                let t = (ys - e.y0) / e.dy;
                xs.push((e.x0 + t * e.dx, e.dir));
            }
        }
        if xs.is_empty() {
            continue;
        }
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let row = &mut counts[(sy / n) * w..(sy / n + 1) * w];
        // Spannen mit Windung ≠ 0 zwischen aufeinanderfolgenden Schnittpunkten füllen
        let mut wind = 0i32;
        for k in 0..xs.len() {
            wind += xs[k].1;
            if wind == 0 {
                continue;
            }
            let xa = xs[k].0;
            let xb = if k + 1 < xs.len() {
                xs[k + 1].0
            } else {
                f64::INFINITY
            };
            // Abtastspalten j mit xa ≤ (j + 0,5)/n < xb; Schätzung, dann exakte Randkorrektur mit
            // demselben Ausdruck wie die Referenz (`searchsorted(side="right")`: x ≤ x_s zählt)
            let wn = w * n;
            let xs_of = |j: usize| (j as f64 + 0.5) / nf;
            let mut j = ((xa * nf - 0.5).ceil()).clamp(0.0, wn as f64) as usize;
            while j > 0 && xs_of(j - 1) >= xa {
                j -= 1;
            }
            while j < wn && xs_of(j) < xa {
                j += 1;
            }
            let mut je = ((xb * nf - 0.5).ceil()).clamp(0.0, wn as f64) as usize;
            while je > j && xs_of(je - 1) >= xb {
                je -= 1;
            }
            while je < wn && xs_of(je) < xb {
                je += 1;
            }
            for jj in j..je {
                row[jj / n] += 1;
            }
        }
    }
    counts
}

/// Verbreitert geschlossene Polygone (Faux-Fett, A): jede Ecke wandert um `d` px entlang der
/// Winkelhalbierenden der Kantennormalen nach außen (Gehrung auf 2·d begrenzt). „Außen“ richtet sich
/// nach der Gesamtorientierung aller Konturen, Innenkonturen (Löcher) werden dadurch kleiner.
pub fn embolden(polys: &mut [Vec<P2>], d: f64) {
    if d == 0.0 {
        return;
    }
    let area = |p: &Vec<P2>| -> f64 {
        let n = p.len();
        (0..n)
            .map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1])
            .sum::<f64>()
            / 2.0
    };
    let total: f64 = polys.iter().map(area).sum();
    // Bildschirm (y nach unten): positive Fläche = im Uhrzeigersinn; Normale nach außen = links der
    // Laufrichtung bei negativer, rechts bei positiver Orientierung
    let sign = if total >= 0.0 { 1.0 } else { -1.0 };
    for p in polys.iter_mut() {
        let n = p.len();
        if n < 3 {
            continue;
        }
        let src = p.clone();
        for i in 0..n {
            let a = src[(i + n - 1) % n];
            let b = src[i];
            let c = src[(i + 1) % n];
            let n1 = normal(a, b, sign);
            let n2 = normal(b, c, sign);
            let (mx, my) = (n1[0] + n2[0], n1[1] + n2[1]);
            let ml = (mx * mx + my * my).sqrt();
            if ml < 1e-9 {
                p[i] = [b[0] + n1[0] * d, b[1] + n1[1] * d];
                continue;
            }
            let (ux, uy) = (mx / ml, my / ml);
            let cos = ux * n1[0] + uy * n1[1];
            let len = if cos > 0.5 { d / cos } else { 2.0 * d };
            p[i] = [b[0] + ux * len, b[1] + uy * len];
        }
    }
}

fn normal(a: P2, b: P2, sign: f64) -> P2 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = (dx * dx + dy * dy).sqrt();
    if l < 1e-12 {
        return [0.0, 0.0];
    }
    // Bei positiver Fläche (im y-unten-System im Uhrzeigersinn) zeigt (dy, −dx) nach außen
    [dy / l * sign, -dx / l * sign]
}

/// Flacht Konturen zu Polygonen ab (für Faux-Fett).
pub fn contours_to_polygons(contours: &[Vec<Segment>], m: &Affine, tol: f64) -> Vec<Vec<P2>> {
    contours
        .iter()
        .map(|c| {
            let mut pts: Vec<P2> = Vec::new();
            for s in c {
                let t = s.map(|p| m.apply(p));
                flatten_segment(&t, tol, &mut |a, b| {
                    if pts.last() != Some(&a) {
                        pts.push(a);
                    }
                    pts.push(b);
                });
            }
            if pts.len() > 1 && pts.first() == pts.last() {
                pts.pop();
            }
            pts
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Edge> {
        let mut e = Vec::new();
        push_polygon(&mut e, &[[x0, y0], [x1, y0], [x1, y1], [x0, y1]]);
        e
    }

    #[test]
    fn full_and_partial_pixels() {
        let c = sample_counts(&rect(1.0, 0.0, 2.5, 1.0), 3, 1, 4);
        assert_eq!(c, vec![0, 16, 8]);
        let c1 = sample_counts(&rect(0.4, 0.4, 0.6, 0.6), 1, 1, 1);
        assert_eq!(c1, vec![1]);
        let c0 = sample_counts(&rect(0.0, 0.0, 0.4, 0.4), 1, 1, 1);
        assert_eq!(c0, vec![0]);
    }

    #[test]
    fn boundary_sample_counts_as_inside_left() {
        // Abtastpunkt genau auf der linken Kante (x = 0,125) zählt, auf der rechten nicht
        let c = sample_counts(&rect(0.125, 0.0, 0.375, 1.0), 1, 1, 4);
        assert_eq!(c, vec![4]);
    }

    #[test]
    fn nonzero_union_of_overlaps() {
        let mut e = rect(0.0, 0.0, 1.0, 1.0);
        e.extend(rect(0.0, 0.0, 1.0, 1.0));
        assert_eq!(sample_counts(&e, 1, 1, 4), vec![16]);
    }

    #[test]
    fn embolden_grows_square() {
        let mut p = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]];
        embolden(&mut p, 1.0);
        assert!((p[0][0][0] + 1.0).abs() < 1e-9 && (p[0][0][1] + 1.0).abs() < 1e-9);
        assert!((p[0][2][0] - 11.0).abs() < 1e-9);
    }
}
