#!/bin/bash
# Cross-compile the FireCraft desktop app for Windows (x86_64) from Linux.
#
# One-time setup:
#   sudo apt install mingw-w64            # provides x86_64-w64-mingw32-gcc (the linker)
#   rustup target add x86_64-pc-windows-gnu
#
# Usage: scripts/build-windows.sh [--release]
# Output: target/x86_64-pc-windows-gnu/{debug,release}/firecraft.exe
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v x86_64-w64-mingw32-gcc >/dev/null; then
    echo "x86_64-w64-mingw32-gcc not found: run 'sudo apt install mingw-w64' first" >&2
    exit 1
fi
rustup target list --installed | grep -q x86_64-pc-windows-gnu || rustup target add x86_64-pc-windows-gnu
cargo build -p firecraft --target x86_64-pc-windows-gnu "$@"
profile=debug; [[ " $* " == *" --release "* ]] && profile=release
exe="target/x86_64-pc-windows-gnu/$profile/firecraft.exe"
ls -la "$exe"
echo "Copy $exe to a Windows PC and run it (no other files needed)."
