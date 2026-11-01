# Blend modes (Fireworks MX, measured and read from the binary)

Measured with a scripted Fireworks MX document: 6 base x 6 blend colours per mode, rendered and
saved by Fireworks (`setBlendMode`). The Tint formula and the luma weights were then read from
`Fireworks.exe` with REA: the blend-function selector at `0x56d9c4` maps each mode value to a
function (Tint `0x56d6b0`, Luminosity `0x56d580`, Hue `0x56d270`, Color `0x56d480`, ...).

| Mode | `BLDi` | FireCraft formula | Max error vs Fireworks (0-255) |
|---|---|---|---|
| Normal | absent | source over | 0 |
| Multiply | 2 | b*s/255 | 0 |
| Difference | 3 | abs(b-s) | 0 |
| Screen | 5 | 255-(255-b)(255-s)/255 | 0 |
| Darken / Lighten | 9 / 10 | min / max | 0 |
| Hue | 11 | HSV(h_s, s_b, v_b); grey base unchanged | 5 |
| Saturation | 12 | HSV(h_b, s_s, v_b); grey base unchanged | 6 |
| Color | 13 | HSV(h_s, s_s, v_b) | 1 |
| Luminosity | 14 | HSV(h_b, s_b, luma(s)) | 4 |
| Invert | 17 | 255-b | 0 |
| Erase | 25 | removes the base where the object is | 0 |
| Tint | 29 | b + (s-b)*(255-luma(b))/255, integer, /255 rounding as Fireworks | 0 |

`luma(c) = (109 R + 366 G + 37 B) >> 9`: Rec. 709 weights 0.2125/0.7154/0.0721 scaled by 512,
computed once at startup in Fireworks.

Mode value 4 has a function in the binary (`0x56cc80`) but no menu entry in MX.

The remaining Hue/Saturation/Luminosity error comes from Fireworks converting through **HSL**
with each component quantized to a byte (helpers `0x564e70` RGB->HSL, `0x565110` back); porting
that exactly is a follow-up.
