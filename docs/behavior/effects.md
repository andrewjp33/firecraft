# Live Effects

Effects are stored per object in `EFLv`: one `EPSv` per effect, each a list of `DCEv{DCKs key,
DCVs value}` pairs (Curves points are a `GPLv` of `GPTv{XLCf,YLCf}`), identified by `EffectMoaID`.
FireCraft parses the effects it renders (`crates/raster/src/effects.rs`); the full list stays in
the object's raw node, so saving never loses an effect.

| Effect | Status |
|---|---|
| Gaussian Blur | **measured** (radii 1-249): `gaussian_blur_radius` is in pixels; Gaussian with sigma = 1.021 x radius (kernel ~2.5 x radius); radii above 70 behave as 70; radii above 200 do not blur at all |
| Drop Shadow | **measured**: offset = round(distance x (cos a, -sin a)), hard edges at softness 0; softness = Gaussian with sigma = 0.475 x `ShadowBlur` (fit RMS 0.006) |
| Hue/Saturation | **bit-exact** (read from `0x7d7433` with REA, verified on 48 renders): byte HSL; hue byte -> whole degrees (x360/256), + `hue_amount`, back (x256/360, wraps); saturation/lightness move by (255-x)*k for k>0, x*k for k<0 (k = amount/100), truncated. Colorize is still approximate |
| Brightness/Contrast, Levels, Auto Levels, Curves, Invert, Convert to Alpha, Color Fill | implemented from parameters; not yet measured |
| Glow / Inner Glow | **measured**: `BevelType` 4; band of pixels at Euclidean distance (offset, offset + width] from the object, unanti-aliased; width = `GlowWidth` (0 = no glow), colour `OuterBevelColor`, gap `GlowStartDistance`, opacity `BevelContrast`, softness sigma = 0.475 x `MaskSoftness` |
| Motion Blur, Radial Blur | approximations |
| Inner/Outer Bevel, Raised/Inset Emboss (`BevelType` 0-3) | **read from the binary + measured**, see bevel.md: within 2-8 levels without softness; mitred corners with softness still differ |
| Eye Candy 4000 LE / Splat LE (Bevel Boss, Motion Trail, Marble, Edges) | built-in versions, see plugins.md: Bevel Boss fitted to Fireworks renders, the others first versions |

Effects see the whole object, including parts outside the canvas or the visible area (a blur
at the canvas edge fades to half, not less), so FireCraft rasterizes the object over the output
area grown by the effect's margin.

Measuring an effect: script it with `dom.applyEffects({category, name, effects:[{EffectMoaID, ...}]})`
on a test shape in Fireworks MX (`tools/fwmx-oracle`), save, and fit FireCraft's output to the
saved render.
