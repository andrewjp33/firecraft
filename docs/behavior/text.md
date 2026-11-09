# Text

The engine is `crates/text`, adapted from FirePixel's `fp-text` (fonts, layout, rasterising). Text objects are
stored as Fireworks' `TXTv`: the block (`LFT`..`BOT`), the first run's attributes, then `TFSv` with attribute changes
before each `TRN` string.

Files saved by Fireworks CS differ: `TFTi` gives the kind of block (0 point text, anchored at its first baseline by
`LFAf`/`TOAf`; 1 a wrapping block given by `LFAf`..`BOAf`; 2 and 3 text on or in a path, shown but not editable yet),
`FONs` is the family and `CFTs` and the runs the PostScript name, and `DTAv` holds its own copy of the story, dropped
when the text is edited. With the same font, point text re-renders to within 0.1% of Fireworks CS's stored image.

## Stored image first

A text object shows the image saved with it (`IMGv`) until it is edited, so a file looks as it was saved whatever
fonts are installed. Editing draws into that same image, and the rest of the renderer doesn't know the difference.
An unedited object keeps Fireworks' record on save, apart from its position.

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
