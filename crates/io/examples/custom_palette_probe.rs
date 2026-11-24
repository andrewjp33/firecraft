//! Write a Fireworks PNG whose export settings have a custom palette with an edited, a
//! transparent and a locked color, to check how Fireworks MX reads FireCraft's `PALv`:
//! custom_palette_probe <out.png>
//!
//! The image: left half red, right half blue. Table: red edited to green, blue transparent,
//! yellow locked; Fireworks MX exports the left half green and the right half transparent (checked
//! 2026-11-24, probes/custom_palette_mx2). With PROBE2 unset the edit goes the other way (green
//! edited to red): Fireworks MX then maps the red pixels to the nearest color matched, yellow
//! (probes/custom_palette_mx), as FireCraft does.
use firecraft_doc::{Bitmap, Document, Element, ElementKind, ExportFormat, PaletteEntry, PaletteKind, Transparency};
use firecraft_raster::{Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: custom_palette_probe <out.png>")?;
    let (red, blue) = (Rgba::new(255, 0, 0, 255), Rgba::new(0, 0, 255, 255));
    let mut img = RgbaImage::new(40, 20, red);
    for y in 0..20 {
        for x in 20..40 {
            img.set(x, y, blue);
        }
    }
    let mut d = Document::new(40, 20, 72.0, Some(Rgba::WHITE));
    if let Some(c) = d.layers[0].elements_mut(0) {
        c.push(Element::new(ElementKind::Bitmap(Bitmap { x: 0, y: 0, image: img })));
    }
    d.export.format = ExportFormat::Gif;
    d.export.palette = PaletteKind::Custom;
    d.export.transparency = Transparency::Index;
    let green = Rgba::new(0, 255, 0, 255);
    let first = if std::env::var_os("PROBE2").is_some() {
        // Red edited to green.
        PaletteEntry {
            original: Some(red),
            ..PaletteEntry::new(green)
        }
    } else {
        // Green edited to red.
        PaletteEntry {
            original: Some(green),
            ..PaletteEntry::new(red)
        }
    };
    d.export.table = vec![
        first,
        PaletteEntry {
            transparent: true,
            ..PaletteEntry::new(blue)
        },
        PaletteEntry {
            locked: true,
            ..PaletteEntry::new(Rgba::new(255, 255, 0, 255))
        },
    ];
    d.export.colors = 3;
    std::fs::write(&out, firecraft_io::save_fireworks_png(&d)?)?;
    let e = firecraft_io::export::export(&d, &d.export)?;
    let at = |x, y| e.preview.get(x, y);
    println!("FireCraft's export: left {:?}, right {:?}", at(5, 5), at(30, 5));
    Ok(())
}
