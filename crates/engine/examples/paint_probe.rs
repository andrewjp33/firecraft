//! Compare FireCraft's painting tools with Fireworks MX: replays a sequence of mouse gestures
//! that was performed by hand in Fireworks MX (tools/fwmx-oracle) on a probe image, and reports
//! how the two flattened results differ.
//!
//!   cargo run -p firecraft-engine --example paint_probe -- probe.png fireworks_result.png [diff.png]
//!
//! STAGE=E stops after the Eraser, STAGE=F after the Paint Bucket.

use firecraft_doc::ElementKind;
use firecraft_engine::{DocState, PaintTool, ToolOptions};
use firecraft_raster::{Rgba, RgbaImage};

/// The Fireworks canvas's top-left pixel on the probe's screen.
const ORIGIN: (i64, i64) = (97, 124);

/// The harness's drag: five integer steps from press to release, in screen pixels.
fn drag(d: &mut DocState, tool: PaintTool, a: (i64, i64), b: (i64, i64), o: &ToolOptions) {
    let at = |p: (i64, i64)| ((p.0 - ORIGIN.0) as f64, (p.1 - ORIGIN.1) as f64);
    let (x, y) = at(a);
    d.begin_paint(tool, x, y, o);
    for s in 1..=5 {
        let (x, y) = at((a.0 + (b.0 - a.0) * s / 5, a.1 + (b.1 - a.1) * s / 5));
        d.extend_paint(x, y, false);
    }
    d.end_paint();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let [_, probe, want, rest @ ..] = a.as_slice() else {
        return Err("usage: paint_probe PROBE.png FIREWORKS.png [DIFF.png]".into());
    };
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(probe)?, Some(probe.clone()))?);
    let want = firecraft_io::textures::decode_image(&std::fs::read(want)?)?;
    let o = ToolOptions {
        stroke: Some(Rgba::BLACK),
        fill: want.get(100, 146),
        ..ToolOptions::default()
    };
    let at = |x: i64, y: i64| ((x - ORIGIN.0) as f64, (y - ORIGIN.1) as f64);

    drag(&mut d, PaintTool::Pencil, (150, 200), (230, 200), &o);
    d.deselect();
    drag(&mut d, PaintTool::Pencil, (320, 320), (420, 320), &o);
    drag(&mut d, PaintTool::Pencil, (340, 350), (440, 350), &o);
    let bitmaps = d
        .doc
        .layers
        .iter()
        .flat_map(|l| l.elements(0).iter())
        .filter(|e| matches!(e.kind, ElementKind::Bitmap(_)))
        .count();
    println!("bitmaps: {bitmaps} (Fireworks: 2)");
    d.deselect();
    let (x, y) = at(197, 230);
    d.selection = d.hit_test(x, y).into_iter().collect();
    let stage = std::env::var("STAGE").unwrap_or_default();
    drag(&mut d, PaintTool::Eraser, (150, 250), (240, 250), &o);
    if stage != "E" {
        let (x, y) = at(197, 270);
        d.paint_bucket(x, y, &o);
        if stage != "F" {
            drag(&mut d, PaintTool::Brush, (170, 180), (240, 215), &o);
        }
    }

    let got = d.doc.render(0);
    let (w, h) = (got.width().min(want.width()), got.height().min(want.height()));
    let (mut differ, mut worst, mut total) = (0u64, 0i32, 0u64);
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    let mut diff = RgbaImage::new(w, h, Rgba::WHITE);
    for y in 0..h {
        for x in 0..w {
            let (p, q) = (
                got.get(x as i64, y as i64).unwrap_or_default(),
                want.get(x as i64, y as i64).unwrap_or_default(),
            );
            let e = if p.a == 0 && q.a == 0 {
                0
            } else {
                [
                    p.r as i32 - q.r as i32,
                    p.g as i32 - q.g as i32,
                    p.b as i32 - q.b as i32,
                    p.a as i32 - q.a as i32,
                ]
                .iter()
                .map(|v| v.abs())
                .max()
                .unwrap_or(0)
            };
            if e > 0 {
                differ += 1;
                total += e as u64;
                worst = worst.max(e);
                let b = bounds.unwrap_or((x, y, x, y));
                bounds = Some((b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y)));
                diff.set(x as i64, y as i64, Rgba::new(255, 0, 0, 255));
            }
        }
    }
    println!(
        "{differ} of {} pixels differ; worst {worst} levels, sum {total}; bounds {bounds:?}",
        w as u64 * h as u64
    );
    if let Some(path) = rest.get(1) {
        std::fs::write(path, firecraft_io::encode_png(&got, 72.0)?)?;
    }
    if let Some(path) = rest.first() {
        std::fs::write(path, firecraft_io::encode_png(&diff, 72.0)?)?;
    }
    Ok(())
}
