#!/usr/bin/env python3
"""Packs a Trunk build of FireCraft into one HTML file that opens straight from disk.

    single_file.py <trunk dist folder> <output .html>

`make web-debug-single` runs it on the debug build Trunk made with FIRECRAFT_WEB_SINGLE=1
(modern build only; post_build.py leaves Trunk's loader alone then). A page opened from disk
(file://) can't fetch or import other files, so everything goes inside the page:

* support.js, as a plain inline script (with `firecraftSingleFile` set before it, so it
  reports a browser without the modern build's WebAssembly features as missing them rather
  than choosing the legacy build, which this file doesn't have).
* wasm-bindgen's JavaScript and its inline_js snippets, bundled by esbuild into one inline
  module script, with a loader in place of Trunk's.
* The .wasm, gzipped and then base64, in a script element of a type browsers don't run. The
  loader decodes it and unpacks it with DecompressionStream (Chrome 80, Firefox 113, Safari
  16.4). Gzip takes the debug build from ~150 MB of base64 to ~40 MB.

Needs Node.js (npx fetches the pinned esbuild post_build.py uses the first time).
"""

import base64
import gzip
import re
import shutil
import subprocess
import sys
from pathlib import Path

sys.dont_write_bytecode = True  # no __pycache__ beside the sources
from post_build import ESBUILD, TRUNK_LOADER, only_wasm

SUPPORT = re.compile(r'<script src="\./(support-[^"]+\.js)"[^>]*></script>')
PRELOADS = re.compile(r'<link rel="(?:modulepreload|preload)" href="\./[^"]*"[^>]*>')
DATA_ID = "firecraft_wasm"


def fail(message: str) -> None:
    print(f"single_file.py: {message}", file=sys.stderr)
    sys.exit(1)


def inline_safe(js: str, what: str) -> str:
    """`js`, safe to put between <script> and </script>."""
    # Only ever inside strings, regular expressions or comments, where `<\/` means the same.
    js = re.sub(r"</(script)", r"<\\/\1", js, flags=re.I)
    if "<!--" in js:
        fail(f"{what} contains `<!--`, which would upset the HTML parser inside a script element")
    return js


def bundle(build: Path, glue: str, rest: str) -> str:
    """wasm-bindgen's glue and snippets, and the loader, as one ES module."""
    entry = build / "single-entry.js"
    entry.write_text(f"""import init, * as bindings from "./{glue}";

// The .wasm is in the page (gzipped, base64): see single_file.py.
function wasmSource() {{
  if (typeof DecompressionStream !== "function") {{
    throw new Error("This one-file build of FireCraft needs a browser with DecompressionStream " +
      "(Chrome 80, Firefox 113, Safari 16.4 or newer). The served web build works in older ones.");
  }}
  const data = document.getElementById("{DATA_ID}");
  const text = data.textContent;
  data.remove();
  const binary = atob(text.trim());
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  const unpacked = new Response(bytes).body.pipeThrough(new DecompressionStream("gzip"));
  return new Response(unpacked, {{ headers: {{ "Content-Type": "application/wasm" }} }});
}}

Promise.resolve()
  .then(() => init({{ module_or_path: wasmSource() }}))
  .then((wasm) => {{{rest.rstrip()}
}})
  .catch((e) => {{ if (window.firecraftFailed) window.firecraftFailed(e); else throw e; }});
""")
    npx = shutil.which("npx")
    if not npx:
        fail("npx (Node.js) is needed to bundle the JavaScript")
    cmd = [npx, "--yes", ESBUILD, str(entry), "--bundle", "--format=esm", "--log-level=warning",
           "--log-override:duplicate-object-key=silent"]  # wasm-bindgen repeats one, harmlessly
    result = subprocess.run(cmd, capture_output=True, text=True)
    entry.unlink()
    if result.returncode != 0:
        fail(f"esbuild couldn't bundle the JavaScript:\n{result.stderr}")
    return result.stdout


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: single_file.py <trunk dist folder> <output .html>")
    build, out = Path(sys.argv[1]), Path(sys.argv[2])
    html = (build / "index.html").read_text()

    if (build / "legacy").exists():
        fail(f"{build} has a legacy/ folder: build it with `make web-debug-single`")
    loader = TRUNK_LOADER.search(html)
    if not loader:
        fail("Trunk's loader script isn't the expected shape (has Trunk changed?); update post_build.py")
    glue, wasm_name, rest = loader.groups()
    wasm = build / wasm_name
    if wasm != only_wasm(build):
        fail(f"the page loads {wasm_name}, which isn't the build's one .wasm")

    module = inline_safe(bundle(build, glue, rest), "the bundled JavaScript")
    html = html[:loader.start()] + f'<script type="module">\n{module}</script>' + html[loader.end():]

    support = SUPPORT.search(html)
    if not support:
        fail("no support.js script in the page")
    support_js = inline_safe((build / support.group(1)).read_text(), "support.js")
    html = (html[:support.start()] + "<script>window.firecraftSingleFile = true;</script>\n  "
            + f"<script>\n{support_js}</script>" + html[support.end():])

    html, count = PRELOADS.subn("", html)
    if count == 0:
        print("single_file.py: warning: no preload links found to remove", file=sys.stderr)
    if re.search(r'(?:src|href)="\./', html):
        fail("the page still refers to another file: " + re.search(r'(?:src|href)="\./[^"]*"', html).group(0))

    packed = base64.b64encode(gzip.compress(wasm.read_bytes(), compresslevel=9, mtime=0)).decode()
    data = f'<script type="application/octet-stream" id="{DATA_ID}">{packed}</script>\n'
    if html.count("</body>") != 1:
        fail("expected one </body> in the page")
    html = html.replace("</body>", data + "</body>")

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(html)
    print(f"Built: {out} ({out.stat().st_size / 1e6:.1f} MB; opens from disk, no server needed)")


if __name__ == "__main__":
    main()
