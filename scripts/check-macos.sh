#!/bin/bash
# Type-check the macOS app (Apple silicon and Intel) without a Mac: catches code that doesn't
# compile for macOS while working on Linux. Nothing is linked: that needs Apple's SDK, so the
# app itself is built on a Mac (scripts/build-macos.sh, or the release workflow).
#
# The C libraries (libwebp, libdeflate) are compiled with this machine's C compiler instead of
# for macOS; `cargo check` never uses their objects.
set -euo pipefail
cd "$(dirname "$0")/.."
targets=(aarch64-apple-darwin x86_64-apple-darwin)
installed=$(rustup target list --installed 2>/dev/null || true)
for t in "${targets[@]}"; do
    grep -qx "$t" <<<"$installed" || rustup target add "$t"
done
if [ "$(uname -s)" = Darwin ]; then
    for t in "${targets[@]}"; do cargo check -p firecraft --target "$t"; done
    exit
fi

dir=$PWD/target/macos-check
mkdir -p "$dir"
cat >"$dir/cc" <<'EOF'
#!/bin/bash
# A stand-in C compiler for scripts/check-macos.sh: compiles for this machine, leaving out
# Apple-only options (the objects are never linked).
args=()
skip=0
for a in "$@"; do
    if [ $skip = 1 ]; then skip=0; continue; fi
    case $a in
        -arch) skip=1 ;;
        -mmacosx-version-min=* | --target=* | -mcpu=*) ;;
        *) args+=("$a") ;;
    esac
done
exec "${HOST_CC:-cc}" "${args[@]}"
EOF
chmod +x "$dir/cc"
for t in "${targets[@]}"; do
    env "CC_${t//-/_}=$dir/cc" cargo check -p firecraft --target "$t"
done
echo "macOS (arm64, x86_64): compiles"
