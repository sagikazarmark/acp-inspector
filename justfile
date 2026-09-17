# The ACP Inspector's tasks. Run from the repository root.

# Build Testy and run every check.
default: testy test

# Run the app in its desktop shell.
#
# Plain `cargo run`, no `dx`: nothing is resolved as an asset and nothing is
# bundled, and keeping it that way is what makes the shell buildable by anyone
# with a Rust toolchain and the WebView libraries (see the README). The
# stylesheet is generated Tailwind/daisyUI output, committed to the tree and
# compiled in rather than resolved (docs/adr/0003-the-stylesheet-is-generated-and-committed.md),
# so Node changes the stylesheet and CI verifies it; it is not required to
# compile or run a valid checkout, and this recipe never calls it. The one
# recipe that does run `dx` is `bundle-macos`, and it makes an artifact out of
# this build rather than being part of it (docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md).
#
# The desktop renderer is the crate's default feature, so this is the plain
# form; `web` is the other feature and, until core builds for `wasm32`, a
# `cargo check` and not a `run` (docs/adr/0011-one-ui-crate-a-feature-per-renderer.md).
run:
    cargo run -p acp-inspector

# Prove the web build of the app crate still compiles.
#
# On the native target, because that is the half of the promise this checkout
# can keep today: every use of `dioxus::desktop` is behind the `desktop`
# feature, and the screens compile without it. That core builds for `wasm32`
# is the other half, and the web ring's to keep.
check-web:
    cargo check -p acp-inspector --no-default-features --features web

# Regenerate the window's stylesheet from its source.
#
# One of the two recipes that need Node — `check` verifies what this one
# writes — and the only thing it does is write `crates/app/style/sheet.css` from
# `crates/app/style/input.css`. Run it after changing the styles and commit what it
# wrote: the generated file is tracked, because nothing resolves it at run time
# and `cargo run` must stay free of npm.
css:
    npm ci
    npm run css

# Build Testy, the rust-sdk's deterministic stdio test agent (docs/architecture.md §12).
#
# The logic lives in the script rather than here so the CI step and a local run
# are the same build; `just` is the convenience, not the contract.
testy:
    ./scripts/build-testy.sh

# The full suite. Needs Testy: `just testy` first, or `just` for both.
test:
    cargo test

# Prove the app crate builds and tests from tracked sources without any
# JavaScript dependency, network access, runtime stylesheet asset or `dx`.
node-free:
    ./scripts/check-node-free.sh

# Render the bundle's icon from the mark the window draws.
#
# The one recipe that writes `crates/app/bundle/icon.icns`, and — like `css` — what
# it writes is committed: the icon is a packaging asset that `dx bundle` reads
# and nothing in the build does
# (docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md). The drawing is
# `crates/app/src/mark.rs`, the same function the window puts in its own icon; the
# `bundle-icon` example renders it at each of the ten sizes an `.icns` holds, on
# macOS's icon grid, and `iconutil` folds them. `iconutil` is macOS's, so this
# runs on a Mac; the PNGs it is made from need only `cargo`. Run it after
# changing the mark and commit what it wrote.
icon:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "$(uname -s)" != "Darwin" ]; then
        echo "icon: iconutil is macOS's, and this is not a Mac." >&2
        exit 1
    fi
    scratch="$(mktemp -d)"
    trap 'rm -rf "$scratch"' EXIT
    set="$scratch/icon.iconset"
    mkdir "$set"
    for s in 16 32 128 256 512; do
        cargo run -q -p acp-inspector --example bundle-icon -- "$s" "$set/icon_${s}x${s}.png"
        cargo run -q -p acp-inspector --example bundle-icon -- "$((s * 2))" "$set/icon_${s}x${s}@2x.png"
    done
    mkdir -p crates/app/bundle
    iconutil -c icns "$set" -o crates/app/bundle/icon.icns
    echo "icon: crates/app/bundle/icon.icns"

check:
    cargo fmt --check
    bash scripts/check-number-features.sh
    cargo clippy --all-targets -- -D warnings
    # The other renderer, held to the same bar (`check-web`): a warning the
    # desktop build never sees is still a warning.
    cargo clippy -p acp-inspector --all-targets --no-default-features --features web -- -D warnings
    # That the committed stylesheet is what its source generates. A tracked
    # build artifact is a claim, and this is what makes it a fact.
    #
    # `npm ci`, the same install CI runs: `npm install` is allowed to rewrite
    # the lockfile, and a check that can change what it is checking against is
    # not one. The two Rust checks above it need nothing but Rust.
    npm ci
    npm run css:check

# Bundle the app as a macOS application and a disk image.
#
# The one recipe that runs `dx`, and the one thing `dx` does here is assemble
# `ACP Inspector.app` — the release build, the `Info.plist` and the icon, from
# what `crates/app/Dioxus.toml` states
# (docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md). Signing, the disk
# image and notarization are Apple's own tools, run from here rather than from
# inside the bundler: an identity is a person's and never committed config, and
# `hdiutil` is on every Mac while the bundler's image step fetches a script
# from GitHub at bundle time.
#
# Runs only on a Mac, for the Mac it runs on: `dx` does not cross-bundle, so an
# Intel image and an Apple Silicon image are this recipe on two machines.
#
#   just bundle-macos                                  # unsigned, for this machine
#   just bundle-macos "Developer ID Application: Name (TEAMID)"          # signed
#   just bundle-macos "Developer ID Application: Name (TEAMID)" notary   # + notarized
#
# The third form names a `notarytool` keychain profile, stored once with
# `xcrun notarytool store-credentials notary`. Everything lands in `dist/`.
bundle-macos identity="" notary="":
    #!/usr/bin/env bash
    set -euo pipefail
    identity={{quote(identity)}}
    notary={{quote(notary)}}

    if [ "$(uname -s)" != "Darwin" ]; then
        echo "bundle-macos: dx bundles only for the platform it runs on, and this is not a Mac." >&2
        exit 1
    fi
    if [ -n "$notary" ] && [ -z "$identity" ]; then
        echo "bundle-macos: notarization needs a signed bundle; pass a Developer ID identity first." >&2
        exit 1
    fi

    # The icon is a packaging asset, committed and rendered by `just icon`:
    # without it dx stamps the app with the renderer's own logo (crates/app/Dioxus.toml).
    icon=crates/app/bundle/icon.icns
    if [ ! -f "$icon" ]; then
        echo "bundle-macos: $icon is missing; \`just icon\` renders it." >&2
        exit 1
    fi

    # `dx` tracks the `dioxus` minor in Cargo.lock; a different one bundles a
    # program built against a renderer this checkout does not use.
    case "$(dx --version 2>/dev/null || true)" in
        "dioxus 0.7."*) ;;
        *)
            echo "bundle-macos: dx 0.7.x is required (cargo install dioxus-cli --version ^0.7 --locked)." >&2
            exit 1
            ;;
    esac

    rm -rf dist
    dx bundle --macos --release --locked -p acp-inspector \
        --package-types macos --out-dir dist

    app="$(find dist -maxdepth 2 -type d -name '*.app' | head -n 1)"
    if [ -z "$app" ]; then
        echo "bundle-macos: dx produced no .app under dist/:" >&2
        ls -la dist >&2 || true
        exit 1
    fi
    if [ "$(basename "$app")" != "ACP Inspector.app" ]; then
        echo "bundle-macos: note — dx named the bundle '$(basename "$app")', not 'ACP Inspector.app';" >&2
        echo "  [application] name in crates/app/Dioxus.toml is not being read as the product name." >&2
    fi

    # Hardened runtime and a secure timestamp are what notarization requires;
    # `--deep` is not used because there is no nested code to sign — one binary,
    # and the WebView is the system's.
    if [ -n "$identity" ]; then
        codesign --force --options runtime --timestamp --sign "$identity" "$app"
        codesign --verify --strict --verbose=2 "$app"
    fi

    # Notarize the application before it goes into the image, so the ticket is
    # stapled to the thing the reader drags out and it launches offline.
    if [ -n "$notary" ]; then
        ditto -c -k --keepParent "$app" dist/notarize.zip
        xcrun notarytool submit dist/notarize.zip --keychain-profile "$notary" --wait
        xcrun stapler staple "$app"
        rm -f dist/notarize.zip
    fi

    version="$(cargo pkgid -p acp-inspector | sed 's/.*@//')"
    dmg="dist/ACP-Inspector-${version}-$(uname -m).dmg"
    hdiutil create -volname "ACP Inspector" -srcfolder "$app" -ov -format UDZO "$dmg"

    if [ -n "$identity" ]; then
        codesign --force --timestamp --sign "$identity" "$dmg"
    fi
    if [ -n "$notary" ]; then
        xcrun notarytool submit "$dmg" --keychain-profile "$notary" --wait
        xcrun stapler staple "$dmg"
    fi

    echo
    echo "bundle-macos: $app"
    echo "bundle-macos: $dmg"
    if [ -z "$identity" ]; then
        echo "bundle-macos: unsigned — Gatekeeper will want right-click → Open on another Mac."
    fi
