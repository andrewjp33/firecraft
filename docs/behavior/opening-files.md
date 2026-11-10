# Opening files (Fireworks MX behaviour, measured)

File > Open (and drag and drop, and files named on the command line) reads a Fireworks PNG as an
editable document and any other image as a flat document. Code: `crates/io/src/formats.rs`, tests
in the same file.

## Formats

| Format | Signature | Fireworks MX | FireCraft |
| --- | --- | --- | --- |
| Fireworks PNG | PNG with Fireworks chunks | editable | editable ([fireworks-png.md](../fireworks-png.md)) |
| PNG | `\x89PNG` | flat | flat; **animated PNG** as frames |
| JPEG | `FF D8 FF` | flat | flat, turned upright by its EXIF orientation |
| GIF | `GIF87a` / `GIF89a` | flat; animated GIFs as frames | the same |
| BMP | `BM` | flat (rejects BMP v4/v5 headers) | flat, all header versions |
| TIFF | `II*\0` / `MM\0*` | flat (misreads 16-bit TIFFs) | flat, 16-bit read correctly |
| Targa | none (`.tga` name, or v2 footer) | flat | flat |
| WebP | `RIFF....WEBP` | — | flat; animated WebP as frames |
| JPEG XL | `FF 0A` or the `JXL ` box | — | flat, colour-managed to sRGB; animations as frames |
| Photoshop | `8BPS` | opens (not measured yet) | not yet: a message says it's on the roadmap |
| Illustrator, FreeHand | `.ai` (PDF/PostScript), `.fh*` | opens (not measured yet) | not yet: same message |

A JPEG, GIF or other non-PNG document is never saved over its file: Save asks for a name, with the
extension changed to `.png`, as Fireworks MX does. Saving a document opened from a flat PNG writes
the Fireworks PNG over it (Fireworks MX's behaviour here isn't measured yet).

## What Fireworks MX makes of a flat image

Measured with a copy of `tools/fwmx-oracle/fw_inspect.sh` that also logs each frame's elements, the
frame delays and the resolution, on 60×40 probes written by ImageMagick (2026-11-10):

- **PNG, JPEG**: layer "Background" with the image as a bitmap at 0,0, an empty "Layer 1" above it,
  then the Web Layer. "Layer 1" is current.
- **TIFF, Targa, BMP**: only "Background" (and the Web Layer).
- **Canvas**: transparent (`BGCi = 00ffffff`).
- **Resolution**: JPEG's JFIF density (300 → 300 pixels/inch); TIFF's XResolution (200 → 200);
  PNG's `pHYs` is shown in **pixels/cm** (5905 px/m → 59.05 pixels/cm, `RSUi = 1`); Targa and
  BMP open at 72.
- Fireworks MX ignores EXIF orientation (phone photos open sideways); FireCraft applies it.
- Fireworks MX refused the BMPs ImageMagick writes by default (v5 header, 24- and 32-bit) with
  error -2147216341, and opened a BMP3 file. FireCraft reads both.
- A 16-bit TIFF opened with wrong colours in Fireworks MX (green 183 where the file has 124);
  FireCraft decodes it as other readers do.

Pixels: FireCraft's renders equal Fireworks MX's re-saves exactly for PNG, BMP, Targa and every
GIF probe (and `anim2.gif`); JPEG within 1 level on 60 of 2400 pixels (decoder rounding).

## Animated GIFs

- **One frame**: layer "Layer 1" (not "Background") with the picture; frame delay = the GIF's
  (0 when the file has none).
- **Several frames**: one Fireworks frame per GIF frame, delay = the GIF's delay (both in 1/100 s),
  on a layer named **"GIF"**. How each frame's picture is cut depends on the disposal:
  - every frame **cleared to the background** afterwards: each frame's own picture at its own
    position and size (e.g. a 20×10 bitmap at 30,20);
  - every frame **kept** (disposal "none"/unspecified) **and an opaque, full-size first frame**:
    a shared-across-frames **"Background"** layer holding the first frame, and on "GIF" each
    frame's picture cropped to the area the later frames change (the union of frames 2..n:
    a 45×15 bitmap at 5,20 in every frame, frame 1's included). The canvas becomes the GIF's
    background colour, opaque (`BGCi = ffffffff` for white);
  - anything else (a transparent or smaller first frame, "restore to previous", mixed
    disposals): the first frame's own picture at its position, then each later frame as the whole
    animation shows it at that point, full size. Measured on `ref_fw_images/anim2.gif` (495×228
    screen, frames growing from 167×221 to 489×221 at 5,4, the last one cleared): a 167×221
    bitmap at 5,4, then three 495×228 bitmaps.
- Canvas transparent, except in the Background case above.

The probes: `anim.gif` (white 60×40 first frame, then 20×10 and 10×10 patches, disposal none),
`animbg.gif` (the same with disposal background), `tk.gif` (transparent first frame, disposal
none), `prev.gif` (second frame "restore to previous").

## Formats Fireworks MX doesn't open

Animated PNG, animated WebP and JPEG XL animations become full-size frames on one layer named after
the format ("PNG", "WebP", "JPEG XL"), like the last GIF case. Still images follow the PNG/JPEG
layout (Background + Layer 1).

`ref_fw_images/minions_banana2.webp` (lossy colour with a separately compressed, pre-processed
alpha channel) decodes as libwebp does: alpha identical everywhere, colour identical on every
visible pixel, semi-transparent edges included.

JPEG XL is checked against the libjxl conformance references: `alpha_triangles` exact,
`grayscale` within 1 level, `cafe` (ICC profile) within 2 levels of the reference converted to sRGB.
