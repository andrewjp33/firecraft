#!/bin/bash
# Cross-compile the FireCraft desktop app for Windows with the MSVC toolchain, from Linux, the
# way the release downloads are built (the release workflow builds them natively on Windows).
#
# One-time setup:
#   sudo apt install clang lld llvm      # clang-cl, lld-link, llvm-lib, llvm-rc
#   cargo install --locked cargo-xwin
#   rustup target add x86_64-pc-windows-msvc
# cargo-xwin downloads Microsoft's CRT and Windows SDK libraries on first use (accepting their
# licence) into ~/.cache/cargo-xwin.
#
# Usage: scripts/build-windows-msvc.sh [--release]
# Output: target/x86_64-pc-windows-msvc/{debug,release}/firecraft.exe
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo-xwin >/dev/null || { echo "cargo-xwin not found: cargo install --locked cargo-xwin" >&2; exit 1; }
rustup target list --installed | grep -q x86_64-pc-windows-msvc || rustup target add x86_64-pc-windows-msvc

# Distributions often install LLVM's tools with a version suffix only (clang-18, lld-link-18):
# give them their plain names here. clang-cl is clang run under that name.
bin=target/xwin-bin
mkdir -p "$bin"
link_tool() { # name, then the programs that can stand in for it
    local name=$1 p c
    shift
    command -v "$name" >/dev/null && return
    for p in "$@"; do
        for c in "$p" $(compgen -c "$p-" | grep -E "^$p-[0-9]+$" | sort -t- -k2 -rn); do
            if command -v "$c" >/dev/null; then
                ln -sfn "$(command -v "$c")" "$bin/$name"
                return
            fi
        done
    done
    echo "$name not found: sudo apt install clang lld llvm" >&2
    exit 1
}
link_tool clang-cl clang
link_tool lld-link lld-link
link_tool llvm-lib llvm-lib
link_tool llvm-rc llvm-rc
export PATH="$PWD/$bin:$PATH"

# libwebp-sys compiles its SSE4.1 files without -msse4.1 when the compiler looks like MSVC:
# Microsoft's cl.exe (the release workflow) accepts that, clang-cl needs the flag.
export CFLAGS="${CFLAGS:-} -msse4.1" # cargo-xwin adds these to its own target flags
cargo xwin build -p firecraft --target x86_64-pc-windows-msvc "$@"
profile=debug; [[ " $* " == *" --release "* ]] && profile=release
exe="target/x86_64-pc-windows-msvc/$profile/firecraft.exe"
ls -la "$exe"
