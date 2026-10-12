# Development

[Documentation index](../README.md#documentation)

- [Prerequisites](#prerequisites)
- [Tasks](#tasks)
- [Layout](#layout)
- [Styles](#styles)
- [Releasing](#releasing)

## Prerequisites

- **Rust.** Install [rustup](https://rustup.rs); it picks up the pinned toolchain from
  `rust-toolchain.toml` on the first build. The oldest Rust the code builds on is 1.92.
- **[just](https://just.systems)**, which runs every task here.
- **Python 3**, only for the test suite's scripted agents.
- **On Linux**, WebKitGTK and the libraries the desktop shell links. On Ubuntu:
  `sudo apt-get install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev`.
  On macOS there is nothing to install beyond Xcode's Command Line Tools.

Or run `devenv shell` at the repository root, which provides all of it
([devenv](https://devenv.sh)).

The desktop build is a WebView app, so building it needs the platform's browser engine: on Linux
that is **WebKitGTK** and its GTK/GLib/libsoup stack, plus **libxdo** and **OpenSSL** for
dependencies `dioxus-desktop` links on that platform. `devenv shell` at the repository root
provides all of them; on a plain Ubuntu that is
`libwebkit2gtk-4.1-dev libxdo-dev libssl-dev`, which is what CI installs. On **macOS** none of
it applies and there is nothing to install: the engine is WKWebView and the TLS is Security,
both from the system SDK, so Xcode's command line tools (or `devenv shell`, which carries the
Apple SDK) and a Rust toolchain are all a build needs. Nothing in `crates/core/` needs any of it on
either platform, and neither does `just check-web`, which compiles the app without the desktop
shell.

**Node is not required to compile or run a valid checkout.** The window's production stylesheet is
generated from Tailwind and daisyUI, committed to the tree
and compiled into the binary rather than resolved as an asset
([ADR 0003](adr/0003-the-stylesheet-is-generated-and-committed.md)), so `cargo run` needs no
`dx`, no npm install and no network. Node is needed to **change** the styles, regenerating the
committed sheet from its source with
`just css`, and to **verify** that the committed sheet is what its source generates, which is a step
in CI and in `just check`. Everything else a contributor runs — `cargo run`, `cargo test`,
`cargo fmt`, `cargo clippy` — never asks for it.

## Tasks

The integration tests drive the same Testy the [user guide](guide.md#pointing-it-at-testy) points
the window at. It is `publish = false`, so it is built from a rust-sdk checkout rather than
depended on:

```sh
just testy    # clone/update the checkout under .testy/ and build the binary
just test     # cargo test
just          # both
just check    # fmt, clippy (both renderers), number features, the committed stylesheet
just deny     # cargo deny: advisories, licences, sources
just css      # regenerate that stylesheet — the one recipe that needs Node
just node-free # build and test the desktop package offline with Node and dx blocked
just icon     # regenerate the bundle's icon from the window's own mark, on a Mac
just licenses # render THIRD_PARTY_LICENSES.html with cargo-about
just bundle-macos # the .app and its disk image, on a Mac — the one recipe that runs dx
```

CI ([`.github/workflows/ci.yaml`](../.github/workflows/ci.yaml)) runs fmt, clippy for both
renderers, the number-feature matrix, Testy at its pinned rev, `cargo test`, the stylesheet check,
the Node tests and the Node-free build on macOS and Ubuntu, `cargo deny check`, and a
`cargo check` on Windows. The toolchain is pinned in `rust-toolchain.toml`; `rust-version` in
`Cargo.toml` is the oldest one the code builds on.

Testy is built from rust-sdk v3.3.0, pinned by commit in `scripts/build-testy.sh` (and the Dagger
module), because the suite asserts on its exact behaviour
([§12](architecture.md#12-dummy-agent-and-test-strategy)). `ACP_RUST_SDK_REV` builds another
revision (`ACP_RUST_SDK_REV=main just testy` for the moving one), and `TESTY_BIN` points the tests
at a binary built elsewhere. The suite's scripted agents need `python3` on `PATH`. Moving the
pin is its own commit, with whatever the new Testy needs.

The complete release check, including both supported window sizes, all Appearance choices, keyboard
operation and representative Agent states, is the [desktop smoke matrix](desktop-smoke-matrix.md).

## Layout

```
crates/core/   acp-inspector-core — headless: the connection contract, stdio, the trace,
               the typed ACP v1 client, the JSONL export, the spawn form's recent
               commands, the window's preferences, and the stores they fill
crates/app/    acp-inspector — the screens and the one binary, strictly presentational,
               over those stores; a Cargo feature per renderer (`desktop`, the default;
               `web`, declared and checked, runnable once core builds for wasm32)
               src/components/  form components installed from the daisyUI registry
                                (PROVENANCE.md)
               style/   theme.css, retained inspector aliases; input.css, daisyUI
                        production input plus layout and behaviour rules; sheet.css,
                        the generated output, committed and compiled in
               src/*.test.mjs   Node tests of the window's clipboard and navigation scripts
               examples/  bundle-icon.rs, which renders the icons (`just icon`)
               bundle/  the bundles' icons: icon.icns, the PNGs, icon.ico
               Dioxus.toml  what the bundles say about themselves
crates/cli/    acp-inspector-cli — the headless CLI over core: run an agent, answer
               permissions by policy, export the trace, exit with a code for CI
CONTEXT.md     the glossary: this project's terms, and nothing else
docs/          guide.md, cli.md and development.md; architecture.md, the spec;
               trace-export.md, the export's pinned schema; client-services.md;
               adr/, the decisions that are the inspector's own; release-plan.md;
               desktop-smoke-matrix.md and native-acceptance.md, the manual release
               checks; performance-hardening.md; schemaform-spike.md
design/        screenshots of the window and the visual audit
scripts/       build-testy.sh, the integration tests' agent; check-node-free.sh;
               check-number-features.sh; check-macos-bundle.sh, the bundle guard;
               acceptance-agent.py and its test; performance-agent.py;
               spike-testy-literals.py; check-*.mjs, Node checks of window behaviour
.github/       CI (workflows/ci.yaml) and the release (workflows/release.yaml)
justfile       every task (`just --list`)
rust-toolchain.toml  the pinned toolchain
deny.toml      cargo-deny: advisories, licences, sources
about.toml     cargo-about: the third-party notices the bundle ships (about.hbs)
devenv.*       the development shell (`devenv shell`)
dagger.*       a Dagger module that builds Testy in a container
package.json   the Tailwind CLI and daisyUI development inputs for the production
               stylesheet — nothing else here is a Node project
```

Split by layer and not by renderer
([ADR 0011](adr/0011-one-ui-crate-a-feature-per-renderer.md)): the CLI is a second surface
over core, beside the app rather than on top of it. The project began inside
another repository and depended on none of its crates: patterns were copied, never linked
([§13](architecture.md#13-relationship-to-the-host-repos-assets)). The crate names, the
application bundle's name and its identifier are fixed
([ADR 0010](adr/0010-the-bundle-is-the-one-thing-dx-makes.md),
[ADR 0011](adr/0011-one-ui-crate-a-feature-per-renderer.md)).

The spec and the ADRs are the decisions of record: a change that goes against one of them should
say so and update it.

## Styles

Changing the look means editing `crates/app/style/input.css` — or `crates/app/style/theme.css`, which
holds the design vocabulary and the colour tokens it imports — running `just css`, and committing
what it wrote: `crates/app/style/sheet.css` is a tracked build artifact, and `just check` — and CI —
fail when it is not what the source generates.

**daisyUI owns ordinary components and themes**
([ADR 0005](adr/0005-daisyui-owns-ordinary-components-and-themes.md)). Use its standard classes
and variants before writing custom control presentation. Inspector Light and Inspector Dark own
light and dark component values; retained custom CSS is for inspector layout, meaning, evidence,
accessibility, ordering or behaviour. [ADR 0004](adr/0004-the-design-vocabulary.md) keeps
semantic, contrast, evidence and motion constraints without imposing the former exact density
scales. The original comparison and measurements remain in
[ADR 0003](adr/0003-the-stylesheet-is-generated-and-committed.md#the-daisyui-verdict).

## Releasing

A release is a tag, made with [cargo-release](https://github.com/crate-ci/cargo-release) (in the
devenv shell). The crates share one version, in the root `Cargo.toml`'s `[workspace.package]`;
`cargo release` bumps it, commits, makes a signed `v{version}` tag and pushes both:

```sh
cargo release minor            # a dry run: what it would change, commit, tag and push
cargo release minor --execute  # 0.1.0 → 0.2.0
```

Nothing goes to crates.io: every crate is `publish = false`. To release the version already in
`Cargo.toml` (the first release, say) without bumping it, tag and push only:

```sh
cargo release tag --execute
cargo release push --execute
```

The [release workflow](../.github/workflows/release.yaml) checks that the tag is the crates'
version, builds every artifact on a runner of the platform it is for, and publishes them to a
GitHub Release with a `SHA256SUMS` file and notes generated from the merged pull requests. The
release is drafted until every artifact is on it, so a failed build never leaves a half-filled
release public. A version with a pre-release part (`0.2.0-rc.1`) is marked as a pre-release.

| Platform | App | CLI |
| --- | --- | --- |
| macOS, Apple Silicon and Intel | `.dmg` | `.tar.xz` |
| Linux, x86_64 and arm64 | `.deb`, and a `.tar.xz` of the executable | `.tar.xz` |
| Windows, x86_64 | `.msi`, and a `.zip` of the executable | `.zip` |

Every artifact carries the third-party notices, `THIRD_PARTY_LICENSES.html`. None is signed: see
the [user guide](guide.md#supported-platforms) for what macOS and Windows ask before the first
launch.

A pull request that touches the packaging (the workflow, the `justfile`, `Dioxus.toml`, the icons)
runs the same builds without publishing, so a broken recipe is found before a tag is.

### Building artifacts by hand

Each workflow step is a `just` recipe, run on the platform it builds for: `dx` does not
cross-bundle. They need `dx` at exactly the `dioxus` version in `Cargo.lock` (the recipes name it
and the install command if it differs) and cargo-about (`cargo install cargo-about --locked`), and
write into `dist/`:

```sh
just bundle-macos     # the .app and its .dmg
just bundle-linux     # the .deb and the executable's .tar.xz
just bundle-windows   # the .msi and the executable's .zip
just package-cli      # the CLI's archive; after a bundle-* recipe, which empties dist/ first
```

`just bundle-macos` has `dx bundle` assemble the application from `crates/app/Dioxus.toml`, then
names it `ACP Inspector.app`, checks it, and signs it ad hoc. Passed an identity and a `notarytool`
profile, Apple's own tools sign and notarize it instead; it is made from the build and is no part
of it, which is the whole of [ADR 0010](adr/0010-the-bundle-is-the-one-thing-dx-makes.md). The
icons are under `crates/app/bundle/`, rendered from the window's own mark by `just icon`.

```sh
just bundle-macos                                                    # signed ad hoc, for this machine
just bundle-macos "Developer ID Application: Name (TEAMID)"          # signed
just bundle-macos "Developer ID Application: Name (TEAMID)" notary   # + notarized
```

**Build macOS artifacts outside the devenv shell.** Nix's C toolchain links its own libiconv from
`/nix/store` and its SDK targets macOS 14, so a binary built there starts only on Macs that have
that store path. From a plain terminal you need Xcode's Command Line Tools, rustup and `just`.
`bundle-macos` and `package-cli` refuse to run in a Nix shell, build with
`MACOSX_DEPLOYMENT_TARGET` set to `minimum_system_version` from `Dioxus.toml`, and
`scripts/check-macos-bundle.sh` fails the bundle unless every library it links is the system's,
its `minos` matches `LSMinimumSystemVersion`, and its signature verifies.

**Build Linux artifacts on Ubuntu 22.04**, as the workflow does: a binary asks for the glibc it was
linked against, so building on the oldest supported release is what lets it run on newer ones.
dx's prebuilt Linux binary needs a newer glibc than 22.04 has, so install it from source there.
The `.deb` declares its run-time dependencies (`[bundle.deb]` in `Dioxus.toml`).

**Windows** needs the MSVC toolchain, and Git for Windows, whose bash `just` runs the recipes with.
The first `.msi` build downloads the WiX toolset.
