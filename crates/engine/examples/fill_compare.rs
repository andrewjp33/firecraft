//! Compare FireCraft's rendering of a Fireworks PNG with the flat image Fireworks saved in it.
//! `fill_compare FILE...`: for each, pixels differing at all and by more than 2 levels.

use firecraft_raster::RgbaImage;

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("reads");
        let flat: RgbaImage = firecraft_io::decode_png(&bytes).expect("a PNG");
        let doc = firecraft_io::open(&bytes, None).expect("opens");
        let mine = doc.render(0);
        let (mut any, mut big, mut first) = (0, 0, None);
        for y in 0..flat.height().min(mine.height()) as i64 {
            for x in 0..flat.width().min(mine.width()) as i64 {
                let (Some(a), Some(b)) = (flat.get(x, y), mine.get(x, y)) else {
                    continue;
                };
                if a.a == 0 && b.a == 0 {
                    continue;
                }
                let d = [
                    a.r.abs_diff(b.r),
                    a.g.abs_diff(b.g),
                    a.b.abs_diff(b.b),
                    a.a.abs_diff(b.a),
                ];
                let m = d.iter().copied().max().unwrap_or(0);
                if m > 0 {
                    any += 1;
                    first.get_or_insert((x, y, a, b));
                }
                if m > 2 {
                    big += 1;
                }
            }
        }
        println!("{path}: {any} differ, {big} by >2, first {first:?}");
    }
}
