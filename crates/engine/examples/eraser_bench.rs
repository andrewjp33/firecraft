//! How long a live Eraser stroke takes, frame by frame, across a canvas-sized bitmap: the
//! engine's update (the erasing) and the canvas's re-render of what changed.
//!
//!   cargo run -p firecraft-engine --example eraser_bench [SIZE] [EDGE] [round|square] [CANVAS_W] [CANVAS_H]

use std::time::Instant;

use firecraft_doc::{Bitmap, Document, Element, ElementKind, RenderCache};
use firecraft_engine::{DocState, PaintTool, ToolOptions};
use firecraft_raster::{Rgba, RgbaImage};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize, d: u32| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (size, edge) = (num(0, 400), num(1, 0));
    let round = a.get(2).is_none_or(|s| s != "square");
    let (cw, ch) = (num(3, 1920), num(4, 1080));
    let mut o = ToolOptions::default();
    o.paint.eraser_size = size;
    o.paint.eraser_edge = edge as u8;
    o.paint.eraser_round = round;
    let mut d = DocState::new(Document::new(cw, ch, 72.0, Some(Rgba::WHITE)));
    let image = RgbaImage::new(cw, ch, Rgba::BLACK);
    d.add_element(Element::new(ElementKind::Bitmap(Bitmap { x: 0, y: 0, image })), "Fill");
    let mut cache = RenderCache::default();
    let _ = d.doc.render_cached(0, &mut cache);
    let mut drawn = d.revision;
    d.begin_paint(PaintTool::Eraser, 200.0, 300.0, &o);
    let (mut engine, mut render, mut worst) = (0.0f64, 0.0f64, 0.0f64);
    // A wandering stroke: 60 frames, three pointer events per frame (about 20 px apart).
    let frames = 60;
    for f in 0..frames {
        let t0 = Instant::now();
        for e in 0..3 {
            let t = (f * 3 + e) as f64 / (frames * 3) as f64;
            d.extend_paint(200.0 + t * (cw as f64 - 400.0), ch as f64 / 2.0 + (t * 9.0).sin() * 200.0, false);
        }
        d.flush_paint();
        let t1 = Instant::now();
        if let Some((from, to, area)) = d.paint_damage.take()
            && from == drawn
            && to == d.revision
        {
            cache.changed_within = Some(area);
        }
        let _ = d.doc.render_cached(0, &mut cache);
        drawn = d.revision;
        let t2 = Instant::now();
        let (a, b) = ((t1 - t0).as_secs_f64(), (t2 - t1).as_secs_f64());
        engine += a;
        render += b;
        worst = worst.max(a + b);
    }
    d.end_paint();
    let n = frames as f64 * 0.001;
    println!(
        "eraser {size} px, edge {edge}, {}: engine {:7.1} ms  render {:6.1} ms  per frame; worst {:7.1} ms; {:5.1} fps",
        if round { "round" } else { "square" },
        engine / n,
        render / n,
        worst * 1000.0,
        frames as f64 / (engine + render)
    );
}
