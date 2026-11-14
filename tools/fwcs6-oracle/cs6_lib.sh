# Shared by the Fireworks CS6 oracle scripts (source it; needs HERE set to this directory).
: "${CS6_PREFIX:?set CS6_PREFIX to the 32-bit Wine prefix with Fireworks CS6 installed}"
export WINEPREFIX=$CS6_PREFIX WINEARCH=win32 WINEDEBUG=${WINEDEBUG:--all}
CS6_DIR="$CS6_PREFIX/drive_c/Program Files/Adobe/Adobe Fireworks CS6"
CS6_HELPER="$HERE/../../target/cs6helper.exe"

# Build cs6helper.exe (input-attach fix, Missing Fonts answering) when missing or out of date.
cs6_build_helper() {
    if [ ! -f "$CS6_HELPER" ] || [ "$HERE/cs6helper.c" -nt "$CS6_HELPER" ]; then
        mkdir -p "$(dirname "$CS6_HELPER")"
        i686-w64-mingw32-gcc -O2 -s -o "$CS6_HELPER" "$HERE/cs6helper.c" || return 1
    fi
}

# True when Fireworks CS6 already runs in this prefix (a second start would hand our files and
# scripts to that instance, wherever it is displayed).
cs6_running() {
    local p
    for p in $(pgrep -f 'Fireworks\.exe'); do
        tr '\0' '\n' <"/proc/$p/environ" 2>/dev/null | grep -qx "WINEPREFIX=$CS6_PREFIX" && return 0
    done
    return 1
}
