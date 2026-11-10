//! Reproduce: align gym3's main text left, undo, then change its stroke colour; print the
//! alignment at each step. `align_undo_probe gym3.png`
use firecraft_doc::ElementKind;
use firecraft_engine::{DocState, ElementRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let mut d = DocState::new(firecraft_io::open(&std::fs::read(&a[1])?, None)?);
    let i = d.doc.layers[0]
        .elements(0)
        .iter()
        .position(|e| matches!(&e.kind, ElementKind::Text(t) if t.runs.iter().map(|r| r.text.as_str()).collect::<String>().contains("Math")))
        .ok_or("no main text")?;
    let r = ElementRef::new(0, i);
    let show = |d: &DocState, step: &str| {
        let t = d.text_at(r).expect("text");
        let aligns: Vec<u8> = t.runs.iter().map(|r| r.align as u8).collect();
        let img = t.cached.as_ref().map(|b| {
            (
                b.x,
                b.y,
                b.image.width(),
                b.image.height(),
                b.image.pixels().as_ptr() as usize,
                b.image.stamp(),
            )
        });
        println!(
            "{step}: runs' align {:?} edited {} image {img:?} rect {:?}",
            &aligns[..aligns.len().min(6)],
            t.edited,
            t.rect
        );
    };
    d.selection = vec![r];
    show(&d, "opened");
    d.format_selected_text("Text Align", false, &|run| run.align = 0);
    show(&d, "aligned left");
    d.undo();
    show(&d, "undone");
    d.selection = vec![r];
    d.set_stroke_color(Some(firecraft_raster::Rgba::new(255, 0, 0, 255)));
    show(&d, "stroke colour");
    Ok(())
}
