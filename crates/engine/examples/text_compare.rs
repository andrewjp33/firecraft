//! Compare FireCraft's text rendering with the image Fireworks stored for each text object.
//! `text_compare FILE [OUT_DIR]`: per object, the offset that matches best and how close it is.

use firecraft_doc::{Element, ElementKind};
use firecraft_raster::RgbaImage;

fn alpha(img: &RgbaImage, x: i64, y: i64) -> i32 {
    img.get(x, y).map_or(0, |p| i32::from(p.a))
}

fn compare(a: &RgbaImage, ax: i64, ay: i64, b: &RgbaImage, bx: i64, by: i64) -> (u64, u64) {
    let (x0, y0) = (ax.min(bx), ay.min(by));
    let x1 = (ax + i64::from(a.width())).max(bx + i64::from(b.width()));
    let y1 = (ay + i64::from(a.height())).max(by + i64::from(b.height()));
    let (mut sum, mut off) = (0u64, 0u64);
    for y in y0..y1 {
        for x in x0..x1 {
            let d = (alpha(a, x - ax, y - ay) - alpha(b, x - bx, y - by)).unsigned_abs();
            sum += u64::from(d);
            off += u64::from(d > 16);
        }
    }
    (sum, off)
}

fn walk(e: &Element, n: &mut usize, out: Option<&str>) {
    if let ElementKind::Group(c) = &e.kind {
        c.iter().for_each(|x| walk(x, n, out));
    }
    let ElementKind::Text(t) = &e.kind else { return };
    let (Some(c), Some(m)) = (&t.cached, firecraft_engine::text::rasterize(t)) else {
        return;
    };
    let mut best = (u64::MAX, 0, 0, 0);
    for dy in -3..=3 {
        for dx in -3..=3 {
            let (s, o) = compare(&c.image, c.x, c.y, &m.image, m.x + dx, m.y + dy);
            if s < best.0 {
                best = (s, dx, dy, o);
            }
        }
    }
    let total: u64 = c.image.pixels().iter().map(|p| u64::from(p.a)).sum();
    println!(
        "{:32} stored {}x{} at {},{}  mine {}x{} at {},{}  best shift {},{}  off>16: {}  error {:.1}%",
        t.text.chars().take(30).collect::<String>(),
        c.image.width(),
        c.image.height(),
        c.x,
        c.y,
        m.image.width(),
        m.image.height(),
        m.x,
        m.y,
        best.1,
        best.2,
        best.3,
        100.0 * best.0 as f64 / total.max(1) as f64
    );
    if let Some(dir) = out {
        let path = |k: &str| format!("{dir}/t{n}_{k}.png");
        let mine = firecraft_io::encode_png(&m.image, 72.0);
        let stored = firecraft_io::encode_png(&c.image, 72.0);
        if let (Ok(a), Ok(b)) = (mine, stored) {
            let _ = std::fs::write(path("fc"), a);
            let _ = std::fs::write(path("mx"), b);
        }
    }
    *n += 1;
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(&a[1]).expect("reads"), None).expect("opens");
    let mut n = 0;
    for l in &doc.layers {
        l.elements(0)
            .iter()
            .for_each(|e| walk(e, &mut n, a.get(2).map(String::as_str)));
    }
    for s in doc.symbols.iter() {
        for l in &s.layers {
            l.elements(0)
                .iter()
                .for_each(|e| walk(e, &mut n, a.get(2).map(String::as_str)));
        }
    }
}
