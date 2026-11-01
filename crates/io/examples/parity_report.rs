//! Compare FireCraft's rendering of Fireworks PNGs with the flattened image Fireworks itself
//! stored in each file (its `IDAT`), and check lossless/save round trips.
//!
//! Usage: parity_report <out-dir> <file.png>...
//! Writes <out-dir>/<name>.cmp.png (Fireworks | FireCraft | difference) for each file.
//!
//! Frame references: a plain exported PNG named `<doc>_frame<N>.png` next to `<doc>.png` is
//! compared with FireCraft's render of frame N of `<doc>.png` (export single frames from
//! Fireworks as PNG32 to test animations).
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().ok_or("out dir")?);
    println!(
        "{:<20} {:>9} {:>6} {:>7} {:>7} {:>6} {:>6}  notes",
        "file", "size", "raw-rt", "match%", "meanΔ", "open", "save"
    );
    for path in args {
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = std::fs::read(&path)?;
        let raw_rt = firecraft_fwpng::FwPng::read(&bytes)
            .map(|p| p.to_bytes() == bytes)
            .unwrap_or(false);
        let t = std::time::Instant::now();
        let doc = match firecraft_io::open(&bytes, Some(path.clone())) {
            Ok(d) => d,
            Err(e) => {
                println!("{name:<20} OPEN FAILED: {e}");
                continue;
            }
        };
        let open_ms = t.elapsed().as_millis();
        let fw = firecraft_io::decode_png(&bytes)?;
        // A frame export: render that frame of the source document instead.
        let frame_ref = (doc.raw.is_none()).then(|| frame_source(&path)).flatten();
        let (doc, frame) = match frame_ref.clone() {
            Some((src, f)) => match std::fs::read(&src).ok().and_then(|b| firecraft_io::open(&b, Some(src.clone())).ok()) {
                Some(d) => (d, f),
                None => (doc, 0),
            },
            None => (doc, 0),
        };
        let ours = doc.render(frame);
        let (w, h) = (fw.width().min(ours.width()), fw.height().min(ours.height()));
        let (mut ok, mut sum) = (0u64, 0u64);
        let mut diff = RgbaImage::new(w, h, Rgba::BLACK);
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let (a, b) = (
                    fw.get(x, y).unwrap_or_default(),
                    ours.get(x, y).unwrap_or_default(),
                );
                // Compare as composited over white, so fully transparent pixels compare equal.
                let wa = Rgba::WHITE.over(a, 255);
                let wb = Rgba::WHITE.over(b, 255);
                let d = [wa.r.abs_diff(wb.r), wa.g.abs_diff(wb.g), wa.b.abs_diff(wb.b)]
                    .into_iter()
                    .max()
                    .unwrap_or(0);
                sum += d as u64;
                if d <= 8 {
                    ok += 1;
                }
                let v = (d as u32 * 4).min(255) as u8;
                diff.set(x, y, Rgba::new(v, v, v, 255));
            }
        }
        let n = (w as u64 * h as u64).max(1);
        // Save round trip: FireCraft's own save must re-open to the same render.
        let save_ok = firecraft_io::save_fireworks_png(&doc)
            .ok()
            .and_then(|b| firecraft_io::open(&b, None).ok())
            .map(|d2| d2.render(frame) == ours)
            .unwrap_or(false);
        let mut notes = String::new();
        // A plain PNG (an export, no Fireworks layer data) opens as one bitmap of its own
        // image: comparing it with itself proves nothing.
        if frame_ref.is_some() {
            notes += &format!("frame {} of its document; ", frame + 1);
        } else if doc.raw.is_none() {
            notes += "PLAIN PNG (no Fireworks data; not a parity test); ";
        }
        if (fw.width(), fw.height()) != (ours.width(), ours.height()) {
            notes += &format!(
                "size fw {}x{} vs ours {}x{}; ",
                fw.width(),
                fw.height(),
                ours.width(),
                ours.height()
            );
        }
        let kinds = doc
            .layers
            .iter()
            .flat_map(|l| l.elements(0))
            .map(|e| e.label())
            .collect::<Vec<_>>();
        notes += &format!(
            "{} layers, {} top-level objects, {} frames",
            doc.layers.len(),
            kinds.len(),
            doc.frames.len()
        );
        println!(
            "{name:<20} {:>9} {:>6} {:>6.1}% {:>7.2} {:>4}ms {:>6}  {notes}",
            format!("{}x{}", doc.width, doc.height),
            if raw_rt { "ok" } else { "DIFF" },
            ok as f64 * 100.0 / n as f64,
            sum as f64 / n as f64,
            open_ms,
            if save_ok { "ok" } else { "DIFF" },
        );
        // Side-by-side image, downscaled for big documents.
        let scale = ((1200.0 / w.max(1) as f64).min(600.0 / h.max(1) as f64)).min(1.0);
        let (sw, sh) = (
            ((w as f64 * scale) as u32).max(1),
            ((h as f64 * scale) as u32).max(1),
        );
        let mut sheet = RgbaImage::new(sw * 3 + 20, sh, Rgba::new(128, 128, 128, 255));
        for (i, img) in [&fw, &ours, &diff].into_iter().enumerate() {
            for y in 0..sh {
                for x in 0..sw {
                    let p = img
                        .get((x as f64 / scale) as i64, (y as f64 / scale) as i64)
                        .unwrap_or_default();
                    sheet.set(
                        (i as u32 * (sw + 10) + x) as i64,
                        y as i64,
                        Rgba::WHITE.over(p, 255),
                    );
                }
            }
        }
        std::fs::write(
            out.join(format!("{name}.cmp.png")),
            firecraft_io::encode_png(&sheet, 72.0)?,
        )?;
    }
    Ok(())
}

/// `dir/<doc>_frame<N>.png` -> (`dir/<doc>.png`, N - 1) when that document exists.
fn frame_source(path: &str) -> Option<(String, usize)> {
    let p = std::path::Path::new(path);
    let stem = p.file_stem()?.to_str()?;
    let (doc, n) = stem.rsplit_once("_frame")?;
    let n: usize = n.parse().ok().filter(|&n| n >= 1)?;
    let src = p.with_file_name(format!("{doc}.png"));
    src.exists().then(|| (src.to_string_lossy().into_owned(), n - 1))
}

