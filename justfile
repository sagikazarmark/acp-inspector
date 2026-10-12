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

# Render the bundles' icons from the mark the window draws.
#
# The one recipe that writes `crates/app/bundle/`, and — like `css` — what it
# writes is committed: the icons are packaging assets that `dx bundle` reads
# and nothing in the build does
# (docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md). The drawing is
# `crates/app/src/mark.rs`, the same function the window puts in its own icon; the
# `bundle-icon` example renders it at each of the ten sizes an `.icns` holds, on
# macOS's icon grid, and `iconutil` folds them. The Linux package's PNGs and
# the Windows `.ico` fill their canvas, as the window's icon does. `iconutil`
# is macOS's, so this runs on a Mac; the rest needs only `cargo`. Run it after
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
    for s in 32 128 256 512; do
        cargo run -q -p acp-inspector --example bundle-icon -- --edge "$s" "crates/app/bundle/${s}x${s}.png"
        echo "icon: crates/app/bundle/${s}x${s}.png"
    done
    cargo run -q -p acp-inspector --example bundle-icon -- --ico crates/app/bundle/icon.ico
    echo "icon: crates/app/bundle/icon.ico"

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

# The tools every `bundle-*` recipe runs, checked before a long build rather
# than after it. `dx` must be the `dioxus` version in Cargo.lock: dx itself
# reports any other one as incompatible with the renderer it is bundling.
[private]
_bundle-tools:
    #!/usr/bin/env bash
    set -euo pipefail
    dioxus_version="$(awk '/^name = "dioxus"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
    case "$(dx --version 2>/dev/null | tr -d '\r' || true)" in
        "dioxus $dioxus_version "* | "dioxus $dioxus_version") ;;
        *)
            echo "bundle: dx $dioxus_version is required, the dioxus version in Cargo.lock" >&2
            echo "  (cargo install dioxus-cli --version =$dioxus_version --locked); found: $(dx --version 2>/dev/null || echo none)." >&2
            exit 1
            ;;
    esac
    if ! cargo about --version >/dev/null 2>&1; then
        echo "bundle: cargo-about is required for the licence notices (cargo install cargo-about --locked)." >&2
        exit 1
    fi

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
# Intel image and an Apple Silicon image are this recipe on two machines (the
# release workflow's two macOS runners). Without an identity the bundle is
# signed ad hoc, as one piece, so that a downloaded copy is one Gatekeeper
# offers to open rather than one it calls damaged.
#
# Runs outside the devenv shell. The shell's C toolchain links Nix's libiconv
# from `/nix/store` and its SDK sets a deployment target of macOS 14, so a
# bundle built there starts only on Macs that have that store path. Release
# bundles are built with Xcode's Command Line Tools, rustup (which reads
# `rust-toolchain.toml`), `dx` 0.7 and cargo-about on PATH (docs/development.md, "Releasing");
# the recipe refuses a Nix shell up front, and `scripts/check-macos-bundle.sh`
# checks the result either way.
#
#   just bundle-macos                                  # unsigned (ad hoc), for this machine
#   just bundle-macos "Developer ID Application: Name (TEAMID)"          # signed
#   just bundle-macos "Developer ID Application: Name (TEAMID)" notary   # + notarized
#
# The third form names a `notarytool` keychain profile, stored once with
# `xcrun notarytool store-credentials notary`. Everything lands in `dist/`.
bundle-macos identity="" notary="": _bundle-tools
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
    # The notices for every crate compiled in, rendered first: `Dioxus.toml`
    # names them as a resource, so dx puts them in `Contents/Resources`, and
    # the image carries them beside the app.
    rm -rf dist
    mkdir dist
    notices=dist/THIRD_PARTY_LICENSES.html
    cargo about generate --locked --fail about.hbs -o "$notices"

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

    if [ ! -f "$app/Contents/Resources/THIRD_PARTY_LICENSES.html" ]; then
        echo "bundle-macos: dx did not copy the licence notices into $app/Contents/Resources." >&2
        exit 1
    fi

    # That another Mac can start it: only system libraries, and the `minos` the
    # plist declares.
    ./scripts/check-macos-bundle.sh "$app"

    # Hardened runtime and a secure timestamp are what notarization requires;
    # `--deep` is not used because there is no nested code to sign — one binary,
    # and the WebView is the system's.
    # Without one, an ad hoc signature over the whole bundle: the linker's
    # covers only the executable, and the plist and resources edited since
    # would leave Gatekeeper a bundle whose seal is broken.
    if [ -n "$identity" ]; then
        codesign --force --options runtime --timestamp --sign "$identity" "$app"
    else
        codesign --force --sign - "$app"
    fi
    codesign --verify --strict --verbose=2 "$app"

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
    version="$(cargo pkgid -p acp-inspector | sed 's/.*[@#]//')"
    target="$(rustc -vV | sed -n 's/^host: //p')"
    dmg="dist/acp-inspector-${version}-${target}.dmg"
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
        echo "bundle-macos: signed ad hoc — on another Mac, Gatekeeper wants Open Anyway in System Settings → Privacy & Security."
    fi

# Bundle the app for Linux: a `.deb` and a `.tar.xz` of the executable.
#
# The `.deb` is `dx`'s (`crates/app/Dioxus.toml` names its dependencies, icons
# and notices) and installs the app with a desktop entry; the archive is the
# same executable with the licences beside it, for anything else. Both are for
# the architecture this runs on: the release workflow runs it on an x86_64 and
# an arm64 runner. Built on the oldest Ubuntu the release supports (22.04), so
# that the glibc it asks for is one every newer distribution has.
#
# Needs the WebKitGTK development packages (docs/development.md,
# "Prerequisites"), `dx` at the `dioxus` version in Cargo.lock and
# cargo-about. Everything lands in `dist/`.
bundle-linux: _bundle-tools
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "$(uname -s)" != "Linux" ]; then
        echo "bundle-linux: dx bundles only for the platform it runs on, and this is not Linux." >&2
        exit 1
    fi
    version="$(cargo pkgid -p acp-inspector | sed 's/.*[@#]//')"
    target="$(rustc -vV | sed -n 's/^host: //p')"

    rm -rf dist
    mkdir dist
    cargo about generate --locked --fail about.hbs -o dist/THIRD_PARTY_LICENSES.html
    dx bundle --linux --release --locked -p acp-inspector --package-types deb --out-dir dist

    deb="$(find dist -maxdepth 1 -name '*.deb' | head -n 1)"
    if [ -z "$deb" ]; then
        echo "bundle-linux: dx produced no .deb under dist/:" >&2
        ls -la dist >&2 || true
        exit 1
    fi

    exe=target/dx/acp-inspector/release/linux/app/acp-inspector
    if [ ! -x "$exe" ]; then
        echo "bundle-linux: dx left no executable at $exe." >&2
        exit 1
    fi
    name="acp-inspector-${version}-${target}"
    mkdir "dist/$name"
    cp "$exe" README.md LICENSE-MIT LICENSE-APACHE dist/THIRD_PARTY_LICENSES.html "dist/$name/"
    tar -C dist -cJf "dist/$name.tar.xz" "$name"
    rm -rf "dist/$name"

    echo
    echo "bundle-linux: $deb"
    echo "bundle-linux: dist/$name.tar.xz"

# Bundle the app for Windows: an `.msi` and a `.zip` of the executable.
#
# The installer is `dx`'s, built with the WiX toolset it fetches the first time;
# it installs per machine and, where the WebView2 runtime is missing (older
# Windows 10), fetches Microsoft's bootstrapper. The archive is the same
# executable, which carries the icon and version information `dx` compiled into
# it, with the licences beside it. Neither is signed: SmartScreen asks first.
#
# Runs on Windows, in the bash Git for Windows brings (`just` runs this script
# with it), with the MSVC toolchain, `dx` at the `dioxus` version in Cargo.lock
# and cargo-about. Everything lands in `dist/`.
bundle-windows: _bundle-tools
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname -s)" in
        MINGW* | MSYS* | CYGWIN*) ;;
        *)
            echo "bundle-windows: dx bundles only for the platform it runs on, and this is not Windows." >&2
            exit 1
            ;;
    esac
    version="$(cargo pkgid -p acp-inspector | sed 's/.*[@#]//')"
    target="$(rustc -vV | sed -n 's/^host: //p' | tr -d '\r')"

    rm -rf dist
    mkdir dist
    cargo about generate --locked --fail about.hbs -o dist/THIRD_PARTY_LICENSES.html
    dx bundle --windows --release --locked -p acp-inspector --package-types msi --out-dir dist

    msi="$(find dist -maxdepth 1 -name '*.msi' | head -n 1)"
    if [ -z "$msi" ]; then
        echo "bundle-windows: dx produced no .msi under dist/:" >&2
        ls -la dist >&2 || true
        exit 1
    fi
    mv "$msi" "dist/acp-inspector-${version}-${target}.msi"

    exe=target/dx/acp-inspector/release/windows/app/acp-inspector.exe
    if [ ! -f "$exe" ]; then
        echo "bundle-windows: dx left no executable at $exe." >&2
        exit 1
    fi
    name="acp-inspector-${version}-${target}"
    mkdir "dist/$name"
    cp "$exe" README.md LICENSE-MIT LICENSE-APACHE dist/THIRD_PARTY_LICENSES.html "dist/$name/"
    pwsh -NoProfile -Command "Compress-Archive -Path 'dist/$name' -DestinationPath 'dist/$name.zip'"
    rm -rf "dist/$name"

    echo
    echo "bundle-windows: dist/acp-inspector-${version}-${target}.msi"
    echo "bundle-windows: dist/$name.zip"

# Package the headless CLI for the platform this runs on, into `dist/`.
#
# A release build of `acp-inspector-cli` with the licences beside it: its own
# notices, which name only what the CLI compiles in. A `.tar.xz` on macOS and
# Linux, a `.zip` on Windows. Adds to `dist/` rather than replacing it, so it
# runs after a `bundle-*` recipe, as the release workflow does. On macOS it
# builds for the same floor as the bundle, and outside the devenv shell for
# the same reason (`bundle-macos`).
package-cli:
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(cargo pkgid -p acp-inspector-cli | sed 's/.*[@#]//')"
    target="$(rustc -vV | sed -n 's/^host: //p' | tr -d '\r')"
    exe=target/release/acp-inspector-cli

    case "$target" in
        *-apple-darwin)
            cc_path="$(command -v cc || true)"
            if [ -n "${IN_NIX_SHELL:-}" ] || [ -n "${DEVENV_ROOT:-}" ] || [ "${cc_path#/nix/store/}" != "$cc_path" ]; then
                echo "package-cli: this is a Nix shell; a macOS release build here links /nix/store libraries." >&2
                echo "  Run it from a plain terminal (docs/development.md, \"Releasing\")." >&2
                exit 1
            fi
            MACOSX_DEPLOYMENT_TARGET="$(sed -n 's/^minimum_system_version *= *"\([0-9.]*\)".*/\1/p' crates/app/Dioxus.toml)"
            export MACOSX_DEPLOYMENT_TARGET
            ;;
        *-windows-*)
            exe="$exe.exe"
            ;;
    esac

    cargo build --release --locked -p acp-inspector-cli

    name="acp-inspector-cli-${version}-${target}"
    mkdir -p dist
    rm -rf "dist/$name"
    mkdir "dist/$name"
    cp "$exe" README.md LICENSE-MIT LICENSE-APACHE "dist/$name/"
    cargo about generate --locked --fail -m crates/cli/Cargo.toml about.hbs -o "dist/$name/THIRD_PARTY_LICENSES.html"
    case "$target" in
        *-windows-*)
            rm -f "dist/$name.zip"
            pwsh -NoProfile -Command "Compress-Archive -Path 'dist/$name' -DestinationPath 'dist/$name.zip'"
            archive="dist/$name.zip"
            ;;
        *)
            tar -C dist -cJf "dist/$name.tar.xz" "$name"
            archive="dist/$name.tar.xz"
            ;;
    esac
    rm -rf "dist/$name"
    echo "package-cli: $archive"
