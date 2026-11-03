//! Lines with the stroke of a template document's first path, at chosen sub-pixel positions,
//! for measuring Fireworks' strokes (open the result in Fireworks and re-save it; its render
//! is in the re-saved file):  stroke_probe <template.png> <out.png>
//!
//! Layout (800x600 canvas): 32 horizontal lines (y = 20 + 8k + k/32, x 20..60), 32 vertical
//! lines (x = 100 + 8k + k/32, y 20..60), and lines from (380 + 60 (k mod 7), 200 + 100 (k div 7))
//! at 0..90 degrees in 5-degree steps (k = 0..18), 50 px long, going up-right.
//!
//! `stroke_probe <template.png> <out.png> brushes`: a 1600x1200 grid of 200x150 cells, one per
//! brush (diameters 1 2 3 4 5 7 9 15 by column pairs... see `BRUSHES`): in each cell horizontal
//! lines at y = top + 20 + 30 i + f (f = 0, 0.25, 0.5; x left+20..left+120) and a 30-degree line
//! from (left+20, top+140), 50 px long. Cell n: column n mod 8, row n div 8; brush n: diameter
//! DIAMETERS[n mod 8], softness SOFTNESS[(n div 8) mod 4] (tenths of a percent), anti-aliased for
//! n < 32.
use firecraft_doc::{BezierPoint, Contour, ElementKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut doc = firecraft_io::open(&std::fs::read(a.get(1).ok_or("template")?)?, None)?;
    let (li, template) = doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(li, l)| {
            l.elements(0)
                .iter()
                .find(|e| matches!(e.kind, ElementKind::Path(_)))
                .map(|e| (li, e.clone()))
        })
        .ok_or("no path in the template")?;
    let line = |x0: f64, y0: f64, x1: f64, y1: f64| {
        let mut e = template.clone();
        if let ElementKind::Path(p) = &mut e.kind {
            p.contours = vec![Contour {
                points: vec![BezierPoint::corner(x0, y0), BezierPoint::corner(x1, y1)],
                closed: false,
            }];
            p.rect = None;
        }
        e
    };
    let mut els = Vec::new();
    if a.get(3).map(String::as_str) == Some("brushes") {
        const DIAMETERS: [i64; 8] = [1, 2, 3, 4, 5, 7, 9, 15];
        const SOFTNESS: [i64; 4] = [0, 250, 500, 1000];
        doc.width = 1600;
        doc.height = 1200;
        for n in 0..64usize {
            let (left, top) = (200.0 * (n % 8) as f64, 150.0 * (n / 8) as f64);
            let (dia, soft, aa) = (DIAMETERS[n % 8], SOFTNESS[(n / 8) % 4], n < 32);
            let brush = |mut e: firecraft_doc::Element| {
                if let Some(bpl) = e
                    .raw
                    .as_mut()
                    .and_then(|r| r.child_mut("PAT"))
                    .and_then(|p| p.child_mut("BPL"))
                {
                    for c in &mut bpl.children {
                        if c.is("BDI") {
                            c.text = format!("{dia:x}");
                        } else if c.is("BSE") {
                            c.text = format!("{soft:x}");
                        } else if c.is("BIA") || c.is("BFB") {
                            c.text = if aa { "1" } else { "0" }.into();
                        }
                    }
                }
                if let ElementKind::Path(p) = &mut e.kind
                    && let Some(s) = &mut p.stroke
                {
                    s.diameter = dia as f64;
                    s.softness = (soft / 10) as u8;
                }
                e
            };
            for (i, f) in [0.0, 0.25, 0.5].into_iter().enumerate() {
                let y = top + 20.0 + 30.0 * i as f64 + f;
                els.push(brush(line(left + 20.0, y, left + 120.0, y)));
            }
            let (s, c) = 30f64.to_radians().sin_cos();
            els.push(brush(line(
                left + 20.0,
                top + 140.0,
                left + 20.0 + 50.0 * c,
                top + 140.0 - 50.0 * s,
            )));
        }
        if let Some(cell) = doc.layers.get_mut(li).and_then(|l| l.elements_mut(0)) {
            *cell = els;
        }
        std::fs::write(a.get(2).ok_or("out")?, firecraft_io::save_fireworks_png(&doc)?)?;
        return Ok(());
    }
    for k in 0..32 {
        let f = k as f64 / 32.0;
        els.push(line(
            20.0,
            20.0 + 8.0 * k as f64 + f,
            60.0,
            20.0 + 8.0 * k as f64 + f,
        ));
        let x = 100.0 + 8.0 * k as f64 + f;
        els.push(line(x, 20.0, x, 60.0));
    }
    for k in 0..19 {
        let deg = 5.0 * k as f64;
        let (s, c) = deg.to_radians().sin_cos();
        // A grid with room around each line (no overlaps): 7 per row, rows at y 200/300/400.
        let (x0, y0) = (380.0 + 60.0 * (k % 7) as f64, 200.0 + 100.0 * (k / 7) as f64);
        els.push(line(x0, y0, x0 + 50.0 * c, y0 - 50.0 * s));
    }
    if let Some(cell) = doc.layers.get_mut(li).and_then(|l| l.elements_mut(0)) {
        *cell = els;
    }
    std::fs::write(a.get(2).ok_or("out")?, firecraft_io::save_fireworks_png(&doc)?)?;
    Ok(())
}
