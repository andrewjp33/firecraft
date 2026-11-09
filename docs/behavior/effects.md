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
| Brightness/Contrast, Levels, Auto Levels, Curves, Invert, Convert to Alpha, Color Fill | implemented from parameters; not yet measured |
| Glow / Inner Glow | **measured**: `BevelType` 4; band of pixels at Euclidean distance (offset, offset + width] from the object, unanti-aliased; width = `GlowWidth` (0 with no softness = no glow; 0 with softness acts as a 2-pixel band, fitted on one file at softness 7), colour `OuterBevelColor`, gap `GlowStartDistance`, opacity `BevelContrast`, softness is a triangle filter of radius `MaskSoftness`; with no gap the object's own pixels feed the softening but aren't painted |
| Motion Blur, Radial Blur | approximations |
| Inner/Outer Bevel, Raised/Inset Emboss (`BevelType` 0-3) | **read from the binary + measured**, see bevel.md: within 2-8 levels without softness; mitred corners with softness still differ |
| Eye Candy 4000 LE / Splat LE (Bevel Boss, Motion Trail, Marble, Edges) | built-in versions, see plugins.md: Bevel Boss fitted to Fireworks renders, the others first versions |

Effects see the whole object, including parts outside the canvas or the visible area (a blur
at the canvas edge fades to half, not less), so FireCraft rasterizes the object over the output
area grown by the effect's margin.

Measuring an effect: script it with `dom.applyEffects({category, name, effects:[{EffectMoaID, ...}]})`
on a test shape in Fireworks MX (`tools/fwmx-oracle`), save, and fit FireCraft's output to the
saved render.

**Softness of shadows and glows.** A triangle filter of the softness's radius along each axis, truncating to whole
levels per pass in Fireworks MX color math and keeping fractions in FireCraft's (FirePixel's measurement), so an Inner Shadow of softness 4 ends exactly 4 pixels in. Shadow offsets
are truncated, not rounded. `innerShadowZero.png` matches Fireworks MX with no differing pixels. Fireworks MX's popup
puts Inner Glow's width back to 2 if set to 0 or 1; Glow and Inner Shadow have no such minimum.
