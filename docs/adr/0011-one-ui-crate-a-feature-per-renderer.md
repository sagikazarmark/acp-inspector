# One UI crate, a feature per renderer

## Context

The project has always been two crates, and the record has always called the second one *the
desktop crate*. `inspector-core` was headless by rule and by test — `tests/headless.rs` walks the
resolved dependency graph and fails the build if any `dioxus*` crate reaches it — and
`inspector-desktop` was "strictly presentational: props in, callbacks out"
([§5](../architecture.md#5-the-coreui-split)). §1 promised on the strength of that boundary that a
web surface would be *additive*, and called going web "a possibility, not a commitment —
unscheduled". The README said the crate names were provisional, and
[ADR 0010](0010-the-bundle-is-the-one-thing-dx-makes.md) fixed the bundle's name while leaving them
so.

Web is now scheduled, and the split from the host repository has happened. Both put the same
question: what is the second crate *for*, now that it is about to draw in two renderers? Three
layouts were weighed.

- **A crate per renderer** — `desktop/`, `web/`, and a `ui/` library of shared components under
  both. This is the layout Dioxus's `Workspace` template generates. Its own README gives the one
  reason for it: "this setup makes it easy to let the views for each platform change
  independently." That is a reason for an app whose *screens* differ by platform — a phone layout
  and a desktop one — and this tool's do not: the Timeline, the Console, the rail and the spawn form
  are the same drawing in a browser tab as in a window. What would differ is the shell around them,
  and in this codebase that shell is small and already in one place: `LaunchBuilder::desktop()` and
  the window's size, `shell.rs`'s menu strip and icon, one `set_title`, one `use_muda_event_handler`.
  Everything else that touches the renderer goes through `document::eval`, which is the same call on
  web. A crate per renderer would put a `main.rs` of forty lines in each and move every screen to
  the library, to separate what does not differ.
- **One crate, a Cargo feature per renderer** — `[features] desktop = ["dioxus/desktop"], web =
  ["dioxus/web"]`, and `#[cfg(feature = "desktop")]` on the shell. This is the layout Dioxus's
  default templates generate and the one its tooling assumes: `dx serve --desktop` is
  `cargo build --no-default-features --features desktop`. It is also what the two largest open
  Dioxus applications the survey found actually do. The Dioxus docsite is a workspace of ten crates
  split by *domain* — docs per version, search, code highlighting — with `web`, `server` and
  `fullstack` as features on the one package that renders. Uplink is a workspace of `kit`, `ui`,
  `common` and `icons` split by *layer*, with platform differences as `cfg(target_os)` inside them.
  Neither has a crate per renderer with a real application behind it.
- **The status quo**, two crates and the second still named for one renderer, with web added as a
  third crate when it comes. Cheapest today, and wrong the day `web` lands: a crate called
  `desktop` that also builds for the browser, or a third `main.rs` that is the second one with a
  different launch call.

The second is taken. **The pattern in the wild is crates by layer, features by renderer, and this
project already has the layer split that matters.** Core is the layer; the app is the layer above
it; and the renderer is not a layer but a build's choice about the one crate that draws.

What the choice rests on is a fact about the web build that no crate layout pre-solves, and it is
recorded here so the web ring inherits it decided. Core cannot compile for `wasm32` today:
`stdio.rs` spawns a child process, `kept.rs`, `export.rs`, `settings.rs` and `recent.rs` write
files, and the pumps spawn onto a tokio runtime. But the app crate consumes core's types
pervasively as props — `SessionSettings`, `TracedFrame`, `Turn`, `PermissionRequest`, `Roots`,
`ConnectionStatus`, some forty paths across fifteen files — and constructs the `Inspector` in
process. Two ways out were weighed. **Core compiles on wasm minus its I/O**, with the stdio
factory and the filesystem modules behind a gate, and the browser fed by a WebSocket
`ConnectionFactory` — the "additive factory" [§6](../architecture.md#6-the-transport-seam) and
[§11](../architecture.md#11-v2-seams-kept-open) seam 5 have promised since the first draft — so
the browser runs the typed layer, the stores, the turns and the conformance rules itself, and the
backend is a relay that spawns the agent and pumps frames. Or **a `model` crate is extracted** of
shared types, core keeps all its logic native, and the browser is a thin view over a server that
runs core and re-exports every store over the wire. The first is taken: it is what the transport
seam was designed for, it keeps `headless.rs` holding as written, and it leaves the names below
right. Under the second, `core` would want splitting and the naming question would reopen.

Two names follow, and a location.

- **`core` stays `core`**, and its package becomes **`acp-inspector-core`**. *Core* is the accurate
  word for what it is — everything that is not a screen: connection lifecycle, the typed layer, the
  stores, conformance, export — and `domain`, `engine` and `model` would each name less than that.
  The `acp-` prefix follows `<project>-core`; `inspector-core` is generic enough to collide the day
  this is published, which ADR 0010 already treats as coming.
- **`desktop` becomes `app`**, and its package becomes **`acp-inspector`**. It is about to stop
  being *the desktop crate* and become *the crate that draws, with a feature per renderer*, and
  `desktop` would be false the day `web` is enabled. `ui` is what Dioxus's template calls a shared
  component *library*; this crate is the binary, and `app` says so. Naming the package after the
  project means the thing a reader runs is `acp-inspector` — `cargo run -p acp-inspector`,
  `dx bundle -p acp-inspector` — and `Dioxus.toml` has said `name = "ACP Inspector"` since ADR 0010.
  The product is the crate.
- **Both move under `crates/`.** Two crates at the root was fine and would not have been moved for
  its own sake. But the rename touches every path anyway — the `justfile`, the node-free script,
  the architecture document, the headless test — and the web ring may well bring a third crate, the
  relay; that would be a second round of the same churn. So the paths change once, now, and not
  again. `crates/` rather than the template's `packages/` because this is a Rust repository and
  `crates/` is the wider Rust convention; the choice is a coin and it is not to be revisited.

## Decision

- **Two crates, split by layer and never by renderer:** `crates/core` is `acp-inspector-core`,
  headless, and `crates/app` is `acp-inspector`, the screens and the one binary.
- **The renderer is a Cargo feature of the app crate**, and a build picks exactly one: `desktop`
  is the default and `web` is the other. A build with neither is a `compile_error!`, not a
  binary that opens nothing. The workspace's `dioxus` dependency carries no renderer feature; the
  renderer is what a build of the app is *for*, and a workspace has no build.
- **`shell.rs` is the one module that imports `dioxus::desktop`**, and it owns everything the
  operating system draws around the screens — the window and its size, the title bar, the menu
  strip, the icon. It is gated on the `desktop` feature along with the three places `main.rs` calls
  into it: launch, `set_title`, and the menu's answers. The screens under them never learn which
  window they are in, and anything that would teach them goes into `shell.rs` or does not go in.
- **The web build is checked on the native target from now on**, by `just check-web` and by the
  `web` clippy line in `just check`: every use of `dioxus::desktop` is behind the `desktop`
  feature and the screens compile without it. That is the half of the promise this checkout can
  keep today. That core builds for `wasm32` is the other half, and it is the web ring's to keep —
  by gating core's I/O, not by moving core's types.
- **The web ring builds a WebSocket connection factory and a relay**, not a server that runs core
  and a browser that views it. The browser runs core. This is the transport seam's promise kept and
  not a new design.
- **The crate names are fixed**, which closes what ADR 0010 left open. The bundle's name and
  identifier were settled there; the crates' are settled here.

## Consequences

- **Every path moved and every name changed in one commit**, and nothing else did: `core/` to
  `crates/core/`, `desktop/` to `crates/app/`, `inspector_core` to `acp_inspector_core` in every
  `use`, `-p inspector-desktop` to `-p acp-inspector` in every recipe.
  `scripts/check-node-free-desktop.sh` is `scripts/check-node-free.sh`, because its job — prove the
  app crate builds from tracked sources with Node and `dx` blocked — did not change with the name
  and never was about the desktop. The split's stale `phoenix/inspector` paths went in the same
  pass, since it was the last one that would touch them all.
- **`headless.rs` still holds, and now guards a second thing.** The test walks the resolved graph of
  `acp-inspector-core` and fails on any `dioxus*` crate; a web build that reached for a
  `model` crate to get around wasm would have had to weaken it, and the decision above is what
  keeps it as written.
- **`main.rs` has four `#[cfg(feature = "desktop")]` in it** — the `mark` and `shell` modules, the
  title effect, the menu block, and `Spine::other`, which was only ever the menu's — and the
  `desktop` feature is the only feature any `cfg` in the crate names. A fifth is the sign that
  something belongs in `shell.rs` instead.
- **The bundle's name may change, and ADR 0010's open question moves.** That record found `dx` 0.7
  naming the bundle after the crate — `InspectorDesktop.app` — and not after `[application] name`.
  The crate is now `acp-inspector`, so the same run today would produce `AcpInspector.app` or
  `acp-inspector.app`; where `dx` takes the product name from is still owed, and it is owed against
  the new name.
- **The README's "the crate names are not fixed" is retired**, and its sentence about the host
  repository's context map is gone with the map: the inspector kept its own context on the way out,
  which is what [ADR 0001](0001-the-inspector-keeps-its-own-context.md) said would happen.
- **§1 stops calling web unscheduled**, and says instead what shape it takes; §3 describes the
  repository it is in rather than the one it came from; §5's "the desktop crate" is "the app crate".
  Nothing in §6 or §11 changed, because the seam they describe is the one the decision relies on.
- **The web ring inherits three things decided and one owed.** Decided: one crate, core on wasm,
  a relay behind a WebSocket factory. Owed: the gate on core's I/O, and with it the question of what
  `dirs::state_dir()` and the recent-commands file mean in a browser, which is a ring's question and
  not this document's.
