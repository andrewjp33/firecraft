//! How long a live Brush stroke takes, frame by frame: the engine's update and the canvas's
//! re-render (the two costs per screen update).
//!
//!   cargo run -p firecraft-engine --example paint_bench [SIZE] [CANVAS_W] [CANVAS_H]

use std::time::Instant;

use firecraft_doc::{Document, RenderCache, brush_presets};
use firecraft_engine::{DocState, PaintTool, ToolOptions};
use firecraft_raster::Rgba;

fn main() {
    let a: Vec<u32> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let size = a.first().copied().unwrap_or(100);
    let (cw, ch) = (a.get(1).copied().unwrap_or(660), a.get(2).copied().unwrap_or(440));
    for (category, name) in [
        ("bc_Pencil", "bn_1-Pixel Anti-Aliased"),
        ("bc_Pencil", "bn_1-Pixel"),
        ("bc_Basic", "bn_Hard Rounded"),
        ("bc_Basic", "bn_Soft Rounded"),
        ("bc_Air Brush", "bn_Basic"),
        ("bc_Charcoal", "bn_Soft"),
        ("bc_Oil", "bn_Strands"),
        ("bc_Unnatural", "bn_3D Glow"),
        ("bc_Unnatural", "bn_3D"),
        ("bc_Random", "bn_Confetti"),
    ] {
        let Some(preset) = brush_presets::find(category, name) else {
            println!("{category}/{name}: no such preset");
            continue;
        };
        let o = ToolOptions {
            stroke: Some(Rgba::BLACK),
            stroke_preset: preset,
            stroke_size: size,
            ..ToolOptions::default()
        };
        let mut d = DocState::new(Document::new(cw, ch, 72.0, Some(Rgba::WHITE)));
        let mut cache = RenderCache::default();
        d.begin_paint(PaintTool::Brush, 60.0, 60.0, &o);
        let (mut engine, mut render, mut worst) = (0.0f64, 0.0f64, 0.0f64);
        // A wandering stroke: 120 frames, three pointer events per frame.
        let frames = 120;
        for f in 0..frames {
            let t0 = Instant::now();
            for e in 0..3 {
                let t = (f * 3 + e) as f64 / (frames * 3) as f64;
                let x = 60.0 + t * (cw as f64 - 120.0);
                let y = ch as f64 / 2.0 + (t * 25.0).sin() * (ch as f64 / 3.0);
                d.extend_paint(x, y, false);
            }
            d.flush_paint();
            let t1 = Instant::now();
            let _ = d.doc.render_cached(0, &mut cache);
            let t2 = Instant::now();
            let (a, b) = ((t1 - t0).as_secs_f64(), (t2 - t1).as_secs_f64());
            engine += a;
            render += b;
            worst = worst.max(a + b);
        }
        d.end_paint();
        let n = frames as f64 * 0.001;
        println!(
            "{:<34} engine {:7.1} ms  render {:6.1} ms  per frame; worst {:7.1} ms; {:5.1} fps",
            format!("{category}/{name}"),
            engine / n,
            render / n,
            worst * 1000.0,
            frames as f64 / (engine + render)
        );
    }
}
