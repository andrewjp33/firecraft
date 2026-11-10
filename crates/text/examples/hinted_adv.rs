//! Per-character advances of a string: linear, rounded, and hinted in several modes.
//! `hinted_adv FONT.ttf SIZE TEXT`
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, SmoothMode, Target};
use skrifa::raw::TableProvider;

struct Nop;
impl skrifa::outline::OutlinePen for Nop {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {}
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
    fn close(&mut self) {}
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let size: f32 = a[2].parse().unwrap();
    let font = skrifa::FontRef::new(&data).unwrap();
    let outlines = font.outline_glyphs();
    let cmap = font.charmap();
    let upm = font.head().map(|h| h.units_per_em()).unwrap_or(2048) as f32;
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let modes: Vec<(&str, Target, f32)> = vec![
        ("mono", Target::Mono, 1.0),
        (
            "light",
            Target::Smooth {
                mode: SmoothMode::Light,
                symmetric_rendering: true,
                preserve_linear_metrics: false,
            },
            1.0,
        ),
        (
            "normal",
            Target::Smooth {
                mode: SmoothMode::Normal,
                symmetric_rendering: true,
                preserve_linear_metrics: false,
            },
            1.0,
        ),
        ("mono4", Target::Mono, 4.0),
    ];
    print!("{:>4} {:>7} {:>4}", "ch", "linear", "rnd");
    for (n, _, _) in &modes {
        print!(" {n:>7}");
    }
    println!();
    for ch in a[3].chars() {
        let gid = cmap.map(ch).unwrap_or_default();
        let lin = metrics.advance_width(gid).unwrap_or(0.0) * size / upm;
        print!("{:>4} {lin:>7.2} {:>4}", format!("{ch:?}"), lin.round());
        for (_, t, k) in &modes {
            let inst = HintingInstance::new(
                &outlines,
                Size::new(size * k),
                LocationRef::default(),
                HintingOptions {
                    engine: Engine::Interpreter,
                    target: *t,
                },
            )
            .unwrap();
            let g = outlines.get(gid).unwrap();
            let m = g.draw(DrawSettings::hinted(&inst, false), &mut Nop).unwrap();
            print!(" {:>7.2}", m.advance_width.unwrap_or(0.0) / k);
        }
        println!();
    }
}
