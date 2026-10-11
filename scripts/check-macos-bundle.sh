#!/usr/bin/env bash
# Fails unless a macOS application bundle will launch on a Mac other than the
# one that built it (release plan R1). `just bundle-macos` runs it before
# signing; run it by hand on any `.app`:
#
#   scripts/check-macos-bundle.sh "dist/ACP Inspector.app"
#
# Three checks on the main executable:
#
# - Every library it links is the system's (`/System/…` or `/usr/lib/…`).
#   A binary built in the devenv shell links Nix's libiconv from `/nix/store`,
#   which no other Mac has.
# - The `minos` the linker stamped (LC_BUILD_VERSION) is the
#   `LSMinimumSystemVersion` the Info.plist declares. A shell whose SDK targets
#   a newer macOS than the plist says builds an app that the older macOS lists
#   as supported and then refuses to start.
# - Its code signature verifies (the linker's ad-hoc one, before signing).
#
# Bash 3.2 compatible: this runs under macOS's own /bin/bash.
set -euo pipefail

app="${1:?usage: check-macos-bundle.sh <path/to/App.app>}"
plist="$app/Contents/Info.plist"

if [ ! -f "$plist" ]; then
    echo "check-macos-bundle: $plist is missing." >&2
    exit 1
fi

executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"
binary="$app/Contents/MacOS/$executable"
if [ ! -f "$binary" ]; then
    echo "check-macos-bundle: $binary is missing." >&2
    exit 1
fi

failed=0

# `otool -L` prints the binary's path, then one indented line per library.
foreign="$(otool -L "$binary" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/System/|/usr/lib/)' || true)"
if [ -n "$foreign" ]; then
    echo "check-macos-bundle: $binary links libraries another Mac does not have:" >&2
    echo "$foreign" | sed 's/^/  /' >&2
    echo "  Build the bundle outside the devenv shell, with Xcode's Command Line Tools as the C toolchain (README, \"Releasing\")." >&2
    failed=1
fi

# LC_BUILD_VERSION carries `minos`; a binary targeting macOS older than 10.14
# carries LC_VERSION_MIN_MACOSX's `version` instead.
minos="$(otool -l "$binary" | awk '
    /cmd LC_BUILD_VERSION/ { build = 1 }
    /cmd LC_VERSION_MIN_MACOSX/ { legacy = 1 }
    build && $1 == "minos" { print $2; exit }
    legacy && $1 == "version" { print $2; exit }
')"
declared="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$plist" 2>/dev/null || true)"

# 11 and 11.0 and 11.0.0 are the same version.
normalize() { echo "$1" | sed -E 's/(\.0)+$//'; }

if [ -z "$minos" ]; then
    echo "check-macos-bundle: no minimum macOS version found in $binary." >&2
    failed=1
elif [ -z "$declared" ]; then
    echo "check-macos-bundle: $plist declares no LSMinimumSystemVersion." >&2
    failed=1
elif [ "$(normalize "$minos")" != "$(normalize "$declared")" ]; then
    echo "check-macos-bundle: the binary needs macOS $minos, but Info.plist declares $declared." >&2
    echo "  Build with MACOSX_DEPLOYMENT_TARGET=$declared (just bundle-macos sets it from crates/app/Dioxus.toml)," >&2
    echo "  outside the devenv shell, whose SDK sets its own deployment target." >&2
    failed=1
fi

# Apple Silicon kills an executable whose signature does not verify, and the
# linker's ad-hoc one is all an unsigned build has. Anything that rewrites the
# binary after linking (a strip, a patch) must leave it valid. Verified on a
# copy outside the bundle: in place, codesign checks it as the bundle's main
# executable and wants the sealed resources only a real signature adds.
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cp "$binary" "$scratch/executable"
if ! codesign --verify "$scratch/executable" 2>/dev/null; then
    echo "check-macos-bundle: $binary has no valid code signature (codesign --verify):" >&2
    codesign --verify --verbose=2 "$scratch/executable" 2>&1 | sed 's/^/  /' >&2 || true
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    exit 1
fi
echo "check-macos-bundle: $binary links only system libraries, targets macOS $minos as Info.plist declares, and its signature verifies."
