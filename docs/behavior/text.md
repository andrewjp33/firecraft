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
block on opening and cleared on save, which MX reads as the same position. Scaled or rotated text keeps its matrix, so
editing it doesn't yet land in the right place.

## Stored image first

A text object shows the image saved with it (`IMGv`) until it is edited, so a file looks as it was saved whatever
fonts are installed. Editing draws into that same image, and the rest of the renderer doesn't know the difference.
An unedited object keeps Fireworks' record on save, apart from its position.

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
