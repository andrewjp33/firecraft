# Live Effects

Effects are stored per object in `EFLv`: one `EPSv` per effect, each a list of `DCEv{DCKs key,
DCVs value}` pairs (Curves points are a `GPLv` of `GPTv{XLCf,YLCf}`), identified by `EffectMoaID`.
FireCraft parses the effects it renders (`crates/raster/src/effects.rs`); the full list stays in
the object's raw node, so saving never loses an effect.

| Effect | Status |
|---|---|
| Gaussian Blur | **measured**: `gaussian_blur_radius` is in pixels; Gaussian with sigma = 0.715 x radius (fit RMS 0.27 levels) |
| Drop Shadow | **measured**: offset = round(distance x (cos a, -sin a)), hard edges at softness 0; softness = Gaussian with sigma = 0.475 x `ShadowBlur` (fit RMS 0.006) |
| Hue/Saturation, Brightness/Contrast, Levels, Auto Levels, Curves, Invert, Convert to Alpha, Color Fill | implemented from parameters; not yet measured |
| Glow / Inner Glow | **measured**: `BevelType` 4; band of pixels at Euclidean distance (offset, offset + width] from the object, unanti-aliased; width = `GlowWidth` (0 = no glow), colour `OuterBevelColor`, gap `GlowStartDistance`, opacity `BevelContrast`, softness sigma = 0.475 x `MaskSoftness` |
| Motion Blur, Radial Blur | approximations |
| Inner/Outer Bevel, Emboss (`BevelType` 0-3) | not rendered yet |
| Eye Candy plug-ins (Bevel Boss, Motion Trail, Edges) | kept in the file, not rendered (third-party Photoshop plug-ins) |

Measuring an effect: script it with `dom.applyEffects({category, name, effects:[{EffectMoaID, ...}]})`
on a test shape in Fireworks MX (`tools/fwmx-oracle`), save, and fit FireCraft's output to the
saved render.
