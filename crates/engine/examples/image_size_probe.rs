//! Canvas Size then Image Size on a Fireworks PNG, to compare with Fireworks MX doing the same:
//! image_size_probe IN.png OUT.png CANVAS_W CANVAS_H IMAGE_W IMAGE_H RES [bicubic|bilinear|soft|nearest]
//! (Canvas Size anchored at the centre; RES in pixels per inch.)
use firecraft_engine::{Anchor, DocState};
use firecraft_raster::transform::Resample;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let n = |i: usize| -> Result<u32, Box<dyn std::error::Error>> { Ok(a.get(i).ok_or("usage")?.parse()?) };
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(a.get(1).ok_or("usage")?)?, None)?);
    d.canvas_size(n(3)?, n(4)?, Anchor::Center);
    let method = Resample::from_key(a.get(8).map_or("bicubic", String::as_str));
    d.image_size(n(5)?, n(6)?, f64::from(n(7)?), false, method);
    std::fs::write(a.get(2).ok_or("usage")?, firecraft_io::save_fireworks_png(&d.doc)?)?;
    Ok(())
}
