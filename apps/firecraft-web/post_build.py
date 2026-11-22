#!/usr/bin/env python3
"""Trunk post-build hook: make the browser build load in older browsers (Safari 13, and 12.1).

Trunk runs this on the staged build, before it's copied to dist/web. It

1. checks the WebAssembly uses no feature newer than the original "MVP" (the build settings for
   that are WEB_ENV in the Makefile, so a plain `trunk build` fails here: use `make web`);
2. rewrites the JavaScript that wasm-bindgen generates (and the inline_js snippets) into syntax
   old browsers read: `?.` and `??` became usable only in Safari 13.1;
3. replaces Trunk's loader, which uses `await` at the top level of a module (Safari 15), with
   a promise, and reports a failed load to support.js's message;
4. updates the integrity hashes of the files it changed.

Needs Node.js (npx fetches a pinned esbuild the first time).
"""

import base64
import glob
import hashlib
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ESBUILD = "esbuild@0.25.0"
# Oldest browsers the generated JavaScript must parse in.
JS_TARGETS = "safari12,chrome64,firefox62,edge79"


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
        hits = sorted(glob.glob(str(cache / "wasm-opt-*" / "bin" / "wasm-opt*")), reverse=True)
        if hits:
            return hits[0]
    return None


def check_mvp(wasm: Path) -> None:
    wasm_opt = find_wasm_opt()
    if not wasm_opt:
        print("post_build.py: warning: wasm-opt not found, so the WebAssembly features weren't checked",
              file=sys.stderr)
        return
    # Validation only (no passes, no output) with nothing beyond MVP enabled.
    result = subprocess.run([wasm_opt, "--mvp-features", str(wasm)], capture_output=True, text=True)
    if result.returncode != 0:
        detail = "\n".join((result.stderr or result.stdout).strip().splitlines()[:6])
        fail(f"{wasm.name} uses WebAssembly features older browsers lack. Build with `make web` "
             f"(or `make web-debug` / `make web-serve`), which sets the flags for that.\n{detail}")


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


def sri(path: Path) -> str:
    return "sha384-" + base64.b64encode(hashlib.sha384(path.read_bytes()).digest()).decode()


def main() -> None:
    staging = Path(os.environ.get("TRUNK_STAGING_DIR", ""))
    if not staging.is_dir():
        fail("run by Trunk (TRUNK_STAGING_DIR isn't set)")
    html_path = staging / "index.html"
    html = html_path.read_text()

    # Trunk's loader: import the glue, then `const wasm = await init(...)` and the rest.
    loader = re.search(
        r"<script type=\"module\">\s*(import init, \* as bindings from '\./([^']+)';)\s*"
        r"const wasm = await init\((.*?)\);(.*?)</script>",
        html, re.S)
    if not loader:
        fail("Trunk's loader script isn't the expected shape (has Trunk changed?); update post_build.py")
    import_line, glue_name, init_args, rest = loader.groups()

    wasms = list(staging.glob("*.wasm"))
    if len(wasms) != 1:
        fail(f"expected one .wasm in {staging}, found {len(wasms)}")
    check_mvp(wasms[0])

    glue = staging / glue_name
    js = [glue] + sorted((staging / "snippets").rglob("*.js"))
    transpile(staging, js)

    new_loader = (
        "<script type=\"module\">\n"
        f"{import_line}\n"
        f"init({init_args}).then((wasm) => {{{rest.rstrip()}\n"
        "}, (e) => { if (window.firecraftFailed) window.firecraftFailed(e); else throw e; });\n"
        "</script>")
    html = html[:loader.start()] + new_loader + html[loader.end():]

    # Fresh integrity hashes for the rewritten files.
    for f in js:
        href = "./" + f.relative_to(staging).as_posix()
        pattern = re.compile(r'(href="' + re.escape(href) + r'"[^>]*?integrity=")[^"]*(")')
        html, count = pattern.subn(lambda m: m.group(1) + sri(f) + m.group(2), html)
        if count == 0 and "integrity=" in html:
            print(f"post_build.py: warning: no integrity hash found for {href}", file=sys.stderr)

    html_path.write_text(html)


if __name__ == "__main__":
    main()
