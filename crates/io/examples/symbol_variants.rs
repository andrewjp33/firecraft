//! Variants of a document for testing in Fireworks: rearranges the elements of symbol SYMBOL_ID's
//! first layer. `symbol_variants IN.png SYMBOL_ID OUT_DIR` writes reversed.png (order reversed),
//! first.png (only the first element kept), last.png (only the last) and same.png (unchanged, saved
//! by FireCraft).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let doc = firecraft_io::open(&std::fs::read(&a[1])?, None)?;
    let id: u32 = a[2].parse()?;
    let out = std::path::Path::new(&a[3]);
    let edits: [(&str, fn(&mut Vec<firecraft_doc::Element>)); 4] = [
        ("same", |_| {}),
        ("reversed", |v| v.reverse()),
        ("first", |v| v.truncate(1)),
        ("last", |v| {
            let l = v.pop();
            v.clear();
            v.extend(l);
        }),
    ];
    for (name, edit) in edits {
        let mut d = doc.clone();
        let s = std::sync::Arc::make_mut(&mut d.symbols)
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("no such symbol")?;
        let els = s
            .layers
            .first_mut()
            .and_then(|l| l.elements_mut(0))
            .ok_or("no elements")?;
        println!("{name}: {} elements before", els.len());
        edit(els);
        std::fs::write(
            out.join(format!("{name}.png")),
            firecraft_io::save_fireworks_png(&d)?,
        )?;
    }
    Ok(())
}
