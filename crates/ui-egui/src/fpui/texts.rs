//! The English texts of FirePixel's UI pieces FireCraft uses (FirePixel's `fp-i18n` en locale).

pub fn t(key: &str) -> String {
    match key {
        "btn-apply" => "Apply",
        "btn-cancel" => "Cancel",
        "pi-color-copy-hex" => "Copy HEX value",
        "pi-color-copy-rgb" => "Copy RGB value",
        "pi-color-dialog-title" => "Color",
        "pi-color-hex" => "HEX:",
        "pi-color-opacity" => "Opacity",
        "pi-color-rgb" => "RGB:",
        "pi-color-system" => "System color picker",
        "pi-fill-dither" => "Web Dither",
        "pi-fill-gradient" => "Gradient",
        "pi-fill-none" => "No Fill",
        "pi-fill-pattern" => "Pattern",
        "pi-fill-solid" => "Solid",
        "pi-stroke-inside" => "Align stroke inside",
        "pi-stroke-center" => "Center stroke on path",
        "pi-stroke-outside" => "Align stroke outside",
        other => other,
    }
    .to_owned()
}
