//! A picture shown on the GPU as a grid of textures. A GPU texture has a maximum side (8192 on
//! many cards, 2048–4096 on older ones and in some browsers) and egui panics past it, but a
//! document can be 32000 pixels across (a 3200 px symbol's Symbol Editor is already 9600 wide).
//! Pictures that fit in one tile behave exactly as a single texture did.

use egui::{Color32, Painter, Rect, TextureHandle, TextureOptions, pos2};
use firecraft_raster::RgbaImage;

/// Tile side when the GPU allows more: big enough that ordinary documents are one texture,
/// small enough that partial updates of huge documents stay cheap.
const MAX_TILE: usize = 4096;

#[derive(Clone)]
pub(crate) struct TiledTexture {
    width: usize,
    height: usize,
    side: usize,
    /// Row by row; `cols` per row.
    tiles: Vec<TextureHandle>,
    cols: usize,
}

/// The tile side this GPU allows.
fn tile_side(ctx: &egui::Context) -> usize {
    ctx.input(|i| i.max_texture_side).clamp(256, MAX_TILE)
}

/// The pixels of `img` in the rectangle (`x`, `y`, `w`, `h`), which lies inside it.
fn region(img: &RgbaImage, x: usize, y: usize, w: usize, h: usize) -> egui::ColorImage {
    let iw = img.width() as usize;
    let px = img.pixels();
    let mut bytes = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        for p in &px[row * iw + x..row * iw + x + w] {
            bytes.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &bytes)
}

impl TiledTexture {
    pub(crate) fn new(ctx: &egui::Context, name: &str, img: &RgbaImage, options: TextureOptions) -> Self {
        let (width, height) = (img.width() as usize, img.height() as usize);
        let side = tile_side(ctx);
        let cols = width.div_ceil(side).max(1);
        let rows = height.div_ceil(side).max(1);
        let mut tiles = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                let (x, y) = (c * side, r * side);
                let (w, h) = (side.min(width - x.min(width)), side.min(height - y.min(height)));
                let image = if w == 0 || h == 0 {
                    egui::ColorImage::new([1, 1], vec![Color32::TRANSPARENT])
                } else {
                    region(img, x, y, w, h)
                };
                tiles.push(ctx.load_texture(format!("{name}-{c}-{r}"), image, options));
            }
        }
        Self {
            width,
            height,
            side,
            tiles,
            cols,
        }
    }

    /// Show `img` instead (any size).
    pub(crate) fn set(&mut self, ctx: &egui::Context, name: &str, img: &RgbaImage, options: TextureOptions) {
        if (img.width() as usize, img.height() as usize) != (self.width, self.height) || self.side != tile_side(ctx) {
            *self = Self::new(ctx, name, img, options);
            return;
        }
        self.set_rect(img, 0, 0, self.width, self.height, options);
    }

    /// Upload only the rectangle (`x`, `y`, `w`, `h`) of `img`, which has this texture's size.
    pub(crate) fn set_rect(&mut self, img: &RgbaImage, x: usize, y: usize, w: usize, h: usize, options: TextureOptions) {
        if (img.width() as usize, img.height() as usize) != (self.width, self.height) {
            return;
        }
        let (x1, y1) = ((x + w).min(self.width), (y + h).min(self.height));
        for (i, tex) in self.tiles.iter_mut().enumerate() {
            let (tx, ty) = ((i % self.cols) * self.side, (i / self.cols) * self.side);
            let (tx1, ty1) = ((tx + self.side).min(self.width), (ty + self.side).min(self.height));
            let (ax, ay, bx, by) = (x.max(tx), y.max(ty), x1.min(tx1), y1.min(ty1));
            if ax >= bx || ay >= by {
                continue;
            }
            tex.set_partial([ax - tx, ay - ty], region(img, ax, ay, bx - ax, by - ay), options);
        }
    }

    /// Draw the whole picture stretched over `rect`.
    pub(crate) fn paint(&self, p: &Painter, rect: Rect) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        let (sx, sy) = (rect.width() / self.width as f32, rect.height() / self.height as f32);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        for (i, tex) in self.tiles.iter().enumerate() {
            let (tx, ty) = ((i % self.cols) * self.side, (i / self.cols) * self.side);
            let (tx1, ty1) = ((tx + self.side).min(self.width), (ty + self.side).min(self.height));
            let r = Rect::from_min_max(
                pos2(rect.min.x + tx as f32 * sx, rect.min.y + ty as f32 * sy),
                pos2(rect.min.x + tx1 as f32 * sx, rect.min.y + ty1 as f32 * sy),
            );
            if p.clip_rect().intersects(r) {
                p.image(tex.id(), r, uv, Color32::WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_raster::Rgba;

    #[test]
    fn region_copies_the_right_pixels() {
        let mut img = RgbaImage::new(5, 4, Rgba::TRANSPARENT);
        img.pixels_mut()[2 * 5 + 3] = Rgba::new(10, 20, 30, 255);
        let r = region(&img, 3, 2, 2, 2);
        assert_eq!(r.size, [2, 2]);
        assert_eq!(r.pixels[0], Color32::from_rgb(10, 20, 30));
        assert_eq!(r.pixels[1], Color32::TRANSPARENT);
    }

    #[test]
    fn huge_pictures_are_split_within_the_gpu_limit() {
        let ctx = egui::Context::default();
        let img = RgbaImage::new(9600, 8800, Rgba::TRANSPARENT);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            let side = tile_side(ctx);
            let t = TiledTexture::new(ctx, "t", &img, TextureOptions::NEAREST);
            assert_eq!(t.tiles.len(), 9600usize.div_ceil(side) * 8800usize.div_ceil(side));
            assert!(t.tiles.iter().all(|t| t.size().iter().all(|&s| s <= side)));
        });
        // Nothing renders here: drop the uploads unapplied.
        out.textures_delta.clear();
    }
}
