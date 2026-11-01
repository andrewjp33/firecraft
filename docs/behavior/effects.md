# Live Effects

Effects are stored per object in `EFLv`: one `EPSv` per effect, each a list of `DCEv{DCKs key,
DCVs value}` pairs (Curves points are a `GPLv` of `GPTv{XLCf,YLCf}`), identified by `EffectMoaID`.
FireCraft parses the effects it renders (`crates/raster/src/effects.rs`); the full list stays in
the object's raw node, so saving never loses an effect.

| Effect | Status |
|---|---|
| Gaussian Blur | **measured**: `gaussian_blur_radius` is in pixels; Gaussian with sigma = 0.715 x radius (fit RMS 0.27 levels) |
| Drop Shadow | angle/distance convention measured (315 deg = down-right); softness scale approximate |
| Hue/Saturation, Brightness/Contrast, Levels, Auto Levels, Curves, Invert, Convert to Alpha, Color Fill | implemented from parameters; not yet measured |
| Glow / Inner Glow | implemented; the parameter that holds the glow colour is not confirmed (a scripted glow with `OuterBevelColor` rendered nothing; files suggest a light glow) |
| Motion Blur, Radial Blur | approximations |
| Inner/Outer Bevel, Emboss (`BevelType` 0-3) | not rendered yet |
| Eye Candy plug-ins (Bevel Boss, Motion Trail, Edges) | kept in the file, not rendered (third-party Photoshop plug-ins) |

Measuring an effect: script it with `dom.applyEffects({category, name, effects:[{EffectMoaID, ...}]})`
on a test shape in Fireworks MX (`tools/fwmx-oracle`), save, and fit FireCraft's output to the
saved render.
