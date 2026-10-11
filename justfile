# The ACP Inspector's tasks. Run from the repository root.

# Build Testy and run every check.
default: testy test

# Run the app in its desktop shell.
#
# Plain `cargo run`, no `dx`: nothing is resolved as an asset and nothing is
# bundled, and keeping it that way is what makes the shell buildable by anyone
# with a Rust toolchain and the WebView libraries (see docs/development.md). The
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

# Run the headless CLI: `just cli -- run --prompt hi -- AGENT [ARGS...]`.
#
# The same core without a window (docs/cli.md). The leading `--`
# keeps `just` from reading the CLI's own flags, and is dropped here; the
# arguments are passed positionally so a prompt with spaces stays one argument.
[positional-arguments]
cli *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "${1:-}" = "--" ]; then shift; fi
    exec cargo run -q -p acp-inspector-cli -- "$@"

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

# Portable acceptance Agent's real stdio subprocess contract (Python 3.9+).
test-acceptance-agent:
    python3 -m unittest discover -s scripts -p test_acceptance_agent.py -v

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

# Check the dependency graph's advisories, licences and sources (`deny.toml`).
# CI runs it; it fetches the RustSec advisory database, so it needs the network.
deny:
    cargo deny check

# Render the third-party licence notices the app ships with, from `about.toml`
# and `about.hbs`. Generated at bundle time and never committed: the file is a
# function of `Cargo.lock`. `bundle-macos` writes it into the bundle and the
# disk image. Needs cargo-about (in the devenv shell; elsewhere
# `cargo install cargo-about --locked`).
licenses out="dist/THIRD_PARTY_LICENSES.html":
    mkdir -p "$(dirname {{quote(out)}})"
    cargo about generate --locked --fail about.hbs -o {{quote(out)}}

# Bundle the app as a macOS application and a disk image.
#
# The one recipe that runs `dx`, and the one thing `dx` does here is assemble
# the application bundle — the release build, the `Info.plist` and the icon, from
# what `crates/app/Dioxus.toml` states
# (docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md). Everything after
# that is this recipe's: the bundle's name, the licence notices, the check that
# it will launch on another Mac, signing, the disk image and notarization.
# Signing and notarization are Apple's own tools, run from here rather than from
# inside the bundler: an identity is a person's and never committed config, and
# `hdiutil` is on every Mac while the bundler's image step fetches a script
# from GitHub at bundle time.
#
# Runs only on a Mac, for the Mac it runs on: `dx` does not cross-bundle, so an
# Intel image and an Apple Silicon image are this recipe on two machines.
#
# Runs outside the devenv shell. The shell's C toolchain links Nix's libiconv
# from `/nix/store` and its SDK sets a deployment target of macOS 14, so a
# bundle built there starts only on Macs that have that store path. Release
# bundles are built with Xcode's Command Line Tools, rustup (which reads
# `rust-toolchain.toml`), `dx` 0.7 and cargo-about on PATH (docs/development.md, "Releasing");
# the recipe refuses a Nix shell up front, and `scripts/check-macos-bundle.sh`
# checks the result either way.
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
    product="ACP Inspector"

    if [ "$(uname -s)" != "Darwin" ]; then
        echo "bundle-macos: dx bundles only for the platform it runs on, and this is not a Mac." >&2
        exit 1
    fi
    if [ -n "$notary" ] && [ -z "$identity" ]; then
        echo "bundle-macos: notarization needs a signed bundle; pass a Developer ID identity first." >&2
        exit 1
    fi

    # Fail before a long build rather than after it (the guard below would).
    cc_path="$(command -v cc || true)"
    if [ -n "${IN_NIX_SHELL:-}" ] || [ -n "${DEVENV_ROOT:-}" ] || [ "${cc_path#/nix/store/}" != "$cc_path" ]; then
        echo "bundle-macos: this is a Nix shell (cc is ${cc_path:-missing}). A bundle built here links /nix/store" >&2
        echo "  libraries and targets the shell's SDK, and no other Mac can start it. Run it from a plain" >&2
        echo "  terminal with Xcode's Command Line Tools and rustup (docs/development.md, \"Releasing\")." >&2
        exit 1
    fi

    # The icon is a packaging asset, committed and rendered by `just icon`:
    # without it dx stamps the app with the renderer's own logo (crates/app/Dioxus.toml).
    icon=crates/app/bundle/icon.icns
    if [ ! -f "$icon" ]; then
        echo "bundle-macos: $icon is missing; \`just icon\` renders it." >&2
        exit 1
    fi

    # `dx` must be the `dioxus` version in Cargo.lock: dx itself reports any
    # other one as incompatible with the renderer it is bundling.
    dioxus_version="$(awk '/^name = "dioxus"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
    case "$(dx --version 2>/dev/null || true)" in
        "dioxus $dioxus_version "* | "dioxus $dioxus_version") ;;
        *)
            echo "bundle-macos: dx $dioxus_version is required, the dioxus version in Cargo.lock" >&2
            echo "  (cargo install dioxus-cli --version =$dioxus_version --locked); found: $(dx --version 2>/dev/null || echo none)." >&2
            exit 1
            ;;
    esac
    if ! cargo about --version >/dev/null 2>&1; then
        echo "bundle-macos: cargo-about is required for the licence notices (cargo install cargo-about --locked)." >&2
        exit 1
    fi

    # The deployment target is the one the plist declares: `minimum_system_version`
    # becomes `LSMinimumSystemVersion`, and this makes the linker stamp the same
    # `minos`, which the guard checks.
    min_macos="$(sed -n 's/^minimum_system_version *= *"\([0-9.]*\)".*/\1/p' crates/app/Dioxus.toml)"
    if [ -z "$min_macos" ]; then
        echo "bundle-macos: no minimum_system_version in crates/app/Dioxus.toml." >&2
        exit 1
    fi
    export MACOSX_DEPLOYMENT_TARGET="$min_macos"

    # The release profile strips, so dx strips the binary again with the
    # toolchain's own rust-objcopy, pointing it at LLVM with LD_LIBRARY_PATH —
    # which macOS ignores, so rustup's rust-objcopy cannot load libLLVM. The
    # fallback path is where it is.
    rm -rf dist
    DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib" \
        dx bundle --macos --release --locked -p acp-inspector \
        --package-types macos --out-dir dist

    app="$(find dist -maxdepth 2 -type d -name '*.app' | head -n 1)"
    if [ -z "$app" ]; then
        echo "bundle-macos: dx produced no .app under dist/:" >&2
        ls -la dist >&2 || true
        exit 1
    fi

    # dx names the bundle after the crate (`AcpInspector.app`) and ignores
    # `[application] name` (release plan T2, #22). Rename it and set the two
    # names Finder and the menu bar show, before anything is signed.
    if [ "$(basename "$app")" != "$product.app" ]; then
        renamed="$(dirname "$app")/$product.app"
        mv "$app" "$renamed"
        app="$renamed"
    fi
    plist="$app/Contents/Info.plist"
    for key in CFBundleName CFBundleDisplayName; do
        /usr/libexec/PlistBuddy -c "Set :$key $product" "$plist" 2>/dev/null \
            || /usr/libexec/PlistBuddy -c "Add :$key string $product" "$plist"
    done

    # The notices for every crate compiled in, inside the bundle and (below)
    # next to it in the image.
    notices="$app/Contents/Resources/THIRD_PARTY_LICENSES.html"
    cargo about generate --locked --fail about.hbs -o "$notices"

    # That another Mac can start it: only system libraries, and the `minos` the
    # plist declares.
    ./scripts/check-macos-bundle.sh "$app"

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

    # The image holds the app, a link to /Applications to drag it onto, and
    # the licence notices.
    version="$(cargo pkgid -p acp-inspector | sed 's/.*@//')"
    dmg="dist/ACP-Inspector-${version}-$(uname -m).dmg"
    stage="dist/image"
    rm -rf "$stage"
    mkdir "$stage"
    ditto "$app" "$stage/$product.app"
    cp "$notices" "$stage/THIRD_PARTY_LICENSES.html"
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "$product" -srcfolder "$stage" -ov -format UDZO "$dmg"
    rm -rf "$stage"

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
        echo "bundle-macos: unsigned — on another Mac, Gatekeeper wants Open Anyway in System Settings → Privacy & Security."
    fi

# Reads `dist-workspace.toml` and the workspace; builds nothing and publishes
# nothing.
#
# Show what `dist` would build for Linux and Windows.
dist-plan:
    dist plan

# Untested on CI, and never uploads: `dist-workspace.toml` configures no CI. On
# Linux it needs the WebKitGTK development packages (docs/development.md, "Prerequisites"); on
# Windows, the MSVC toolchain. Ship `just licenses` beside it (L5).
#
#   just dist-build                              # this machine's target
#   just dist-build x86_64-unknown-linux-gnu     # a named one, if this machine can build it
#
# Build the Linux or Windows archive into `target/distrib/`.
dist-build target="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n {{quote(target)}} ]; then
        dist build --artifacts=local --target {{quote(target)}}
    else
        dist build --artifacts=local
    fi
