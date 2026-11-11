# Vendored crates

Unmodified copies of crates.io releases except where listed. The workspace `Cargo.toml` uses them
through `[patch.crates-io]`. Each folder keeps its own licence files.

## jxl-oxide 0.12.6 and jxl-jbr 0.2.1

- Source: https://github.com/tirr-c/jxl-oxide (`jxl-oxide` at commit 74c45ba, `jxl-jbr` at
  f37ac03; see each `.cargo_vcs_info.json`)
- Licence: MIT OR Apache-2.0
- Change: `Cargo.toml` requires `brotli-decompressor` 6.3.1 instead of 5.0.0. Nothing else.

Why: `brotli-decompressor` before 6.2.0 has the block-switch bug of CVE-2026-111497
("Broccoli2Shell"), and there is no fixed 5.x. JPEG XL files reach it when FireCraft opens them:
Brotli-compressed metadata boxes (`brob`) and JPEG reconstruction data (`jbrd`) are decompressed
while the file is read. Both crates use only `DecompressorWriter` (`new`, `write_all`, `flush`,
`close`, `get_ref`, `into_inner`), which 6.x keeps unchanged. Tests: the JPEG XL tests in
`crates/io/src/formats.rs`.

To check the copies: `diff -r` against `~/.cargo/registry/src/*/jxl-oxide-0.12.6` (and
`jxl-jbr-0.2.1`) shows only that line.

Remove both folders and the `[patch.crates-io]` entries once a jxl-oxide release requires
brotli-decompressor 6.2 or later.
