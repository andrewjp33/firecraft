//! Kerning of each adjacent pair in TEXT as the layout sees it (fonts from FP_FONT_DIRS and the
//! system): kern_pairs FAMILY TEXT
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = firecraft_text::FontDb::shared()
        .resolve(&a[1], false, false)
        .expect("font")
        .font;
    let chars: Vec<char> = a[2].chars().collect();
    for w in chars.windows(2) {
        let (g1, g2) = (f.glyph_index(w[0]), f.glyph_index(w[1]));
        let (k, legacy) = (f.kerning(g1, g2), f.legacy_kerning(g1, g2));
        if k != 0 || legacy != 0 {
            println!("{:?}{:?}: layout {k}, kern table {legacy}", w[0], w[1]);
        }
    }
}
