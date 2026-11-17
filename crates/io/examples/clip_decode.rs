//! What a paste of clipboard data would bring in: clip_decode <data> [out.png]
//! (data as `sysclip_probe get` saves it: a Fireworks PNG, PNG, DIB, BMP or JPEG).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(a.first().ok_or("usage: clip_decode <data> [out.png]")?)?;
    match firecraft_io::read_clipboard(&bytes)? {
        firecraft_io::Pasted::Objects(e, s, t) => {
            println!("{} objects, {} symbols, {} tiles", e.len(), s.len(), t.len());
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
