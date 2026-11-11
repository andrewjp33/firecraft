# Live Effects

Effects are stored per object in `EFLv`: one `EPSv` per effect, each a list of `DCEv{DCKs key,
DCVs value}` pairs (Curves points are a `GPLv` of `GPTv{XLCf,YLCf}`), identified by `EffectMoaID`.
FireCraft parses the effects it renders (`crates/raster/src/effects.rs`); the full list stays in
the object's raw node, so saving never loses an effect.

| Effect | Status |
|---|---|
| Gaussian Blur | **exact** (FirePixel's reconstruction, adapted, Apache-2.0): sigma = `gaussian_blur_radius` (0.1-70; above 70 as 70); separable kernel to ceil(3 sigma), `f32` weights, horizontal then vertical pass each rounding down to 8 bits; colour averaged over pixels with alpha > 0 (not alpha-weighted); output ceil(2.5 sigma) beyond the object. Above 200: no blur; the object's pixel rectangle (bounds of its pixels with alpha > 0) is filled fully opaque with the plain average of every pixel in it, transparent ones counting as white (FirePixel's rule; verified in Fireworks MX with circleBlur.png: a red circle becomes a (255, 53, 53) square. An earlier FireCraft measurement used a rectangle, whose shape can't change, and wrongly concluded the shape is kept) |
| Drop Shadow | **measured**: offset = trunc(distance x (cos a, -sin a)), hard edges at softness 0; softness = triangle filter of radius `ShadowBlur` (see below) |
| Hue/Saturation | **bit-exact** (read from `0x7d7433` with REA, verified on 48 renders): byte HSL; hue byte -> whole degrees (x360/256), + `hue_amount`, back (x256/360, wraps); saturation/lightness move by (255-x)*k for k>0, x*k for k<0 (k = amount/100), truncated. Colorize is still approximate |
| Brightness/Contrast | **measured** (grey ramp, 14 settings; every value within one level, 90.8% exact): brightness added and clamped first; contrast scales around 128 by 100/(100−c) for c > 0 (50 doubles, 75 quadruples), (100+c)/100 for c < 0 (−100: flat 128); 100 is a hard threshold at 128 (Fireworks MX's 99 is gentler, slope ~11); rounded |
| Convert to Alpha | **measured**: black, alpha 255 − Rec. 709 luma (grey and color ramps exact) |
| Levels, Auto Levels, Curves, Invert, Color Fill | implemented from parameters; not yet measured |
| Glow / Inner Glow | **measured**: `BevelType` 4; colour `OuterBevelColor`, gap `GlowStartDistance`, opacity `BevelContrast`, softness a triangle filter of radius `MaskSoftness`. **Glow** (oracle probe, 2026-11-10, below): every visible pixel is the object (alpha 1 glows like 255); hard, the band is anti-aliased, coverage `clamp(width + 1 − d, 0, 1)` at Euclidean distance d; softened, a hard band `d ≤ width + 1`; with no gap the object's pixels feed the softening and the glow is painted under them too (it shows through semi-transparent pixels); opacity is taken as a byte (65% = 165) and the product truncated. Within 1 level (hard) and 2 (softness 12) of Fireworks MX. **Inner Glow**: a hard band `offset < d ≤ offset + width` inside the half-alpha edge (not re-measured) |
| Motion Blur, Radial Blur | approximations |
| Inner/Outer Bevel, Raised/Inset Emboss (`BevelType` 0-3) | **read from the binary + measured**, see bevel.md: within 2-8 levels without softness; mitred corners with softness still differ |
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
