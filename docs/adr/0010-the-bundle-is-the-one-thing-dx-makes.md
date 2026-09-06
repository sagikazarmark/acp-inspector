# The bundle is the one thing dx makes

## Context

The build has one command, and the record says so in three places. `cargo run -p acp-inspector`
is "the whole build" ([§15 q1](../architecture.md#15-open-questions--validation-gaps)); the
`justfile` runs it "plain, no `dx`: nothing is resolved as an asset and nothing is bundled"; and
[ADR 0003](0003-the-stylesheet-is-generated-and-committed.md) spent its length making sure that
stayed true when Node arrived — the stylesheet is generated, committed and `include_str!`'d, so a
Rust toolchain and the platform's WebView libraries are everything a checkout needs. The window's
icon is drawn in code for the same reason (`crates/app/src/shell.rs`, "drawn rather than shipped").
What all of that bought is a binary that is already the whole program.

The same paragraph of §15 q1 records the other half: **packaging is still untried** — nobody has
built a distributable artifact of this, only run it. A macOS application is now wanted: a thing a
reader double-clicks, handed over as a disk image, and one Gatekeeper lets them open. On macOS that
is a directory — `Contents/Info.plist`, `Contents/MacOS/<binary>`, `Contents/Resources/<icon>.icns`
— and, for anyone but the person who built it, a Developer ID signature with the hardened runtime and
a notarization ticket. Because the binary is self-contained, bundling is exactly that wrapper.

Three ways to make the wrapper were weighed.

- **A shell script in `scripts/`**, beside the two that are there: a plist template, `iconutil`,
  `codesign`, `hdiutil`. Nothing new enters the project. What it costs is a plist kept right by hand
  and, later, a second script for every other format.
- **`cargo-bundle`**: pure cargo, configured in `[package.metadata.bundle]`. What it costs is a
  lightly maintained bundler that does not sign, chosen for a renderer whose framework ships its own.
- **`dx bundle`**: the renderer's own CLI over tauri-bundler, maintained against the Dioxus this
  project compiles, one `Dioxus.toml` describing the bundle for every platform it will ever want.
  What it costs is a second toolchain — and it is the one the checkout has been kept free of.

The third is taken, and the reason it does not contradict the record is the distinction the record
was actually drawing. **The objection to `dx` was never to `dx`; it was to needing it to build or to
run.** A bundle is neither. It is made *from* the build and the build does not depend on it: delete
`Dioxus.toml` and the checkout compiles, runs and tests as before; block `dx` from `PATH`, as
`scripts/check-node-free.sh` does, and the proof still passes. What ADR 0003 refused was an
input to the binary produced by a tool outside the Rust toolchain. A plist wrapped around the
finished binary is not an input to it.

**What `dx` does not get to do is sign, notarize or make the image**, and each refusal has a reason.
A signing identity is a person's, held in their Keychain; for the bundler to apply it the identity
would have to be committed configuration (`[bundle.macos] signing_identity`), which is the wrong
place for a fact about a person. tauri-bundler's disk-image step downloads the `create-dmg` script
from GitHub when it runs — a network dependency inside a release step, the same kind ADR 0003 removed
from the build. And `codesign`, `hdiutil`, `notarytool` and `stapler` are on every Mac with the
command line tools, are what Apple documents, and take exactly the flags (`--options runtime`,
`--timestamp`) notarization checks for. So `dx` assembles the `.app`, and the recipe does the rest
with tools it can be held to.

Two decisions the project had deferred, the bundle forces. The README says the product name and the
crate names are not fixed, and that naming happens at the split. A bundle cannot wait: `CFBundleName`
is what Finder shows and `CFBundleIdentifier` is what macOS keys preferences, TCC grants, Keychain
items and notarization history to — permanent, in the sense that a change is a migration. The bundle
therefore carries **ACP Inspector**, which is what the window has titled itself all along, and
**`dev.sagikazarmark.acp-inspector`**, under the slug the state directory (`crates/core/src/kept.rs`) and
the `clientInfo` name (`crates/core/src/client.rs`) already use. The crate names stay provisional; this fixes
the bundle's, not theirs.

The icon is the one binary asset the bundle needs and the tree does not have. `shell::icon()` draws
a 64-pixel mark and argues against a PNG in the tree because the build story is that `cargo run`
needs nothing resolved. An `.icns` needs the sizes up to 1024 pixels, lives in
`Contents/Resources`, and is never compiled in or resolved at run time — so it is a *packaging*
asset, and the argument against a build asset does not reach it. It is not optional: given no icon,
`dx` stamps the bundle with the renderer's own logo, which is the outcome `shell::icon()` exists to
refuse.

## Decision

- **`dx bundle` assembles the macOS application** — the release build, the `Info.plist` and the
  icon — from `crates/app/Dioxus.toml`, and **`just bundle-macos` is the only thing that runs it.**
- **The checkout stays `cargo`-only.** `cargo build`, `cargo run`, `cargo test`, `just check` and
  `just node-free` never invoke `dx`; the node-free proof keeps blocking it from `PATH`; nothing but
  `dx bundle` reads `Dioxus.toml`. A checkout with that file deleted is a valid checkout.
- **Signing, notarization and the disk image are Apple's own tools, run by the recipe.** The
  identity and the `notarytool` profile are recipe arguments; neither is ever configuration in the
  tree. The application is signed with the hardened runtime and a timestamp, notarized and stapled
  *before* it goes into the image, so what a reader drags out launches offline; the image is then
  signed, notarized and stapled in turn.
- **The bundle's name is `ACP Inspector` and its identifier is `dev.sagikazarmark.acp-inspector`.**
  Both are settled by this document; the crate names are not.
- **The icon is a packaging asset at `crates/app/bundle/icon.icns`**, rendered from the same mark the
  window draws, at the sizes an `.icns` needs. It is committed, like the stylesheet, and `just icon`
  is what writes it; the recipe refuses to run without it rather than let `dx` supply a default.
- **Hardened runtime on, no entitlements, and App Sandbox never.** This tool's job is spawning an
  executable the reader named in a directory the reader chose, which is precisely what the sandbox
  forbids. The WebView runs in its own process, so no JIT or memory entitlement is owed.
- **The version is the crate's** (`crates/app/Cargo.toml`), the architecture is the machine's — `dx`
  does not cross-bundle — and an Intel image and an Apple Silicon image are the recipe run twice.

## Consequences

- **A `Dioxus.toml` in `crates/app/`, a recipe in the `justfile`, `dist/` ignored, and §15 q1
  amended** to say what "no bundling step" now means: none in the build, one after it.
- **The icon exists, and "the same mark" is now literally the same function.** The drawing moved
  out of `shell.rs` into `crates/app/src/mark.rs`, restated as proportions of its side and drawn by
  coverage rather than by pixel, so a size is a parameter: the window asks for it at 64px and the
  `bundle-icon` example asks for it at the ten sizes an `.icns` holds, on macOS's icon grid — 824
  on 1024, with the margin every Dock icon keeps — and `just icon` folds those with `iconutil` into
  `crates/app/bundle/icon.icns`. The example is an example and not a second binary so that `cargo run`
  and `dx bundle` still see one program; its one encoder, `png`, is a dev-dependency already in the
  resolved graph under the renderer. The `.icns` is the first binary asset in the tree, and it is
  the kind ADR 0003's argument allows: written by a recipe, committed, and read by nothing but the
  bundler.
- ~~**None of this has been run.**~~ It was written on Linux, and `dx` bundles only for the platform
  it runs on; the first `just bundle-macos` on a Mac was owed, and it is what retires §15 q1's
  "packaging is still untried". Three things that run verifies, in order: that `[application] name`
  is read as the product name (the recipe reports the name `dx` chose), that the icon lands, and that
  notarization accepts the signature the recipe applied. **The run has happened**, unsigned, on an
  Apple Silicon Mac with `dx` 0.7.9, when the icon landed in the tree. It answered the first two:
  the icon lands — `Contents/Resources/InspectorDesktop.icns`, byte for byte the committed file, and
  `CFBundleIconFile` names it — and **the name is not read**: the bundle came out as
  `InspectorDesktop.app` with `CFBundleName` and `CFBundleDisplayName` to match, the crate's name
  and not `[application] name`, while `CFBundleIdentifier` was honoured. The recipe reported it as
  written. Where `dx` 0.7 takes the product name from, and a signed and notarized run, are the two
  things still owed.
- **A second toolchain exists for the release path**, and it must track the `dioxus` minor in
  `Cargo.lock`: the recipe refuses anything but `dx` 0.7.x. The host repository's `devenv.nix`
  provides 0.7.9 today and does not travel with the project — ADR 0003's point about Node, made again
  about `dx` — so the repository this becomes has to provide its own. Dependabot bumps neither.
- **The bundle surfaces a defect the checkout could not.** An application launched from Finder
  inherits launchd's `PATH` — `/usr/bin:/bin:/usr/sbin:/sbin` — and not the reader's shell's, and
  `crates/core/src/stdio.rs` resolves the agent with `Command::new` through `PATH` and no shell. A bare
  `claude-code-acp` or `npx` that spawns from `cargo run` fails to spawn from the bundle. The transport
  reports that honestly as spawn evidence; a reader reads it as broken. The workarounds exist today —
  an absolute path, or `PATH=…` in the spawn form's environment field, which `Command` honours for
  lookup on Unix — and the fix, reading the login shell's `PATH` at startup on macOS, is a ticket for
  the desktop shell and not for core. It is owed before a bundle goes to anyone who did not build it.
- **The smoke matrix owes a row**: the bundle launches from Finder, and spawns Testy by absolute
  path.
- **The README's "the product name is not fixed" narrows to the crate names.**
- **ADR 0003 is untouched.** The artifact contains what the binary contains, and nothing in it is
  resolved at run time. What changed is a sentence in §15 q1 and a comment in the `justfile`, and
  both now say the same thing this document does.
- **Other formats are `--package-types` on the same file** — `.deb`, AppImage, `.msi` — and none is
  decided here. Each carries its own signing, its own icon formats and its own runtime dependency
  story, and each is a decision when it is wanted.
