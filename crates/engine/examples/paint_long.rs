//! Does a live stroke slow down as it gets longer? Scribbles for many frames and reports the
//! time per frame at intervals.
//!
//!   cargo run -p firecraft-engine --example paint_long [SIZE] [FRAMES]

use std::time::Instant;

use firecraft_doc::{Document, RenderCache, brush_presets};
use firecraft_engine::{DocState, PaintTool, ToolOptions};
use firecraft_raster::Rgba;

fn main() {
    let a: Vec<u32> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let size = a.first().copied().unwrap_or(100);
    let frames = a.get(1).copied().unwrap_or(4000) as usize;
    for (tool, category, name) in [
        (PaintTool::Brush, "bc_Basic", "bn_Hard Rounded"),
        (PaintTool::Brush, "bc_Unnatural", "bn_3D Glow"),
        (PaintTool::Eraser, "bc_Basic", "bn_Hard Rounded"),
    ] {
        let mut o = ToolOptions {
            stroke: Some(Rgba::BLACK),
            stroke_preset: brush_presets::find(category, name).unwrap_or(0),
            stroke_size: size,
            ..ToolOptions::default()
        };
        o.paint.eraser_size = size;
        let (cw, ch) = (660u32, 440u32);
        let mut d = DocState::new(Document::new(cw, ch, 72.0, Some(Rgba::WHITE)));
        if tool == PaintTool::Eraser {
            d.begin_paint(
                PaintTool::Brush,
                300.0,
                200.0,
                &ToolOptions {
                    stroke_size: 300,
                    ..o.clone()
                },
            );
            d.end_paint();
        }
        let mut cache = RenderCache::default();
        d.begin_paint(tool, 330.0, 220.0, &o);
        print!("{tool:?} {name:<18}");
        let (mut sum, mut n) = (0.0f64, 0);
        for f in 0..frames {
            let t0 = Instant::now();
            for e in 0..3 {
                let t = (f * 3 + e) as f64 * 0.013;
                let x = 330.0 + t.sin() * 200.0 * (t * 0.37).cos();
                let y = 220.0 + (t * 1.7).cos() * 130.0;
                d.extend_paint(x, y, false);
            }
            d.flush_paint();
            let _ = d.doc.render_cached(0, &mut cache);
            sum += t0.elapsed().as_secs_f64();
            n += 1;
            if (f + 1) % (frames / 4).max(1) == 0 {
                print!("  frame {:>5}: {:6.1} ms", f + 1, sum * 1000.0 / n as f64);
                (sum, n) = (0.0, 0);
            }
        }
        println!();
        d.end_paint();
    }
}
