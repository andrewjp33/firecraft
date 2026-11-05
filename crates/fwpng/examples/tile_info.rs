//! Print a Fireworks PNG's tiles (id, flags, non-zero reserved bytes, payload size), optionally
//! only those whose id (hex) is listed: tile_info <file.png> [id...]
use firecraft_fwpng::FwPng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let png = FwPng::read(&std::fs::read(a.get(1).ok_or("file")?)?)?;
    let want: Vec<u32> = a
        .iter()
        .skip(2)
        .filter_map(|s| u32::from_str_radix(s, 16).ok())
        .collect();
    for (_, t) in png.tiles()? {
        if want.is_empty() || want.contains(&t.id) {
            let res: Vec<(usize, u8)> = t
                .reserved
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, b)| *b != 0)
                .collect();
            println!(
                "{:x} flags={} reserved={:?} len={}",
                t.id,
                t.flags,
                res,
                t.data.len()
            );
        }
    }
    Ok(())
}
