//! What a paste of clipboard data would bring in: clip_decode <data> [out.png]
//! (data as `sysclip_probe get` saves it: a Fireworks PNG, PNG, DIB, BMP or JPEG).
//! For objects: where the picture was copied from, how far its pixels reach, each object's bounds.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(a.first().ok_or("usage: clip_decode <data> [out.png]")?)?;
    match firecraft_io::read_clipboard(&bytes)? {
        firecraft_io::Pasted::Objects(e, s, t) => {
            println!("{} objects, {} symbols, {} tiles", e.len(), s.len(), t.len());
            println!("picture (x, y, w, h): {:?}", firecraft_io::clipboard_picture(&bytes));
            let flat = firecraft_io::decode_png(&bytes)?;
            let (w, h) = (i64::from(flat.width()), i64::from(flat.height()));
            let lit: Vec<(i64, i64)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| flat.get(x, y).is_some_and(|c| c.a > 0))
                .collect();
            let span = |f: fn(&(i64, i64)) -> i64| (lit.iter().map(f).min(), lit.iter().map(f).max());
            println!("pixels with alpha: x {:?}, y {:?} (in the picture)", span(|p| p.0), span(|p| p.1));
            for el in &e {
                println!("  object bounds {:?}", el.bounds());
            }
        }
        firecraft_io::Pasted::Image(img) => {
            let p = |x, y| img.get(x, y).map(|c| (c.r, c.g, c.b, c.a));
            println!("picture {}x{}; (0,0)={:?} (99,59)={:?}", img.width(), img.height(), p(0, 0), p(99, 59));
            if let Some(out) = a.get(1) {
                std::fs::write(out, firecraft_io::encode_png(&img, 72.0)?)?;
            }
        }
    }
    Ok(())
}
