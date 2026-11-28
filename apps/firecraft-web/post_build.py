#!/usr/bin/env python3
"""Trunk post-build hook: FireCraft's two WebAssembly builds, and a page that loads either.

`make web` runs Trunk twice (see the Makefile):

* legacy.html, the build for older browsers (Safari 13, and 12.1), into target/web-legacy.
  Plain "MVP" WebAssembly (WEB_LEGACY_ENV). This hook checks the .wasm really is MVP-only, and
  rewrites the JavaScript wasm-bindgen generates (and the inline_js snippets) into syntax old
  browsers read: `?.` and `??` became usable only in Safari 13.1.

* index.html, the modern build and the page itself, into dist/web. This hook checks the .wasm
  uses no WebAssembly feature beyond MODERN_FEATURES (support.js tests the browser for exactly
  those), copies the legacy build into legacy/, and replaces Trunk's loader with one that
  loads the build support.js chose. (Trunk's loader also uses `await` at the top level of a
  module, which Safari has only from 15.) It drops the preload links, so older browsers don't
  download the modern .wasm too; the loader starts that download itself.

`make web-debug-single` runs Trunk once, for index.html only, with FIRECRAFT_WEB_SINGLE=1: this
hook then only checks the modern build's features and leaves Trunk's loader alone, and
single_file.py packs the result into one HTML file (no legacy build).

Needs Node.js for the legacy build (npx fetches a pinned esbuild the first time).
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ESBUILD = "esbuild@0.25.0"
# Oldest browsers the legacy build's JavaScript must parse in.
JS_TARGETS = "safari12,chrome64,firefox62,edge79"
# The modern build's WebAssembly features (wasm-opt's names). Keep in step with
# WASM_FEATURES in support.js.
MODERN_FEATURES = ["sign-ext", "nontrapping-float-to-int", "bulk-memory", "multivalue",
                   "reference-types", "mutable-globals"]
# Where `make web` puts the legacy build (relative to this folder); FIRECRAFT_WEB_LEGACY overrides.
LEGACY_DIST = "../../target/web-legacy"

# Trunk's loader: import the glue, then `const wasm = await init(...)` and the rest.
TRUNK_LOADER = re.compile(
    r"<script type=\"module\">\s*import init, \* as bindings from '\./([^']+)';\s*"
    r"const wasm = await init\(\{ module_or_path: '\./([^']+)' \}\);(.*?)</script>",
    re.S)


def fail(message: str) -> None:
    print(f"post_build.py: {message}", file=sys.stderr)
    sys.exit(1)


def find_wasm_opt() -> str | None:
    found = shutil.which("wasm-opt")
    if found:
        return found
    caches = [Path.home() / ".cache" / "trunk", Path.home() / "Library" / "Caches" / "trunk"]
    if os.environ.get("LOCALAPPDATA"):
        caches.append(Path(os.environ["LOCALAPPDATA"]) / "trunk")
    for cache in caches:
        hits = sorted(cache.glob("wasm-opt-*/bin/wasm-opt*"), reverse=True)
        if hits:
            return str(hits[0])
    return None


def without_custom_section(wasm: bytes, name: str) -> bytes:
    """`wasm` without the custom section called `name`."""

    def leb(i: int) -> tuple[int, int]:
        value = shift = 0
        while True:
            byte = wasm[i]
            i += 1
            value |= (byte & 0x7F) << shift
            shift += 7
            if byte < 0x80:
                return value, i

    out = bytearray(wasm[:8])
    i = 8
    while i < len(wasm):
        start = i
        section = wasm[i]
        size, i = leb(i + 1)
        end = i + size
        if section == 0:
            length, j = leb(i)
            if wasm[j:j + length] == name.encode():
                i = end
                continue
        out += wasm[start:end]
        i = end
    return bytes(out)


def check_features(wasm: Path, features: list[str], fix: str) -> None:
    """Fail unless `wasm` validates with only MVP plus `features` enabled."""
    wasm_opt = find_wasm_opt()
    if not wasm_opt:
        print("post_build.py: warning: wasm-opt not found, so the WebAssembly features weren't checked",
              file=sys.stderr)
        return
    # Validation only (no passes, no output), of a copy without the target_features section:
    # wasm-opt would otherwise allow whatever features that lists.
    with tempfile.TemporaryDirectory() as tmp:
        copy = Path(tmp) / wasm.name
        copy.write_bytes(without_custom_section(wasm.read_bytes(), "target_features"))
        cmd = [wasm_opt, "--mvp-features"] + [f"--enable-{f}" for f in features] + [str(copy)]
        result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        detail = "\n".join((result.stderr or result.stdout).strip().splitlines()[:6])
        fail(f"{wasm.name} uses WebAssembly features beyond {features or 'MVP'}. {fix}\n{detail}")


def transpile(staging: Path, files: list[Path]) -> None:
    npx = shutil.which("npx")
    if not npx:
        fail("npx (Node.js) is needed to make the JavaScript readable by older browsers")
    cmd = [npx, "--yes", ESBUILD, f"--target={JS_TARGETS}", "--log-level=warning",
           "--log-override:duplicate-object-key=silent",  # wasm-bindgen repeats one, harmlessly
           "--allow-overwrite", f"--outbase={staging}", f"--outdir={staging}"]
    cmd += [str(f) for f in files]
    if subprocess.run(cmd).returncode != 0:
        fail("esbuild couldn't rewrite the JavaScript for older browsers")


def trunk_loader(html: str) -> re.Match:
    loader = TRUNK_LOADER.search(html)
    if not loader:
        fail("Trunk's loader script isn't the expected shape (has Trunk changed?); update post_build.py")
    return loader


def only_wasm(folder: Path) -> Path:
    wasms = list(folder.glob("*.wasm"))
    if len(wasms) != 1:
        fail(f"expected one .wasm in {folder}, found {len(wasms)}")
    return wasms[0]


def legacy(staging: Path) -> None:
    check_features(only_wasm(staging), [], "Build with `make web` (or `make web-debug` / `make web-serve`), "
                   "which sets the flags for it (WEB_LEGACY_ENV).")
    # Trunk writes every page as index.html.
    glue, _, _ = trunk_loader((staging / "index.html").read_text()).groups()
    transpile(staging, [staging / glue] + sorted((staging / "snippets").rglob("*.js")))


def modern(staging: Path, source: Path) -> None:
    check_features(only_wasm(staging), MODERN_FEATURES,
                   "Has Rust started using another one by default? Add it to MODERN_FEATURES here "
                   "and to WASM_FEATURES in support.js.")
    if os.environ.get("FIRECRAFT_WEB_SINGLE") == "1":
        return  # single_file.py takes it from here

    built = (source / os.environ.get("FIRECRAFT_WEB_LEGACY", LEGACY_DIST)).resolve()
    if not (built / "index.html").is_file():
        fail(f"no legacy build in {built}: build with `make web` (or web-debug / web-serve), "
             "which makes it first")
    legacy_glue, legacy_wasm, _ = trunk_loader((built / "index.html").read_text()).groups()
    out = staging / "legacy"
    out.mkdir()
    for name in (legacy_glue, legacy_wasm):
        shutil.copy2(built / name, out / name)
    shutil.copytree(built / "snippets", out / "snippets")

    html_path = staging / "index.html"
    html = html_path.read_text()
    loader = trunk_loader(html)
    glue, wasm, rest = loader.groups()
    new_loader = f"""<script type="module">
// The build support.js chose (modern, or legacy/ for older browsers).
const legacy = window.firecraftWasmBuild === "legacy";
const dir = legacy ? "./legacy/" : "./";
const wasmFile = fetch(dir + (legacy ? "{legacy_wasm}" : "{wasm}"));
import(dir + (legacy ? "{legacy_glue}" : "{glue}"))
  .then((bindings) => bindings.default({{ module_or_path: wasmFile }}).then((wasm) => {{{rest.rstrip()}
}}))
  .catch((e) => {{ if (window.firecraftFailed) window.firecraftFailed(e); else throw e; }});
</script>"""
    html = html[:loader.start()] + new_loader + html[loader.end():]
    # Preloads of the modern build's files (older browsers would fetch them for nothing).
    html, count = re.subn(r'<link rel="(?:modulepreload|preload)" href="\./[^"]*"[^>]*>', "", html)
    if count == 0:
        print("post_build.py: warning: no preload links found to remove", file=sys.stderr)
    html_path.write_text(html)


def main() -> None:
    staging = Path(os.environ.get("TRUNK_STAGING_DIR", ""))
    if not staging.is_dir():
        fail("run by Trunk (TRUNK_STAGING_DIR isn't set)")
    page = Path(os.environ.get("TRUNK_HTML_FILE", "index.html")).name
    if page == "legacy.html":
        legacy(staging)
    elif page == "index.html":
        modern(staging, Path(__file__).resolve().parent)
    else:
        fail(f"unexpected page {page}")


if __name__ == "__main__":
    main()
