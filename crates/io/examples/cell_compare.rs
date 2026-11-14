//! Compare FireCraft's render of a probe document with Fireworks' flat image, cell by cell:
//! cell_compare <fireworks-saved.png> <cell width> <cell height> [labels,comma,separated]
//!
//! Renders the document with FireCraft and compares it with the flat image Fireworks stored
//! (premultiplied RGBA). Per cell: exact %, within-1 %, max difference and the worst pixel.
//! `CELL_DUMP=dir` writes each differing cell side by side (Fireworks | FireCraft).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let path = a.first().ok_or("usage: cell_compare <file.png> <cw> <ch> [labels]")?;
    let cw: u32 = a.get(1).ok_or("cw")?.parse()?;
    let chh: u32 = a.get(2).ok_or("ch")?.parse()?;
    let labels: Vec<String> = a.get(3).map(|s| s.split(',').map(str::to_owned).collect()).unwrap_or_default();
    let bytes = std::fs::read(path)?;
    let fw = firecraft_io::decode_png(&bytes)?;
    let doc = firecraft_io::open(&bytes, Some(path.clone()))?;
    let fc = doc.render(0);
    let (w, h) = (fw.width(), fw.height());
    let pm = |p: firecraft_raster::Rgba| {
        let a = p.a as u32;
        [p.r as u32 * a / 255, p.g as u32 * a / 255, p.b as u32 * a / 255, a].map(|v| v as i32)
    };
    for cy in 0..h / chh {
        for cx in 0..w / cw {
            let (mut n, mut exact, mut near, mut maxd) = (0u32, 0u32, 0u32, 0i32);
            let mut worst = (0, 0, [0; 4], [0; 4]);
            for y in cy * chh..(cy + 1) * chh {
                for x in cx * cw..(cx + 1) * cw {
                    let (p, q) = (fw.get(x as i64, y as i64).unwrap_or_default(), fc.get(x as i64, y as i64).unwrap_or_default());
                    let (p, q) = (pm(p), pm(q));
                    let d = (0..4).map(|i| (p[i] - q[i]).abs()).max().unwrap_or(0);
                    n += 1;
                    exact += (d == 0) as u32;
                    near += (d <= 1) as u32;
                    if d > maxd {
                        maxd = d;
                        worst = (x - cx * cw, y - cy * chh, p, q);
                    }
                }
            }
            let label = labels.get(cx as usize).map_or(String::new(), |s| s.clone());
            println!(
                "r{cy} c{cx:>2} {label:<14} exact {:>6.2}%  <=1 {:>6.2}%  max {maxd:>3}  worst ({},{}) fw {:?} fc {:?}",
                exact as f64 * 100.0 / n as f64,
                near as f64 * 100.0 / n as f64,
                worst.0,
                worst.1,
                worst.2,
                worst.3
            );
        }
    }
    if let Ok(dir) = std::env::var("CELL_DUMP") {
        let mut both = firecraft_raster::RgbaImage::new(w, h * 2, firecraft_raster::Rgba::TRANSPARENT);
        for y in 0..h {
            for x in 0..w {
                both.set(x as i64, y as i64, fw.get(x as i64, y as i64).unwrap_or_default());
                both.set(x as i64, (y + h) as i64, fc.get(x as i64, y as i64).unwrap_or_default());
            }
        }
        std::fs::write(format!("{dir}/fw_vs_fc.png"), firecraft_io::encode_png(&both, 72.0)?)?;
    }
    Ok(())
}
