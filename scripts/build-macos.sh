#!/bin/bash
# Build FireCraft.app for macOS. Runs on a Mac (Apple's linker and SDK are needed): install the
# Xcode command line tools once with `xcode-select --install`. The Rust targets are added on
# first use.
#
# Usage: scripts/build-macos.sh [--release] [--arch universal|arm64|x86_64]
#   --arch      universal: one app for Apple silicon and Intel Macs (both builds joined with
#               lipo). Default: universal for --release, else this Mac's own (quicker to build).
# Output: dist/macos/FireCraft.app (debug) or dist/macos-release/FireCraft.app.
#
# Signing (every Mac app needs a signature; Apple silicon Macs refuse to run code without one):
# - By default the app is signed "ad hoc" (no certificate). It runs, but a downloaded copy is
#   held back by Gatekeeper the first time: System Settings > Privacy & Security > Open Anyway.
# - With MACOS_SIGN_IDENTITY set to a Developer ID certificate in the keychain (e.g.
#   "Developer ID Application: Name (TEAMID)", see `security find-identity -v -p codesigning`)
#   it's signed with that and the hardened runtime, as notarization requires.
#   scripts/package-release.sh macos then notarizes the disk image.
set -euo pipefail
cd "$(dirname "$0")/.."

profile=debug
arch=
while [ $# -gt 0 ]; do
    case $1 in
        --release) profile=release ;;
        --arch) arch=${2:?--arch universal, arm64 or x86_64}; shift ;;
        --arch=*) arch=${1#--arch=} ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done
[ "$(uname -s)" = Darwin ] || { echo "The macOS build runs on a Mac (Apple's linker and SDK)." >&2; exit 1; }
if [ -z "$arch" ]; then
    if [ $profile = release ]; then arch=universal; else arch=$(uname -m); fi
fi
case $arch in
    universal) targets=(aarch64-apple-darwin x86_64-apple-darwin) ;;
    arm64 | aarch64) targets=(aarch64-apple-darwin) ;;
    x86_64) targets=(x86_64-apple-darwin) ;;
    *) echo "unknown --arch: $arch" >&2; exit 1 ;;
esac

# Oldest macOS each build runs on. Apple silicon started with macOS 11; Intel Macs go back to
# 10.13 High Sierra, the oldest that Metal (wgpu) supports. FireCraft's users often keep old
# machines, so don't raise this without need.
MIN_MACOS_ARM=11.0
MIN_MACOS_INTEL=10.13
min_macos=$MIN_MACOS_ARM
for t in "${targets[@]}"; do
    if [ "$t" = x86_64-apple-darwin ]; then min_macos=$MIN_MACOS_INTEL; fi
done

# A plain string, not an array: macOS's /bin/bash is version 3.2, where an empty array is an
# "unbound variable" under set -u.
flag=
if [ $profile = release ]; then flag=--release; fi
installed=$(rustup target list --installed 2>/dev/null || true)
bins=()
for t in "${targets[@]}"; do
    grep -qx "$t" <<<"$installed" || rustup target add "$t"
    case $t in
        aarch64-*) export MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS_ARM ;;
        x86_64-*) export MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS_INTEL ;;
    esac
    cargo build $flag -p firecraft --target "$t"
    bins+=("target/$t/$profile/firecraft")
done

# One executable holding every architecture (macOS runs the slice matching the Mac).
bin=target/macos-$arch/$profile/firecraft
mkdir -p "$(dirname "$bin")"
if [ ${#bins[@]} -gt 1 ]; then
    lipo -create -output "$bin" "${bins[@]}"
else
    cp "${bins[0]}" "$bin"
fi

# The version as cargo has it (e.g. 0.3.0); the release workflow sets FIRECRAFT_VERSION to the
# tag's. macOS wants plain numbers in CFBundleVersion, so 0.3.0-dev.abc1234 is 0.3.0 there.
version=${FIRECRAFT_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
bundle_version=${version%%[-+]*}

out=dist/macos
if [ $profile = release ]; then out=dist/macos-release; fi
app=$out/FireCraft.app
# A fresh bundle each time: codesign seals every file in it, and leftovers would be sealed too.
if [ -e "$app" ]; then rm -r "$app"; fi
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/firecraft"
sed -e "s/@VERSION@/$version/g" -e "s/@BUNDLE_VERSION@/$bundle_version/g" -e "s/@MIN_MACOS@/$min_macos/g" \
    apps/firecraft/macos/Info.plist >"$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist"
printf 'APPL????' >"$app/Contents/PkgInfo"
# The licence notices travel with the program (see ATTRIBUTION.md).
cp README.md LICENSE-MIT LICENSE-APACHE ATTRIBUTION.md "$app/Contents/Resources/"

if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    echo "Signing with $MACOS_SIGN_IDENTITY (hardened runtime)"
    codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$app"
else
    echo "Signing ad hoc (set MACOS_SIGN_IDENTITY to sign with a Developer ID)"
    codesign --force --sign - "$app"
fi
codesign --verify --strict --verbose=2 "$app"
echo "Architectures: $(lipo -archs "$app/Contents/MacOS/firecraft"); macOS $min_macos or later"
echo "Built: $app"
