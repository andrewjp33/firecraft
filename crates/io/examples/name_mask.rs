//! Name every top-level masked object's mask and save: name_mask <in.png> <out.png> (checks that
//! a mask's name is saved and read back; open the result in Fireworks MX to see it kept).
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, out] = a.as_slice() else {
        return Err("usage: name_mask <in.png> <out.png>".into());
    };
    let mut doc = firecraft_io::open(&std::fs::read(input)?, None)?;
    let mut n = 0;
    for l in &mut doc.layers {
        for cell in &mut l.cells {
            for e in cell.iter_mut() {
                if let ElementKind::Masked { mask, .. } = &mut e.kind {
                    n += 1;
                    mask.name = Some(format!("mask{n}"));
                }
            }
        }
    }
    std::fs::write(out, firecraft_io::save_fireworks_png(&doc)?)?;
    let back = firecraft_io::open(&std::fs::read(out)?, None)?;
    for l in &back.layers {
        for e in l.elements(0) {
            if let ElementKind::Masked { mask, .. } = &e.kind {
                println!("read back: {:?}", mask.name);
            }
        }
    }
    Ok(())
}
