//! Render one top-level object alone with only its first `k` Live Effects (what effect k+1
//! receives): effect_input <in.png> <layer> <element> <k> <out.png>
use firecraft_doc::ElementKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, li, ei, k, out] = a.as_slice() else {
        return Err("usage: effect_input <in.png> <layer> <element> <k> <out.png>".into());
    };
    let (li, ei, k): (usize, usize, usize) = (li.parse()?, ei.parse()?, k.parse()?);
    let mut doc = firecraft_io::open(&std::fs::read(input)?, Some(input.clone()))?;
    for (l, layer) in doc.layers.iter_mut().enumerate() {
        if let Some(els) = layer.elements_mut(0) {
            for (e, el) in els.iter_mut().enumerate() {
                if l == li && e == ei {
                    el.effects.truncate(k);
                    // Text keeps a cached image with its effects baked in; drop it.
                    if let ElementKind::Text(t) = &mut el.kind {
                        t.cached = None;
                    }
                } else {
                    el.visible = false;
                }
            }
        }
    }
    std::fs::write(out, firecraft_io::encode_png(&doc.render(0), doc.resolution)?)?;
    Ok(())
}
