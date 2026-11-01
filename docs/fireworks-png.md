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
LAYv  BKGb, URLb (web layer), LNMs name, MSLb (shared across frames), MCLb (current), DISb (expanded),
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

Fireworks MX renders FireCraft-written documents identically except 1-px anti-aliased strokes,
which it draws thinner (stroke rendering parity is on the roadmap).

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
   understand them (pages, persistent 9-slice symbols as CS edits them). FirePixel's reader/writer
   (Apache-2.0) is the reference for that structure.
