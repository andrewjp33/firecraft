# Fireworks PNG format notes

Derived black-box from files written by Fireworks MX (its shipped libraries/styles and documents it
saved under the oracle harness), then confirmed by having Fireworks MX open files FireCraft wrote.
Implementation: `crates/fwpng` (container) and `crates/io` (document mapping).

## Container

A normal PNG whose `IDAT` is the flattened canvas (32-bit RGBA), so every viewer shows it, plus:

| chunk | content |
|---|---|
| `mkBF` | 72 bytes: `FADECAFE 00000004` then zeros |
| `mkTS` | zlib: the document tree (below) |
| `mkBS` | zlib: a small secondary tree (one layer, one 256-wide image); purpose unknown, optional |
| `mkBT` | one pixel tile each |
| `prVW` | zlib: preview, magic `CAFEBEEF`; optional |

Fireworks MX also writes `sBIT` and `tEXt Software=Macromedia Fireworks MX`; neither is required.

## Tree grammar

`TAG KIND { body }` with a 3-byte ASCII tag and kind `v` (vector of nodes), `i` (hex integer,
32-bit two's complement), `f` (decimal), `b` (`0`/`1`), `s` (u16 big-endian length in UTF-16 units,
then UTF-16BE; may contain `}`). Every observed file round-trips byte for byte through FireCraft's
parser/serializer (8 trees, 258 distinct tags).

## Tiles (`mkBT`)

`FACECAFE`, u32 id (referenced by `TIDi`), u32 flags (0/1, preserved), 64 zero bytes, zlib payload.
Image tiles (`TSZi{80}`, 128 px) inflate to 128×128 **A,R,G,B** straight alpha; mask tiles
(`TSZi{100}`) to 256×256 bytes. Tiles cover an element row-major in `TILv` order.

## Document

```text
MKBv  FRCi frames, RQNi ?, XLCf/YLCf canvas origin, WIDf/HITf size, RESf, RSUi (1 = per cm),
      BGCi canvas ARGB (alpha 0 = transparent),
      [PLLv brushes, FLDv fills, GLDv gradients, EFDv effects, MSLv masks/textures, PATv, EXPv export,
       MNFv]  preset lists: optional
      LYLv  LAYv*  (bottom layer first)
      GRDv grid, GDSv guides, OSBi, OSAi
LAYv  BKGb, URLb (web layer), LNMs name, MSLb (unknown; 1 on the Web Layer), MCLb (1 = own content per
      frame, 0 = shared across frames: Fireworks then shows cell 0 in every frame), DISb (expanded),
      CLLv  CELv* (one per frame)  { LCKb, ELMv elements }, VIFv { VISb }
IMGv  XOFf YOFf XLCf YLCf (document space), WPXi HPXi, TSZi, TILv { TIDi* }
      pixel buffer/tile grid at (XLC, YLC) sized WPX x HPX; visible image rect is
      (XLC+XOF, YLC+YOF)..(XLC+WPX, YLC+HPX): XOF/YOF hide the buffer's top-left strip
      (read from the bitmap constructor 0x62acd0 in Fireworks.exe; reader at 0x7b4ff9)
PTHv  PATv { BCLi stroke ARGB, FCLi fill ARGB, BPLv brush preset, FPLv fill preset, TXBv/TXFv textures },
      BRPi, PTTi, FOTb, RNDi, TOXf.., PSXf..PFYf (gradient handles), EFLv effects,
      PBLv { PBPv* { ISCb closed, BSLf, PPCi, PBTv* { XLCf YLCf [XPCf YPCf] [XSCf YSCf] ... } } },
      PCLv (cached flattened stroke polyline with pressure)
TXTv  text attributes (FONs, PTSf, BOLb...), TFSv runs, cached IMGv
GRPv  GRTi, ELMv
(any) OBNs object name (Layers panel), after the object's own data, before BLDi/EFLv/OPAi;
      VISb (first child) hidden; BLDi blend mode; OPAi opacity in tenths of a percent
      LCKb locked (Fireworks CS's per-object lock, after VISb; CS writes it on every object,
      some MX objects carry LCKb 0; MX ignores it). FireCraft writes it when set or as read.
MSLv  (root) MSTv* symbol definitions: UNMs name, MSNi id, MSYi type (absent Graphic, 1 Button
      with FRCi 4 states, 2 Animation), MOXf/MOYf, LUTs, own LYLv (Layer 1 + Web Layer), GRDv,
      GDSv, FRCi frames, OSBi, OSAi, BGCi, ATSb 1, IDSb, DVLb, IODb; LFNs/LMNi when linked to a
      library file. Convert to Symbol centres the content on the symbol origin (a 50x40
      rectangle at (10,10) becomes -25..25 x -20..20; the instance matrix translates by (35,30))
ALSv  symbol instance: MSNi (symbol id), ALNi (buttons), MTXv (M00 M01 M10 M11 M20 M21: symbol
      space -> document space, row-vector convention), XFMi 0, ANNi (= the symbol's frame
      count for new instances), ANXf 0, ANYf 0, ASCf 100 (not yet decoded further)
```

Bezier control points (`XPC/YPC` predecessor, `XSC/YSC` successor) are **absolute** coordinates and
are omitted when they coincide with the anchor.

## What Fireworks MX requires (bisected with `tools/fwmx-oracle/fw_inspect.sh`)

* Every tile id referenced anywhere in the trees must exist, or Fireworks silently drops content
  (a whole layer, in one test). FireCraft therefore carries over every original tile still
  referenced by preserved nodes.
* A path needs **`RNDi`** (per-object id) and **both `BCLi` and `FCLi`** in `PATv`, even with no
  stroke or no fill. Without them the file fails to open (error `0x8004142B`).
* Not required: preset lists, `prVW`, `mkBS`, `sBIT`, `tEXt`, `PCLv`, textures, gradient handles,
  `BZLv`/`NSDf` on points, `RQNi`.

## Shape primitives (PRIv)

A rectangle drawn with Fireworks' Rectangle tool is a *RectanglePrimitive*: its `PTHv` has
`PTTi 1`, `TOXf`/`TOYf`, `PCFb`, `PSXf..PFYf` after `RNDi`, and after the points a
`PRIv { PCFb, PSXf..PFYf, MTXv (row vectors: M20/M21 translation), RCTv { RDSf roundness,
LFTf, TOPf, RITf, BOTf base box } }` (CS adds `RDMb`). Fireworks MX and CS rebuild the shape
from `RCTv` and `MTXv`, not from the path points (verified with tools/fwmx-oracle: moved
points with the old record reopen at the old place).

Roundness `RDSf` (0..1) is the corner radius as a fraction of half the shorter side (hd.png:
0.3 on a 209x123 box gives radius 18.45). The outline: 4 corners clockwise from top left, or
with rounding 8 points from the left end of the top edge, each corner a quarter circle with
handles 0.5523 of the radius.

FireCraft keeps the record as `Path::rect` (`RectShape`: base box, roundness, matrix). Moves
and affine transforms update its matrix; editing points makes the path plain (no `PRIv`). On
read and write, a record that doesn't match the points (files saved by FireCraft before it
kept primitives) is fitted to them by an affine transform, or dropped. FireCraft's Rectangle
tool draws primitives, which Fireworks MX opens as RectanglePrimitive objects (verified:
sharp, rounded, and rotated rounded, rendering the same as FireCraft).

## Fireworks CS files

A Fireworks CS5 tree holds two documents: `MKBv` (the classic one, which Fireworks MX reads;
in CS files its root also has `VRSi`/`VRMi` version fields and `PIDs`, `PILb`, `ILLb`, `IMPb`,
`LSMv`, `ZSLv`), followed by a second top-level node, `PDCv` (`VRSi`, `VRMi`, then the whole
document again, CS's own copy). FireCraft reads `MKBv` and writes only it; it removes the
CS-only root fields, since with them CS expects the `PDCv` and opens an empty canvas (seen on
a re-saved CS5 file). Without them CS reads the file like one from MX.

9-slice guides (`NGSv` in a symbol, see `docs/behavior/symbols.md`) are read by CS. CS never
skews or distorts a 9-slice instance (it asks to remove the slices) and hangs opening a file
that has one; for a symbol with such an instance FireCraft writes the guides in its own node
`FNGv` (same `NGCf` values) instead, which MX and CS ignore, so CS shows it unsliced.

## Clipboard

Fireworks MX copies objects as a clipboard format named **"PNG"** holding a complete Fireworks
PNG cropped to the copied objects, with the root `XLCf`/`YLCf` set to their original top-left
(also "Fireworks Internal Clipboard Format 3.0", CF_DIB/CF_DIBV5 and OLE wrappers). It pastes
"PNG" back as editable objects, including FireCraft-written ones. Under Wine the format is the
X11 target `image/png` (with Wine's `explorer` running, as in a desktop session); see
`docs/behavior/clipboard.md`.

Fireworks MX renders FireCraft-written documents the same as FireCraft, except stroke softness
(see `docs/behavior/strokes.md`).

## Compatibility levels (design)

Fireworks MX opens files from later versions only when they keep the MX structure; CS-era files
(pages `PDCv`, `VRSi`/`VRMi` format version, CS frame/state layout) generally don't open in MX,
while MX files open fine in CS versions. Measured with the oracle (`fw_inspect.sh`):

* MX **ignores unknown nodes** anywhere in the tree (root vectors and scalars, inside elements,
  brand-new tags) and opens the file normally.
* MX **drops unknown nodes when it re-saves** a file.

So FireCraft saves in tiers:

1. **Fireworks MX (default).** MX structure. Features MX lacks are either *baked* into ordinary
   objects (e.g. the 9-slice Scaling tool reshapes paths/bitmaps directly) or stored as extra nodes
   MX ignores, using the CS tag names where CS defines them (e.g. 9-slice guides on a symbol as
   `NGSv{NGLb,NGKi,NGCf x4 (top, bottom, left, right),NGMv,NGVv}`). The file opens in MX and CS;
   editing and re-saving in MX loses only the extras.
2. **Fireworks CS6 (planned, opt-in on Save As/Export).** For documents whose features need CS6 to
   understand them (pages, CS states). FirePixel's reader/writer (Apache-2.0) is the reference for
   that structure.

Fireworks CS5 opens FireCraft's MX-format files, including CS features stored the CS way
(9-slice symbol guides, per-object locks), and FireCraft opens CS5's files (it reads `MKBv`; see
"Fireworks CS files" above). Checked by hand in CS5.

## FireCraft extensions

Stored as extra nodes in the root `mkTS` tree. Fireworks MX opens such files normally, ignores the
nodes, and drops them if it re-saves the file.

| Node | Meaning |
|---|---|
| `FCMi` | Colour math: `1` = high precision (double-precision blend modes and Hue/Saturation). Absent = Fireworks-exact. |
| `FNGv` (in `MSTv`) | 9-slice guides of a symbol that has a skewed or perspective instance, in place of `NGSv` (same `NGCf` values), because such an instance hangs Fireworks CS5. CS and MX ignore it; FireCraft reads either. |

