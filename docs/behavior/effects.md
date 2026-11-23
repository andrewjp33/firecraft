# Live Effects

Effects are stored per object in `EFLv`: one `EPSv` per effect, each a list of `DCEv{DCKs key,
DCVs value}` pairs (Curves points are a `GPLv` of `GPTv{XLCf,YLCf}`), identified by `EffectMoaID`.
FireCraft parses the effects it renders (`crates/raster/src/effects.rs`); the full list stays in
the object's raw node, so saving never loses an effect.

| Effect | Status |
|---|---|
| Gaussian Blur | **exact** (FirePixel's reconstruction, adapted, Apache-2.0): sigma = `gaussian_blur_radius` (0.1-70; above 70 as 70); separable kernel to ceil(3 sigma), `f32` weights, horizontal then vertical pass each rounding down to 8 bits; colour averaged over pixels with alpha > 0 (not alpha-weighted); output ceil(2.5 sigma) beyond the object. Above 200: no blur; the object's pixel rectangle (bounds of its pixels with alpha > 0) is filled fully opaque with the plain average of every pixel in it, transparent ones counting as white (FirePixel's rule; verified in Fireworks MX with circleBlur.png: a red circle becomes a (255, 53, 53) square. An earlier FireCraft measurement used a rectangle, whose shape can't change, and wrongly concluded the shape is kept) |
| Drop Shadow | **measured**: offset = trunc(distance x (cos a, -sin a)), hard edges at softness 0; softness = triangle filter of radius `ShadowBlur` (see below) |
| Hue/Saturation | **bit-exact** (Fireworks MX's routine `0x7d7433`, verified on 48 renders): byte HSL; hue byte -> whole degrees (x360/256), + `hue_amount`, back (x256/360, wraps); saturation/lightness move by (255-x)*k for k>0, x*k for k<0 (k = amount/100), truncated. Colorize is still approximate |
| Brightness/Contrast | **measured** (grey ramp, 14 settings; every value within one level, 90.8% exact): brightness added and clamped first; contrast scales around 128 by 100/(100−c) for c > 0 (50 doubles, 75 quadruples), (100+c)/100 for c < 0 (−100: flat 128); 100 is a hard threshold at 128 (Fireworks MX's 99 is gentler, slope ~11); rounded |
| Convert to Alpha | **measured**: black, alpha 255 − Rec. 709 luma (grey and color ramps exact) |
| Levels, Auto Levels, Curves | **bit-exact** (filter probe, below): four 256-entry tables applied as `rgb[channel[v]]` (the channel's table first, then RGB's) to every pixel, alpha unchanged. Levels: input clamped to low..high, `t = (x - low)/(high - low)` raised to 1/gamma (gamma 0.1..10), output `low + t (high - low)` truncated; values read as bytes; an empty input range gives 0. Auto Levels: red, green and blue each stretched from the 0.5% limits of their histogram (pixels with alpha > 0). Curves: flat before the first point and after the last; between them cubic segments whose inner handles lie along the line joining the neighbours, half the distance to each, flattened adaptively and sampled with rounding (FirePixel's reconstruction) |
| Blur, Blur More, Sharpen, Sharpen More, Find Edges | **bit-exact** (filter probe): 3x3 integer filters on straight (not premultiplied) values, alpha included (Find Edges keeps each pixel's alpha), division towards zero, clamped. Blur: neighbours 1, centre 8, /16; Blur More: centre 4, /12; Sharpen: neighbours -1, centre 18, /10; Find Edges: neighbours 1, centre -8. As Live Effects Sharpen and Sharpen More draw identically (no `sharpen_amount` stored: strength 1); the Filters menu uses strengths 4 and 8 (FirePixel; not yet checked in Fireworks MX). The filters stay inside the object's rectangle (a bitmap's own rectangle, not just its visible pixels: Blur fills the transparent corners of a 96x72 bitmap but never goes beyond it) and repeat its edge pixels; **the colour stored in transparent pixels takes part** (green transparent pixels give a green fringe) |
| Unsharp Mask | within 2-4 levels: where a channel (alpha too) exceeds its Gaussian-blurred value by more than the threshold it gains `(round(amount% x 255) x difference) >> 8`. Fireworks MX's blurred reference isn't FireCraft's Gaussian (on a smooth ramp MX's sharpening steps unevenly, e.g. +4, -0, +2 on neighbouring pixels); not yet found |
| Invert | 255 - v. **Measured, bit-exact**: on semi-transparent pixels Fireworks MX draws the inverted pixel over the original. Alpha A = a + round(a(255-a)/255) (60 becomes 106); colour o + (i - o)·a/A with the step truncated toward zero (o original, i inverted), so faint pixels land near the middle: alpha 1 gives about 128 whatever the colour. Fitted with `crates/io/examples/stack_probe.rs` on SOLAR.png's sun (white at every alpha) and supermarket.png's shading (black at every alpha), every pixel exact. This decides Auto Levels after Invert: one straight-colour 254 at alpha 1 used to stretch SOLAR's sun to the wrong range. Precise colour math: the same blend in f64, rounded once (up to 3 levels apart on very faint pixels, where MX rounds the new alpha first) |
| Color Fill | **Measured** (`crates/io/examples/color_fill_probe.rs`, all 13 modes at 100% and 50% on alpha 5..255): the fill is composited onto the pixel with the pixel's own alpha (W3C: it shows through unblended where the pixel is faint), and the pixel keeps its alpha. At 100%: round(((255 - a)·fill + a·B)/255) with B the mode's rounded blend, bit-exact in every mode. Exceptions: Invert and Tint draw their blended result over the pixel (at 100% alpha doesn't matter); Erase fills as Normal. 50% is within 2 levels. Fireworks MX takes the opacity as a byte (50% is 127). Precise colour math: the same compositing in f64 with the exact opacity (50% is 0.5) and the blend not rounded before mixing, rounded once; within 1 level of Classic (2 on very faint pixels at partial opacity; 2-3 for Hue/Saturation/Color/Luminosity, whose blends differ in Precise anyway) |
| Grayscale (FireCraft) | Adjust Color > Grayscale: each pixel's luminance (sRGB to linear light, Y = 0.2126 R + 0.7152 G + 0.0722 B, back to sRGB), so colors keep their perceived lightness (pure blue 76, red 127, green 220). Stored as Hue/Saturation at saturation −100 plus a `firecraft_effect = grayscale` parameter: Fireworks MX keeps the effect (and the parameter, on re-save) and shows its own desaturation, HSL lightness (51 for 102,48,0 where Grayscale gives 63). Edited in Fireworks MX it becomes that Hue/Saturation: Fireworks MX keeps the parameter but renames the effect "Hue/Saturation..." and stores the new values, so FireCraft reads it as Grayscale only while the name is "Grayscale" and the values are still saturation −100, hue 0, lightness 0, no colorize (minions_banana2_gray.png: edited to −60 in Fireworks MX, now drawn as that, pixel-identical) |
| Glow / Inner Glow | **measured**: `BevelType` 4; colour `OuterBevelColor`, gap `GlowStartDistance`, opacity `BevelContrast`, softness a triangle filter of radius `MaskSoftness`. **Glow** (oracle probe, 2026-11-10, below): every visible pixel is the object (alpha 1 glows like 255); hard, the band is anti-aliased, coverage `clamp(width + 1 − d, 0, 1)` at Euclidean distance d; softened, a hard band `d ≤ width + 1`; with no gap the object's pixels feed the softening and the glow is painted under them too (it shows through semi-transparent pixels); opacity is taken as a byte (65% = 165) and the product truncated. Within 1 level (hard) and 2 (softness 12) of Fireworks MX. **Inner Glow**: a hard band `offset < d ≤ offset + width` inside the half-alpha edge (not re-measured) |
| Motion Blur, Radial Blur | approximations |
| Inner/Outer Bevel, Raised/Inset Emboss (`BevelType` 0-3) | **Fireworks MX's formulas + measured**, see bevel.md: within 2-8 levels without softness; mitred corners with softness still differ |
| Eye Candy 4000 LE / Splat LE (Bevel Boss, Motion Trail, Marble, Edges) | built-in versions, see plugins.md: Bevel Boss fitted to Fireworks renders, the others first versions |

Effects see the whole object, including parts outside the canvas or the visible area (a blur
at the canvas edge fades to half, not less), so FireCraft rasterizes the object over the output
area grown by the effect's margin.

Measuring an effect: script it with `dom.applyEffects({category, name, effects:[{EffectMoaID, ...}]})`
on a test shape in Fireworks MX (`tools/fwmx-oracle`), save, and fit FireCraft's output to the
saved render.

**Glow probe.** White 30×40 blocks at alpha 1, 4, 16, 64, 127, 128, 192 and 255 in one bitmap, given
`ref_fw_images/minions_banana2_glow.png`'s Glow (width 5, 65%, `#df0000`) by `dom.applyEffects` at
softness 0 and 12, saved by Fireworks MX and compared pixel by pixel. That file (a WebP's soft
floor shadow, which glows in Fireworks) and gym, gym2, gym3 and circles_hi now match at 100.0%.

**Effects of a rectangle.** A rectangle primitive's record (`PTHv > PRIv`) carries its own copy of
the effect list (`EFLv`, between `MTXv` and `RCTv`), and Fireworks MX takes a rectangle's effects from
there: FireCraft wrote only the path's copy, so a rectangle saved by FireCraft opened in Fireworks MX
with no effects (smoke_fc.png: 4 rectangles, 0 effects each; 2 each once written).

**Effects in a text's stored image.** Fireworks draws a text object's Live Effects into the image
it stores with it (`IMGv`; a glowing text made in Fireworks MX by script stores the glow). An
unedited text shows that image as it is; FireCraft used to apply the effects a second time (gym's
text glow was twice as dark). Changing the text's effects draws it afresh first.
FireCraft writes text it has drawn the same way, with the effects in the stored image, so the file
reopens unchanged (in FireCraft and, by the same rule, in Fireworks).

**Softness of shadows and glows.** A triangle filter of the softness's radius along each axis, truncating to whole
levels per pass in Fireworks MX color math and keeping fractions in FireCraft's (FirePixel's measurement), so an Inner Shadow of softness 4 ends exactly 4 pixels in. Shadow offsets
are truncated, not rounded. `innerShadowZero.png` matches Fireworks MX with no differing pixels. Fireworks MX's popup
puts Inner Glow's width back to 2 if set to 0 or 1; Glow and Inner Shadow have no such minimum.

**Filter probe** (`crates/io/examples/filter_probe.rs`, 2026-11-14). Fifteen settings (none, Blur, Blur More,
Sharpen, Sharpen More, Find Edges, three Unsharp Masks, Auto Levels, two Levels, two Curves, Invert) on three
96x72 bitmaps each: a colour-ramp ellipse with a soft edge on transparent black, the same with green
transparent pixels, and an opaque ramp rectangle in 40..200. Saved through Fireworks MX
(`fw_inspect.sh`) and compared cell by cell with `crates/io/examples/cell_compare.rs`. Everything but Unsharp
Mask and Invert matches every pixel. Fireworks MX fills in a new filter effect's `name` and `category` itself
(`dom.applyEffects` with just the MoaID: "Blur", "Blur More", "Sharpen", "Sharpen More", "Unsharp Mask...",
"Find Edges", "Auto Levels", "Levels...", "Curves...", "Invert"); Curves gets four empty point lists,
Unsharp Mask no values (its dialog supplies them; FireCraft uses FirePixel's 50%, 4.1, 0).

**Not yet measured**: Brightness/Contrast on semi-transparent pixels. supermarket.png (87%, also after a fresh
Fireworks MX re-save) has two nearly transparent black bitmaps given Color Fill white then Brightness/Contrast
(-50/33, -55/50); Fireworks MX leaves them almost invisible, FireCraft draws them 15-24 levels lighter. The
filters on objects other than bitmaps (paths, text) work on the visible pixels' rectangle; unchecked.

**Filters menu.** The same effects applied for good: to the selected pixels of a bitmap (blended in
through the selection's soft edge), otherwise to each selected object. Objects that aren't bitmaps are
first turned into bitmaps of how they look (Live Effects drawn in), after Fireworks' question "Applying
filters will convert the selection to a bitmap. Continue?". Filters that grow the image (Gaussian Blur)
grow the bitmap. Popups preview on the canvas; Repeat Filter (Ctrl+Alt+Shift+X) reapplies the last one
with its settings, which its popup also starts from next time.

## Last settings and Defaults

Fireworks MX keeps its tools' last settings between sessions (`Fireworks MX Preferences.txt`:
`BrushCurrentColor`, `FillCurrentColor`, `WandToolTolerance`, `EraserToolDiameter`, `Text_Font`...).
FireCraft keeps everything new objects and the painting tools take from the tool options (colors,
stroke category, size, edge and texture, fill category and gradient, Gradient tool, text formatting,
Eraser, Paint Bucket, Magic Wand and Marquee settings) under the storage key `firecraft.tools`, and
each effect's last confirmed settings under `firecraft.effects` (`crates/ui-egui/src/tool_settings.rs`).
A newly added Live Effect, or a Filters menu popup, starts from the settings last confirmed with OK for
that effect (Cancel forgets them); Live Effects and the Filters menu share them. Every effect popup has
a **Defaults** button (a FireCraft addition) that puts all its settings back to Fireworks' defaults.
Textures and patterns are stored by name and looked up once their folders are read.
