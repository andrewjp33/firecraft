//! Save a copy with top-level object `index` of the first layer hidden: hide_object <in> <out> <index>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, out, index] = a.as_slice() else { return Err("usage: hide_object <in> <out> <index>".into()) };
    let mut doc = firecraft_io::open(&std::fs::read(input)?, None)?;
    let i: usize = index.parse()?;
    if let Some(e) = doc.layers[0].cells[0].get_mut(i) {
        e.visible = false;
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&doc)?)?;
    Ok(())
}
