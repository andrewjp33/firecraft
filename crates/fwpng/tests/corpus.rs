//! Round-trip every Fireworks PNG found under `FIRECRAFT_FWMX_DIR` (an installed Fireworks MX
//! folder, or any folder of Fireworks PNGs). The corpus is not redistributed with FireCraft, so
//! the test is skipped when the variable is unset.

use std::path::{Path, PathBuf};

use firecraft_fwpng::FwPng;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if let Ok(b) = std::fs::read(&p) {
            if b.starts_with(b"\x89PNG") {
                out.push(p);
            }
        }
    }
}

#[test]
fn corpus_round_trip() {
    let Ok(dir) = std::env::var("FIRECRAFT_FWMX_DIR") else {
        eprintln!("FIRECRAFT_FWMX_DIR unset; skipping");
        return;
    };
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    let mut docs = 0;
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let png = FwPng::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert_eq!(png.to_bytes(), bytes, "{}: chunk round trip", f.display());
        if let Some(tree) = png.tree().unwrap() {
            docs += 1;
            let tiles = png.tiles().unwrap();
            // Every tile id referenced by the tree must exist.
            fn ids(n: &firecraft_fwpng::Node, out: &mut Vec<u32>) {
                if n.is("TID") {
                    out.extend(n.as_u32());
                }
                n.children.iter().for_each(|c| ids(c, out));
            }
            let mut refs = Vec::new();
            ids(&tree, &mut refs);
            for id in refs {
                assert!(tiles.contains_key(&id), "{}: missing tile {id:x}", f.display());
            }
            // Byte-exact tree round trip.
            let raw = {
                use std::io::Read;
                let c = png.chunks.iter().find(|c| c.is(b"mkTS")).unwrap();
                let mut v = Vec::new();
                flate2::read::ZlibDecoder::new(&c.data[..]).read_to_end(&mut v).unwrap();
                v
            };
            assert_eq!(tree.to_bytes(), raw, "{}: tree round trip", f.display());
        }
    }
    eprintln!("checked {docs} Fireworks documents");
    assert!(docs > 0, "no Fireworks documents found in {dir}");
}
