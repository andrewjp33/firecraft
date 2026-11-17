# Canvas Size and Image Size (Modify > Canvas)

Measured in Fireworks MX under Wine on 2026-11-16 (private Xvfb, `tools/fwmx-oracle/xin.py`,
results read with a `.jsf` that dumps the elements and saves). Probe:
`crates/engine/examples/crop_probe.rs` (300 x 200, 72 ppi). Resolution storage and units in
general: [resolution.md](resolution.md).

Fireworks MX's Modify > Canvas menu: Image Size..., Canvas Size..., Canvas Color... | Trim
Canvas, Fit Canvas | Rotate 180°, Rotate 90° CW, Rotate 90° CCW.

## Canvas Size

The dialog: **New Size** (width ↔ and height ↕, each with its own unit list Pixels | Inches |
Centimeters, and the resulting pixels on the right as "W: 300", "H: 200"), **Anchor** (3 x 3
buttons, the centre chosen when the dialog opens), **Current Size** (both sides in pixels), OK,
Cancel.

| What | Fireworks MX |
|---|---|
| Inches, Centimeters | Converted with the document's resolution, shown to 6 significant digits: 300 px at 72 ppi = 4.16667 in, 200 px = 7.05556 cm |
| Typed lengths | Truncated to whole pixels: 4.21 in (303.12 px) and 4.22 in (303.84) both give 303; 7.08 cm (200.69) gives 200 |
| Centre anchor, odd change | The left/top gets ceil(change / 2): 300x200 -> 401x261 put 51 px on the left and 31 on top; 401x261 -> 300x200 took 50 off the left and 30 off the top |
| Objects | Keep their document coordinates; the canvas origin (root `XLCf`/`YLCf`) moves (-51, -31 above), as with cropping |
| Shrinking | Works as Crop Document: objects wholly outside are deleted and bitmaps are trimmed; a 120x90 bitmap whose only part inside the 160x100 canvas was a transparent corner was deleted |
| Undo step | "Crop Document" (the same as cropping) |

FireCraft: `DocState::canvas_size` (a crop to `canvas_size_rect`), `size_dialogs::CanvasSize`.

## Image Size

The dialog: **Pixel Dimensions** (↔, ↕, each with Pixels | Percent), **Print Size** (↔, ↕ with
Percent | Inches | Centimeters, and Resolution with Pixels/Inch | Pixels/cm), a bracket with a
lock beside the constrained rows, **Constrain Proportions**, **Resample Image** with Bicubic |
Bilinear | Soft | Nearest Neighbor, OK, Cancel. Constrain, Resample and the method are
remembered (preferences `ResizeConstrain`, `ResizeResample`, `ImageSizeTransformMode`); print
sizes start in Centimeters for a pixels/cm document. The method is Image Size's own, separate
from Preferences > General > Interpolation (`EditTransformMode`, used for transforming
objects: [objects.md](objects.md#interpolation)): setting that to Soft left Image Size at
Bicubic.

| What | Fireworks MX (160 x 100 at 72 ppi unless stated) |
|---|---|
| Sizes are kept unrounded | Constrained, 161 px wide shows 101 px high (100.625 rounded) but 1.398 in (100.625 / 72), not 101 / 72 |
| Rounding of the linked pixel value | Nearest, halves up: 164 -> 102.5 -> 103; 165 -> 103.125 -> 103; 107 high -> 171.2 -> 171 wide |
| Print size display | Up to 3 decimals, trailing zeros dropped: 2.222, 4, 2.5, 0.694 |
| Percent | 168 px shows 105; 150% gives 240 x 150 |
| Resample on, resolution changed | The print size stays, the pixels change: 108 ppi made 240 x 150 |
| Resample off | Pixel fields disabled; Constrain checked and disabled; the bracket takes in the resolution. 144 ppi halves the print size (1.111 x 0.694 in); 4 in wide makes 40 ppi (and 2.5 in high) |
| Resolution unit changed | Converted exactly and shown to 3 decimals (72 ppi -> 28.346 px/cm, 108 -> 42.52); pixels and print size stay. See [resolution.md](resolution.md#image-size-modify--canvas) for what MX stores |
| Scaling | Everything scales about Fireworks' **document** origin by new/old pixels per axis (401 -> 602: x1.50125, 261 -> 392: x1.50192); the canvas origin scales too, rounded (-1 at x1.5 -> -2; -51, -31 -> -77, -47) |
| Bitmaps | Placed in document coordinates at round(x * s), size ceil(w * s): 5,175 30x20 -> 8,263 46x31; 150,90 120x90 -> 225,135 181x136 |
| Vector objects | Points scaled exactly (10,10 80x60 -> 15.0125,15.0192 120.1x90.1) |
| Undo step | "Image Size" |

FireCraft: `DocState::image_size`, `size_dialogs::ImageSize`. After Canvas Size 401x261 and
Image Size 108 ppi on the probe, Fireworks MX opened FireCraft's file with the same document
size, origin and object positions as its own (vectors equal to float precision, bitmaps
exactly).

## Not matched yet

* Resampling: FireCraft's Bicubic is Catmull-Rom (a = -0.5) on premultiplied colour, Soft a
  cubic B-spline. Against Fireworks MX's Bicubic on the probe's 120x90 bitmap at x1.5 the mean
  difference is 0.8 levels (max 31, on the soft alpha edge); Bilinear 0.9, Soft 1.2. Fireworks'
  own kernels are unmeasured; `crates/engine/examples/image_size_probe.rs` makes comparable
  files.
* Whether Image Size scales stroke widths, effects (blur radii, shadow distances) and text
  sizes as FireCraft's transform does: unmeasured.
* Canvas Size's anchor and units on reopening: FireCraft starts at the centre in pixels.
