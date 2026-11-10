# Text

The engine is `crates/text`, adapted from FirePixel's `fp-text` (fonts, layout, rasterising). Text objects are
stored as Fireworks' `TXTv`: the block (`LFT`..`BOT`), the first run's attributes, then `TFSv` with attribute changes
before each `TRN` string.

Files saved by Fireworks CS differ: `TFTi` gives the kind of block (0 point text, anchored at its first baseline by
`LFAf`/`TOAf`; 1 a wrapping block given by `LFAf`..`BOAf`; 2 and 3 text on or in a path, shown but not editable yet),
`FONs` is the family and `CFTs` and the runs the PostScript name, and `DTAv` holds its own copy of the story, dropped
when the text is edited. With the same font, point text re-renders to within 0.1% of Fireworks CS's stored image.

Fireworks MX's blocks have their own line model, whatever the font: each line is an em box with its baseline 80% of the
way down, so the first baseline is `top + 0.8 × size` and the block ends one em below the last line's top. Measured on
every MX-made object in the reference files whose font is available: after this the vertical offset from MX's stored image
is 0, across Arial 10-12, Prototype 18, Trebuchet MS 36 and MoolBoran 59-200, whose ascents differ widely. New text uses
it too; blocks from Fireworks CS records keep CS's rule (first baseline at the font's own Latin ascent).

MX keeps a text object's move in its matrix (`MTXv`) and leaves the block where it was typed. A plain move is applied to the
block on opening and cleared on save, which MX reads as the same position. Any other matrix (scale, rotation, skew,
distortion) stays with the text, as in MX: the block and its formatting are untouched, Scale/Skew/Distort/rotate add to
the matrix, and the text is drawn through it (glyph outlines transformed; a perspective matrix bends the flat drawing).
Editing works in the block's own space: clicks go through the inverse, and the outline, selection and caret through the
matrix. Checked both ways with MX (rotated 30°, scaled 1.5×0.7, skewed, distorted Arial 24): FireCraft draws MX's objects
in place (shift 0, the remaining difference is anti-aliasing), and MX redraws FireCraft's at the same place.
Fireworks CS records keep ignoring the matrix until a CS file shows what it does.

## Stored image first

A text object shows the image saved with it (`IMGv`) until it is edited, so a file looks as it was saved whatever
fonts are installed. Editing draws into that same image, and the rest of the renderer doesn't know the difference.
An unedited object keeps Fireworks' record on save, apart from its position.

## Fill and spacing

A text object can have any fill a path has (Property inspector: the fill kind beside the colour box; a gradient, pattern
or dither shows as a swatch that opens its editor, and its handles show on the canvas). Gradient, pattern and web
dither fills shade the glyphs, sampled at each pixel's canvas coordinate as for paths; a texture applies over them.
Plain colour stays per character. While a font is missing and the text hasn't been edited, a fill change re-shades the
stored glyphs. Checked on `gym2` (a linear gradient over mixed fonts, sizes, widths and spacing): MX redraws
FireCraft's saved file the same way (with the fonts installed; the reference image itself was drawn with a
replacement for Prototype).

Range kerning (`RKNf`) is an em fraction in Fireworks MX (`0.34`: letters 34% of the size apart); Fireworks CS
records count tenths of that. FireCraft keeps MX's unit and converts CS records when reading and writing.

## Text on a path

Fireworks MX keeps text on a path as one object: the text's record keeps its block (`LFT`..`BOT`, as typed) and gets
`TAPv { TAMi mode, CHOf offset, PTHv path }`, the whole path element (stroke and all). FireCraft reads, draws, edits and
writes it (layout from FirePixel's on-path model):

- Text > Attach to Path (Ctrl+Shift+Y): exactly one text object and one path selected; the path becomes part of the text.
- Text > Detach from Path: the path comes back below the text, whose block keeps its size, centred on the path's bounds.
- Orientation (Rotate Around Path, Vertical, Skew Vertical, Skew Horizontal: `TAMi` 0-3), Reverse Direction (the path's
  points reversed, as MX writes them), and the Property inspector's Text offset (`CHOf`).
- Editing: clicks on the characters place the caret along the path; the path is outlined while editing.

Checked with MX: an ellipse attached in FireCraft and saved, then redrawn by MX, is pixel-identical to MX's own Attach to
Path. FireCraft's drawing of MX's on-path text has the right geometry in all four orientations, with the offset and
reversed; the remaining drift along the line is MX's small-size character widths (see Convert to Paths). Fireworks CS
text in a path (`TFTi` 3) is still shown from its stored image only.

## Text Editor

Text > Editor... (Ctrl+Shift+E, from Fireworks MX's shortcut file) opens MX's Text Editor on the selected text object:
font, size, colour, B/I/U, kerning, leading, alignment, scale, baseline, anti-alias and Auto Kern above a text area
that shows the text in its own fonts, sizes and colours, all of it selected. Show Font and Show Size & Color unticked
show it plainly (one font; one size in black) without changing the formatting. The area wraps at its width, scrolls,
and turns dark for light text. Typing, selection and formatting work as on the canvas (it edits a working copy with the
Text tool's code); OK and Apply put it back as one History step, Cancel and Esc leave the object as it was.

## Convert to Paths

Checked against Fireworks MX (`convert_compare` example, MX's own conversion of the same file): each text object becomes
a group with one path per character in text order (none for spaces), then an underline path after each stretch of
underlined characters. The outlines are the fonts' unhinted ones with each line's baseline on a whole pixel, through the
text's matrix; MX's nodes and handles match to within 0.06 px (Arial, Trebuchet MS). Each path has the text's fill
(a gradient keeps its handles) or its character's colour, and no stroke; the group keeps the text's effects, opacity
and blending.

Underlines in Fireworks MX text: whole pixels, the top at the font's underline position below the baseline rounded, the
thickness the font's rounded, one per stretch of one colour, in that colour (measured: Trebuchet MS 30 -> 178..180 under
baseline 174). Fireworks CS records keep FirePixel's measured rule.

Known differences: Fireworks MX widens small text differently (its per-character advances at 8-24 px differ from the
fonts' by up to ~0.25 px each; closest model so far: hinted at 4x the size, unrounded), and its synthetic bold (a font
without a bold file) keeps the outlines' curves and is narrower than FireCraft's. Both are on the roadmap.

## Text menu

Read from Fireworks MX's menu resource and shortcut file, and checked in MX: Font, Size (Other..., Smaller and Larger
change by 1 pt, presets 8-120), Style and Align are always enabled and change the text being edited, the selected text
objects, or the Text tool's settings, with the current values checked. Shortcuts: Plain/Bold/Italic/Underline
Ctrl+Alt+Shift+P/B/I/U (also F5/F6/F7 for the first three), Align Left/Centered/Right/Justified/Stretched
Ctrl+Alt+Shift+L/C/R/J/S; they work while editing too. Editor... and Convert to Paths need text selected, Attach to Path
text and a path (Ctrl+Shift+Y), Convert to Paths is Ctrl+Shift+P, Check Spelling Shift+F7. The vertical Align items,
Orientation and Reverse Direction are for vertical text and text on a path.

## Font list

While the font list is open, typing finds a font by name, as in Fireworks: a letter goes to the next font starting with
it (pressing it again steps on), and letters typed within a second narrow the search. Arrows, Page Up/Down, Home, End and
Enter work too. The list opens at the current font.

## Missing fonts

Opening a document whose fonts aren't here offers Fireworks MX's choice: **Maintain Appearance** keeps the stored
images, and text you edit afterwards uses a replacement font; **Replace Fonts** swaps them now and redraws.

FireCraft goes further than Fireworks MX in two ways: changing only the colour of text whose font is missing
recolours its stored glyphs instead of redrawing them in a replacement font, and a **Fonts folder** in Preferences
adds a folder of `.ttf`/`.otf` files, such as fonts recovered from backups. The file keeps the original font name, so
text picks the real font up again once it is available. In the browser, **Use Installed Fonts...** (Preferences and
Missing Fonts) asks for permission to use the computer's fonts, in browsers that offer it (Chrome and Edge). Names
are listed at once and each font's data is fetched when something needs it; the permission lasts between visits. The
Fonts folder works everywhere, and egui's own fonts stand in until there is something else.

## Using the tool

A click starts a block that grows with its text; a drag sets a width to wrap to. Clicking text edits it; double-clicking
it with the Pointer edits it with all of it selected. Escape, clicking elsewhere or changing tools finishes, as one
History step; text left empty is removed. With characters selected the Property inspector formats them; with only a
caret, its changes apply to what you type next.

A block's height follows its text (measured: `BOT` = top + size x leading x lines). Fireworks MX's record has no flag
for a growing block, so FireCraft adds `FAWi`, which Fireworks MX ignores.

## Compared with Fireworks MX

In a font both have, line heights match exactly and widths are usually within a few pixels, with no tuning; the
Fireworks MX used for this runs under Wine, whose font hinting differs from Windows'. Bold italic is the largest
difference (about 6 pixels wide in one test).
