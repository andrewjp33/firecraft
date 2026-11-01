//! Print FireCraft's blend results for the oracle grid (bases x tops) as CSV-ish lines.
use firecraft_raster::{BlendMode, Rgba};
fn main() {
    let h = |s: &str| -> Rgba {
        let v = u32::from_str_radix(&s[1..], 16).unwrap_or(0);
        Rgba::new((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
    };
    let bases = ["#c82828", "#28b43c", "#1e3cc8", "#dcc828", "#808080", "#fafafa"].map(h);
    let tops = ["#ff0000", "#0000ff", "#64c896", "#f07814", "#3c3c3c", "#b45ac8"].map(h);
    for (m, mode) in BlendMode::ALL.iter().enumerate() {
        for (i, b) in bases.iter().enumerate() {
            for (j, t) in tops.iter().enumerate() {
                let r = mode.composite(*b, *t, 255);
                println!("{m} {i} {j} {} {} {}", r.r, r.g, r.b);
            }
        }
    }
}
