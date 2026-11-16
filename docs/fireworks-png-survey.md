# Fireworks PNG survey: unimplemented and unknown data

A sweep of `../ref_fw_images` (100 files: 87 with a Fireworks document, the rest flat PNGs) on
2026-11-15, to list what FireCraft doesn't read or write yet, for later investigation. Method:

* PNG chunks: every chunk of every file, by type, with its contents checked.
* Tree tags: every node of every `mkTS` tree (`crates/fwpng/examples/dump_tree`), compared with
  the tags FireCraft's code names anywhere (a tag never named is not interpreted; it may still be
  kept, see below).
* Saving: every Fireworks file re-saved with FireCraft (`crates/io/examples/resave`) and its tree
  compared tag by tag with the original. Tags not listed under "Changed by a FireCraft save" keep
  their count, so FireCraft carries them over even where it doesn't understand them.

Format background: [fireworks-png.md](fireworks-png.md).

## PNG chunks

| Chunk | Files | Status |
|---|---|---|
| `IHDR`, `IDAT`, `IEND`, `pHYs`, `sBIT`, `PLTE`, `tRNS` | all / most | Standard PNG; FireCraft writes its own flattened image. |
| `mkBF` | 87 | Identical in every file (`FADECAFE 00000004`, 64 zero bytes); FireCraft writes the same. |
| `mkTS` | 87 | The document tree; read and written. |
| `mkBT` | 87 (6221 tiles) | Pixel tiles. All start `FACECAFE`; flags only 0 (6038) or 1 (183); reserved bytes always zero. Read and written. |
| `mkBS` | 84 | **Solved:** a stub document, one 256x172 image reading "This file was created in Fireworks" in 8 languages, "Fireworks 4, http://www.macromedia.com", presumably what older Fireworks versions show instead of the real document. The CS5 variant adds `LCKb` and `DTAv`. FireCraft copies it from the opened file; new documents get none. |
| `prVW` | 74 | **Format found, not written:** zlib, then `CAFEBEEF`, thumbnail width and height as u16 (112x84 for a 200x150 canvas, longest side 112), a 256-colour RGB palette (768 bytes) and a 128x128 buffer of palette indices (17160 bytes inflated; 17672 in a CS5 file). Probably the preview in Fireworks' Open dialog. FireCraft writes no `prVW`, so its files have none (the original's is dropped, not left stale). To do: write one, after checking what Fireworks shows without it. |
| `tEXt` | 84 | `Software` (Fireworks 4.0, MX, MX 2004, 8, CS5, "Adobe Fireworks") and `Creation Time` (`MM/DD/YY`). FireCraft doesn't write them. |
| `iTXt` | 7 | `XML:com.adobe.xmp`: Adobe XMP metadata in CS-era files (CS5 tests, supermarket, tm). Dropped on save. |

No other chunk types occur.

## Changed by a FireCraft save

Intended, already documented elsewhere:

* Fireworks CS root tags (`IMPb`, `VRMi`, `VRSi`, `PILb`, `ILLb`, `PIDs`, `LSMv`, `ZSLv`) are
  dropped so Fireworks CS5 also opens the file ([fireworks-png.md](fireworks-png.md#fireworks-cs-files)).
* CS 9-slice data (`NGSv` with `NGMv`/`NGVv`, edges `TOPf LFTf RITf BOTf`, `NGKi`, `NGLb`) is
  rewritten as `FNGv` for skewed 9-slice symbols ([behavior/nine-slice.md](behavior/nine-slice.md)).
* `TMCi` single-colour tiles are written wherever a tile is one colour, so counts change both ways.
* An empty symbol library (`MSLv` with no symbols, 44 files) is left out.
* A document without export settings gets FireCraft's (`EXPv` and its children, seen as added in
  shapes_fc).

To investigate (could lose data, or Fireworks may mind):

| What | Files | Notes |
|---|---|---|
| Cells of a layer shared across frames | mechscene2 (10 -> 1), mechscene3, mechscene2_loop* | Fireworks writes one cell per frame (content in the first, the rest empty); FireCraft writes one. Check Fireworks MX opens the result with all frames right. |
| Cells inside symbols | Animations (symbol cells 68 -> 53) | Multi-frame (animated or button) symbols; probably empty cells as above. Check frames and button states survive. |
| `VISb` in symbol cells | Buttons (12 lost), Nav Bar Buttons, Symbian_Series60, Themes, 769_edit2 | Per-cell visibility inside button symbols; dropped. |
| `UNMs` in a texture (`TXFv`) | Buttons, Themes | A texture's second name (FireCraft reads `INMs`); dropped. |
| `FGDb` in a gradient (`FGYv`) | the CS5 9-slice test files | A gradient flag (dither?) inside a symbol; dropped. |
| Gradient data added | Themes | FireCraft writes `FGLv/FGYv/FG0v/FG1v` for fills that had none. Harmless if Fireworks ignores it; check. |

The parity report's "save" check only compares the first frame's render, so frame, button state
and symbol-frame losses would not show there.

## Tags FireCraft doesn't interpret (kept on save)

Counts are files / occurrences. Meanings are guesses unless stated.

**Document root and defaults**

| Tag | Files | Where | Values / guess |
|---|---|---|---|
| `RQNi` | 84 / 84 | root | `2c3eb8c7` in 57 files: a constant or version stamp. |
| `PLLv`, `FLDv`, `GLDv`, `EFDv` | 84 | root | Stroke, fill, gradient and effect preset tables (Fireworks' defaults). |
| `MNDs`, `MNTi` | 76, 1 | `MNFv` | Unknown (`MNFv` is a root record). |
| `PGNs` | 1 | root | `Page 1`: Fireworks CS4+ pages (supermarket). |
| `LXDs`, `LXFs` | 6-7 | root | Last export folder and file name. |
| `LLLi` | 14 / 44 | `LAYv` | Always 0, CS5 files: a layer field (lock level?). |

**Export settings (`EXPv`)**

`EFXs` (export file extension filter, `*.png`), `TPWi`, `CSMb`, `ECSi`, `ENCi`, `XTMi`,
`NS0s`-`NS3s`, `CBUs`, `CBRi`, `CBAi`, `CDAi`, `CDMv`, `CXBb` (12-21 files), `IMDs` (import
folder), `EFBs` (export base name), `MCTi`/`MCCi` (Mac file type and creator, `TIFF`/`MKBY`).
HTML/slicing export options, file locations and naming; see [behavior/export.md](behavior/export.md)
for the fields FireCraft maps.

**Brush presets (`BPLv`): pressure and speed response**

`SSZi SPOi SSOi SPBi SSBi SRBi SRZi SRHi SRAi SRSi SROi SPRi SRRi SSRi SPHi SSHi` (76 files) and
`SHBi SVBi SPLi SSLi SRLi SHAi SVAi SHHi SVHi SHLi SVLi` (57), `SVZi`, `SHZi` (1). Values look like
per-mille amounts (`3e8` = 1000). Likely Fireworks' "Sensitivity" settings (pressure / speed /
horizontal / vertical / random against size, ink amount, angle, blackness, hue, lightness,
saturation, scatter). Not used by FireCraft's brush engine; they matter for pen pressure and the
random brushes.

**Fill presets**: `FSXi` (19 files, 1706): mostly 0, some 2/3/5; a fill preset field (texture or
edge?).

**Path data**

| Tag | Files / count | Where | Guess |
|---|---|---|---|
| `NSDf`, `BZLv`, `BZCi`, `BAZv` | 66 / ~22000 | `PBTv` (`PBPv`) | Per-point data of the stored stroke (`BZCi` always 1). |
| `CRVb` | 27 / 2529 | `PBTv` | Always 1: a curve flag per point. |
| `DURi`, `ONCb` | 66 / 57587, 25035 | `PPTv` (`PPLv`) | Pressure points: duration (always 0), "on curve" (always 1). |
| `POPi` | 12 / 92 | `PTHv` | Values like `e75ae8c`: an id or pointer. |
| `RDMb` | 12 / 92 | `PRIv` | Always 0: a primitive flag (random?). |

**Text (`TXTv`)**: `TA1i` (`c0`), `TA2i` (`40`), `TOSi` (8), always the same, 13 files; `FOAb`
(always 0, 8 files). Probably anti-aliasing and oversampling settings.

**Symbols, instances, buttons**

| Tag | Files | Where | Guess |
|---|---|---|---|
| `LUTs`, `LFNs`, `LMNi` | 37, 10, 10 | `MSTv` | Library symbols: last update time, linked library file (`file:///...` paths), modification number. Needed for Update from library. |
| `ATMv`, `ALNi` | 8, 4 | `ALSv` (instance) | Instance data; `ALNi` looks like an id. |
| `BUTs` | 1 | `ALSv` | Button text (`HOME`, `RATES`): button symbol labels (tut2_final). |
| `LNKb` | 1 | `GRPv` | A group link flag (tablet). |

**Web: URLs and behaviors (`URLv`)**

`URSi`, `TSLi`, `TDTs` (URL settings, 4-6 files) and the behaviors: `BEVv` > `BEHv` with `BETi`
(behavior type: 9 and 2 seen), `BEEi` (event), `CDTs` (creation date), `VERi`, `TMOi`, and the
pop-up menu settings `XSLf YSLf RELb CSPi CPDi OPQb HICi CSHi BOCi BOSi OVWb OVHb MITi MIVb MIDi
MIWi MIHi HIMb RMOb PREb` (Custom Pop-Up, inTransit, tut2_final). Belongs to roadmap item 9 (web
layer, behaviors).

**Guides**: `GDCf` in `GDVv` (UIQ, 2 values): a guide field (colour or count?).

**Auto shapes (Fireworks 8+)**: `ISSv` in a group, with `FILs` (`Rounded Rectangle`), `JSVv` >
`JSSs` (the shape's **JavaScript source**), `CPSv` > `CPTv` (control points: `CTYi`, tooltip `TTPs`
such as "Click to switch corners", `TTDb`). One file (banner_graphic). FireCraft keeps the group
and its stored rendering and never runs the script; editing auto shapes would need it.
