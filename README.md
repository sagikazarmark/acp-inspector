# ACP Inspector

A desktop tool for driving and observing one ACP agent over stdio: point it at an agent command
and it spawns the agent, speaks ACP to it as a well-formed client, and shows you both
conversations at once — the turn and the wire.

The architecture spec is [`docs/architecture.md`](docs/architecture.md); it is where every
decision below is argued. The words it argues in are [`CONTEXT.md`](CONTEXT.md), this project's own
glossary — the inspector was a context of its own inside the repository it began in, and kept it
on the way out ([ADR 0001](docs/adr/0001-the-inspector-keeps-its-own-context.md)). The crate names,
the application bundle's name and its identifier are fixed
([ADR 0010](docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md),
[ADR 0011](docs/adr/0011-one-ui-crate-a-feature-per-renderer.md)).

## Where it is

The frame layer, a shell over it, the typed ACP client, the turn timeline with its inline
permission requests, the capability/auth display, and the JSONL trace export
([spec §14](docs/architecture.md#14-build-order-suggested), all five steps): a connection
factory that spawns an agent over stdio, whole JSON-RPC frames in both directions, a trace that
records every one of them below any typed layer, the agent's stderr in a line-timestamped
diagnostic channel — and above that, a client that drives the spec's cut of v1 (`initialize`,
`session/new`, `session/prompt`, `session/cancel`, `session/list`, `session/load`,
`session/resume`, `session/close`, `session/delete`, `authenticate`, `logout`), decodes what comes back
**as v1**, and keeps a timeline whose entries are id-keyed and a turn state the update stream
drives.

The typed layer decorates the trace; it never replaces it
([§8](docs/architecture.md#8-unknown-traffic-the-raw-first-rule)). Traffic it does not recognize
— an extension method, an update variant v1 has no name for, a client service the inspector
declined by advertising no capability — becomes a first-class unrecognized entry carrying the
frame verbatim, and an agent that calls a declined service is answered method-not-found with the
call recorded as evidence.

The window is a rail and two screens ([ADR 0009](docs/adr/0009-the-window-is-the-drawing.md)):
the connection on the left with the live session's settings and both parties' capabilities behind
two tabs, the turn timeline in the middle, and the console beside it — the trace and the agent's
stderr as two tabs, opening on the trace, with the frame a reader selects read whole in a pane at
the foot of the list, and expanding onto stderr by itself when an agent is gone without being asked
to go. A **Split | JSON-RPC full** control in the toolbar says whether the two screens share the
width or the wire takes all of it; a window too narrow for three columns shows one region at a time
and puts a **Details | Session | Messages** bar at its foot. Every affordance the window has is
reachable by name from a command palette (`Ctrl`/`⌘` + `K`). Launching an
agent runs the whole handshake, so the composer is ready as soon as there is a session; a prompt
streams into the timeline as the agent sends it, tool calls merge their updates into one card, the
stop button sends `session/cancel`, and the turn's state and its `stopReason` are read from the
stream rather than from the prompt call returning.

What the agent claimed in its `initialize` result is a tab of the rail: its name and version,
every v1 capability it did and did not advertise, and the auth methods it offers. Beside it, the
client identity and every Client Capability sent in the request remain equally explicit. The session
lifecycle is shown whole — `session/load`, `list`, `resume`, `close`, `delete` and
`additionalDirectories`, six capabilities agents disagree sharply about, each saying whether this
agent advertised it and none of them left out when it did not. The two shapes an agent makes those
claims in are shown as they are rather than flattened into one notion of support: `session/load` is
a boolean on `agentCapabilities`, the other five are objects under `sessionCapabilities` whose
*presence* is the claim, and each row names the field it was read from. **Affordances
are gated on the advertisement, not on content** — the `session/list` button exists because the
agent said it lists sessions, not because there is anything to list, and the listing it returns
is shown as the agent gave it. A `nextCursor` (no surveyed agent sends one) is carried back
opaquely, never parsed. Authentication is display-only: `authenticate` can be sent with any
advertised method and its outcome is shown, and a `-32000 auth_required` from anywhere becomes a
login-needed state that names the advertised methods and says where to go — the agent's own
login, in the user's own terminal, because the inspector embeds no terminal and never will.

A session can be opened **without relaunching the agent**. `session/new` is the one lifecycle
method the protocol does not gate — every v1 agent opens sessions — so the button sits beside the
capability-gated listing whenever an agent has described itself, and the session it opens becomes
the live one, with the composer ready to prompt into what was just created. Creating one while a
session is already live **is a switch**, and the switch is stated where the button is: a turn still
running is cancelled first, every waiting permission request is answered `cancelled` — the sequence
the stop button already owes them — and the timeline is discarded and rebuilt, because the
inspector holds one live session and the timeline is the view of it. It does not wait for the
cancelled turn to end, since an agent that never ends one is the sort this tool is pointed at; what
answers that turn afterwards answers the call and nothing on screen. **The trace is untouched**:
every frame of the session that was left is where it was, so a switch costs the view and never the
evidence.

**The roots a session opens with are yours to supply**, where the agent advertised
`sessionCapabilities.additionalDirectories`: a list beside the create button and on every row that
reopens a session, one absolute path per line. A reopen's is prefilled with what the listing
reported for that session, so reopening stays a reopen until you change it — and changing it is
allowed, because the schema says a reopen's list may differ from any previously reported one as long
as the `cwd` matches. A relative path is resolved against the *session's* `cwd` before anything is
sent, since a request the schema calls malformed would put this client's own violation into every
annotation drawn against that agent. An empty list puts no field on the wire at all.

And a session the agent listed can be **opened**. A listed session stops being text you can read and
becomes a way into that session: the row carries one button, labelled with the method it will send,
and the session it opens becomes the live one. `session/load` and `session/resume` restore the same
thing — their requests and responses are field-for-field identical — and the one observable
difference is an ordering the specification states: load **must** replay the whole conversation as
Session Updates *before* it answers, and resume **must not** replay it at all. So replay is a
property of the call rather than two buttons: where an agent advertises both, load, because it is the
one that can rebuild the conversation; where it advertises only resume, the screen says plainly that
**this agent does not support viewing previous messages**, because a resumed session's empty timeline
is correct and nobody should have to work that out from an empty screen. Opening one is the same
switch creating one is, and costs the same: the running turn cancelled, the waiting requests answered
`cancelled`, the timeline discarded and rebuilt, the trace untouched. It is discarded *before* the
ask rather than after it, because the replay arrives ahead of the answer.

And a listed session can be **closed** and **deleted**, each where the agent advertised that call
and nowhere else. The two are not one operation with a property the way load and resume are: close
frees a session the agent still holds and delete takes it away, so they are two buttons gated on two
claims. **The listing is asked for again afterwards**, which is the point of pressing either — whether
a closed session is still in a listing is exactly the kind of thing agents differ on and no document
will settle, so what the agent now claims to hold is fetched rather than assumed. Testy's answer:
a closed session leaves its listing, and so does a deleted one.

**The corners the specification leaves undefined are offered, deliberately.** Closing the live
session, closing one that is closed already, deleting one that was never opened — the specification
defines none of them and calls some implementation-defined outright. The inspector sends what it was
asked to send and reports what came back, without saying what should have happened; withholding the
button would be the tool deciding what may be observed. Ending the live session leaves the inspector
with **no live session**, and the screen says so as itself — the timeline goes with the session it
was a view of, the composer says nothing is open to prompt into and names the two ways to open one,
and every frame is still in the trace. A close or a delete the agent *refused* ended nothing: the
session stays live and the failure is named by the method that failed.

**What a `session/close` does to an in-flight prompt was settled by driving it, not by reading**
([§15 q5](docs/architecture.md#15-open-questions--validation-gaps)). Nothing is asked to stop first
— a `session/cancel` sent ahead of the close would answer the question on the agent's behalf — and
what Testy does is answer the close *first*, then resolve the prompt with `stopReason: "cancelled"`.
That is recorded and not graded: the specification states no MUST about the ordering, so none of it
is annotated.

**The second conformance annotation lands here.** A `session/load` that answers having replayed
nothing — into a session this connection has already heard the agent talk in — is annotated where the
traffic is, with the message it did not replay, the load, and the answer open beneath the claim. It
is one shape and deliberately only one: the trace cannot tell an empty session from a withheld
conversation, a refused load restored nothing to replay, and behaviour the specification leaves
undecidable draws no complaint. **Testy is the first real subject** — it answers a load without
replaying, which was confirmed by driving it rather than by reading it
([§15 q5](docs/architecture.md#15-open-questions--validation-gaps)).

Permission requests are answered where the turn stops for them. A `session/request_permission`
becomes a panel **inline in the timeline**, at the point the turn blocked: the tool call under
question, and a button for every option the agent offered, in the order it offered them. Clicking
one sends `selected` with that option id and the turn goes on; pressing stop while a request is
waiting sends `session/cancel` and then answers every waiting request `cancelled`, which is what
a client owes them and the only way an agent blocked on one gets to end its turn. No modal and no
toast: in ACP these are the main traffic of a turn, not an alert beside it.

The trace exports as evidence. **Export** writes every frame the trace holds as JSONL — one
record per frame with its timestamp, its connection, its direction and the frame verbatim, behind
a self-describing header that warns nothing is redacted — and the shell shows the path it wrote.
The schema is pinned in [`docs/trace-export.md`](docs/trace-export.md), because ACP Wiretap and
replay tooling are meant to read it: it aligns with the bridge's `--trace-frames` records field
for field and adds what the inspector knows and the bridge did not record.

The spawn form remembers what answered. An invocation whose agent described itself — a session
opened, or an agent that wants a login before it will open one — joins a short list under the
form: newest first, ten of them, one click to put one back in the four fields, and the Launch
button that was always there to run it. A command that never started said nothing and is not
remembered, because that is a typo and it is still in the fields. It is a convenience and stays one
([§1.1](docs/architecture.md#11-what-is-deliberately-not-built)): nothing to name, edit,
import or export, and the persistent agent catalog is still deferred. The list lives in one JSON
file under the platform's own state directory — `~/.local/state/acp-inspector/recent.json` on
Linux, `~/Library/Application Support/acp-inspector/` on macOS, `%APPDATA%\acp-inspector\` on
Windows, `0600` on Unix because a remembered invocation carries its environment. What it holds and
why is [§15 q6](docs/architecture.md#15-open-questions--validation-gaps).

The window has a light theme and a dark one. A **System / Light / Dark** switch in the top bar
chooses between them; System is the default and follows the operating system, and the choice is
remembered across restarts in `settings.json` beside the recent commands — its own file, ordinary
permissions, and one this cannot read leaves the window on System rather than refusing to open.
Light maps to the purpose-built `inspector-light` daisyUI theme and Dark maps to
`inspector-dark`; System leaves `data-theme` absent so the operating-system preference selects
between them.
The five colours that mean something — what we sent, what the agent said, trouble, live, and
traffic nobody has a name for — mean the same thing in both schemes, so a screenshot stays readable
evidence whichever one it was taken in. Both palettes hold every text colour to 4.5:1 against its
ground and every control boundary, dot and stripe to 3:1. Whether System tracks the operating system is
unverified on a real desktop, along with one known Linux limitation:
[§15 q9](docs/architecture.md#15-open-questions--validation-gaps).

An **Indent** switch sits beside it, off by default and remembered in the same file. On, a payload
that is JSON is drawn with whitespace between its tokens wherever this window draws bytes to be
read — the frames under an entry this window has no rendering for, a tool call's raw input and
output — and **only where somebody is looking**, so a trace of ten thousand frames lays out the one
being read rather than all of them. The Console's own pane is the exception and needs no switch: it holds the
one frame a reader selected, on the surface whose whole subject that frame is, so it is always laid
out and coloured by what it is made of. It **adds whitespace and changes nothing else** — the text is validated as JSON and then
re-spaced rather than decoded and printed back, so an object's keys keep the agent's order, two
fields that share a name both survive, and numbers and escapes stay spelled the way they arrived. A
frame that is not JSON is drawn exactly as it arrived, because a frame can be anything and that is
what this tool is for. And it stops at the reader's eyes: the trace records, the export writes and
every copy takes the bytes that crossed, whatever the switch says.

**Clear** drops the frames captured so far and nothing else — an export already written is
untouched, an export taken afterwards carries only what followed and says in its header how many
frames it never had. The trace keeps its newest 10 000 frames and counts the rest as dropped,
shown beside the frame count; it is never cleared by a disconnect, because an agent that just
died is when its frames are worth the most. Which is why **every row says which connection it
crossed** — `#1`, `#2`, the ordinal the export writes for that same frame — and the row where one
agent gave way to the next draws the boundary: one trace can hold two agents' traffic, and both
agents' JSON-RPC ids start again at 1.

## Layout

```
crates/core/ acp-inspector-core — headless: the connection contract, stdio, the trace,
             the typed ACP v1 client, the JSONL export, the spawn form's recent
             commands, the window's preferences, and the stores they fill
crates/app/  acp-inspector — the screens and the one binary, strictly presentational,
             over those stores; a Cargo feature per renderer (`desktop`, the default;
             `web`, declared and checked, runnable once core builds for wasm32)
             style/  theme.css, retained inspector aliases; input.css, daisyUI
                     production input plus layout and behaviour rules; sheet.css,
                     the generated output, committed and compiled in
CONTEXT.md   the glossary: this project's terms, and nothing else
docs/        architecture.md, the spec; trace-export.md, the export's pinned
             schema; adr/, the decisions that are the inspector's own
scripts/     build-testy.sh, the integration tests' agent; check-node-free.sh
package.json the Tailwind CLI and daisyUI development inputs for the production
             stylesheet — nothing else here is a Node project
```

Two crates, split by layer and not by renderer
([ADR 0011](docs/adr/0011-one-ui-crate-a-feature-per-renderer.md)). The project began inside a
host repository and depended on none of its crates: patterns were copied, never linked
([§13](docs/architecture.md#13-relationship-to-the-host-repos-assets)).

## Running it

```sh
just run      # cargo run -p acp-inspector
```

Fill the spawn form — command, one argument per line, `KEY=VALUE` environment per line, working
directory — and launch. The console at the bottom fills as frames cross; its other tab carries the
agent's stderr, and the console expands onto it if the agent never starts. Once the session is
open, type a prompt and press enter (shift-enter for a newline) to watch the turn stream in.

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
([ADR 0003](docs/adr/0003-the-stylesheet-is-generated-and-committed.md)), so `cargo run` needs no
`dx`, no npm install and no network. Node is needed to **change** the styles, regenerating the
committed sheet from its source with
`just css`, and to **verify** that the committed sheet is what its source generates, which is a step
in CI and in `just check`. Everything else a contributor runs — `cargo run`, `cargo test`,
`cargo fmt`, `cargo clippy` — never asks for it.

### Pointing it at Testy

[Testy](https://github.com/agentclientprotocol/rust-sdk) — the ACP rust-sdk's deterministic stdio
test agent, the same one the integration tests drive — is the best first agent to point it at:
it is scripted rather than a real assistant, so every screen fills with known traffic.

Build it once, from the repository root:

```sh
just testy    # clone/update the rust-sdk checkout under .testy/ and build the binary
```

The binary lands at `.testy/bin/testy`. Then `just run`, and fill the spawn form with:

| Field             | Value                                                                        |
| ----------------- | ---------------------------------------------------------------------------- |
| Command           | the absolute path — `$PWD/.testy/bin/testy` from this directory               |
| Arguments         | empty; Testy takes none                                                       |
| Environment       | empty                                                                         |
| Working directory | any directory that exists, `/tmp` will do — it is what `session/new` is told   |

An absolute path because a relative one resolves against the inspector process's own working
directory, not the form's. Launch, and the handshake runs to an open session: `initialize`,
`session/new`, and a composer ready for a prompt.

Testy's prompts are commands rather than conversation. Each of these is a whole prompt on its own:

- `help` — the commands and scenarios it supports, streamed back as a message.
- `echo hello` — the shortest round trip there is; one turn, one chunk, `end_turn`.
- `session_updates` — every stable `session/update` variant, which is the timeline's whole
  vocabulary in one turn.
- `tool_calls` — tool call creation and updates, the case where many updates merge into one card.
- `wait_for_cancel` — accepts the prompt and then waits, so the stop button has something to
  cancel; the turn ends `cancelled`, read from the stream.
- `callbacks` — the agent-to-client direction: a permission request, then `fs/` and `terminal/`
  calls, and then five elicitations.
- `elicitations` — the five on their own: a form tied to a tool call, a form to decline, a form
  scoped to a request rather than a session, a URL with the completion that follows it, and a URL
  outside any session.
- `full` — all the stable scenarios in order.

The JSON command form works too, and is what the tests send:
`{"command":"run_scenario","scenario":"session_updates"}`.

`callbacks` (and `full`, which runs it) is where the two halves of the raw-first rule are visible
side by side. The permission request and the five elicitations are **serviced**: the turn stops on
each of them, the panel appears inline in the timeline where it stopped, and the turn goes on when
you answer it. A form is built from the schema Testy sent — ten fields, defaults filled in, and a
Raw tab holding the exact object your answer will send; a URL is shown in full with its host, and
opens in your own browser when you click it and not before. The `fs/` and `terminal/` calls between
them are **declined by capability** — this client mediates no filesystem and embeds no terminal
([§7.3](docs/architecture.md#73-decline-by-capability)) — so each is answered method-not-found and
recorded as a first-class unrecognized entry, which is
[§8](docs/architecture.md#8-unknown-traffic-the-raw-first-rule) doing its job rather than a
failure.

Answer them and **the scenario runs to `end_turn`**, which it could not before the fifth ring
([§7.8](docs/architecture.md#78-the-fifth-ring-elicitation)): a client that had not claimed the URL
elicitation mode made Testy fail the whole prompt with `-32602 client does not support URL
elicitation`, and the turn ended with no stop reason at all. Leave one unanswered and the turn stays
stopped, which is the same thing the permission request has always done and the reason the line
above the composer says the agent is waiting on you.

## Working on it

The integration tests drive the same Testy the section above points the shell at. It is
`publish = false`, so it is built from a rust-sdk checkout rather than depended on:

```sh
just testy    # clone/update the checkout under .testy/ and build the binary
just test     # cargo test
just          # both
just check    # fmt, clippy, and that the committed stylesheet is its source's output
just css      # regenerate that stylesheet — the one recipe that needs Node
just node-free # build and test the desktop package offline with Node and dx blocked
just icon     # regenerate the bundle's icon from the window's own mark, on a Mac
just bundle-macos # the .app and its disk image, on a Mac — the one recipe that runs dx
```

`bundle-macos` has `dx bundle` assemble `ACP Inspector.app` from `crates/app/Dioxus.toml` and
Apple's own tools sign, notarize and image it when passed an identity and a `notarytool` profile;
it is made from the build and is no part of it, which is the whole of
[ADR 0010](docs/adr/0010-the-bundle-is-the-one-thing-dx-makes.md). It needs the icon at
`crates/app/bundle/icon.icns` and refuses to run without it. That icon is a tracked packaging asset,
like the stylesheet: `crates/app/src/mark.rs` is the drawing — the same one the window puts in its
own icon — and `just icon` renders it at the ten sizes an `.icns` holds and folds them with
`iconutil`. Change the mark, run `just icon`, commit what it wrote.

Changing the look means editing `crates/app/style/input.css` — or `crates/app/style/theme.css`, which
holds the design vocabulary and the colour tokens it imports — running `just css`, and committing
what it wrote: `crates/app/style/sheet.css` is a tracked build artifact, and `just check` — and CI —
fail when it is not what the source generates.

The complete release check, including both supported window sizes, all Appearance choices, keyboard
operation and representative Agent states, is [`docs/desktop-smoke-matrix.md`](docs/desktop-smoke-matrix.md).

**daisyUI owns ordinary components and themes**
([ADR 0005](docs/adr/0005-daisyui-owns-ordinary-components-and-themes.md)). Use its standard classes
and variants before writing custom control presentation. Inspector Light and Inspector Dark own
light and dark component values; retained custom CSS is for inspector layout, meaning, evidence,
accessibility, ordering or behaviour. [ADR 0004](docs/adr/0004-the-design-vocabulary.md) keeps
semantic, contrast, evidence and motion constraints without imposing the former exact density
scales. The original comparison and measurements remain in
[ADR 0003](docs/adr/0003-the-stylesheet-is-generated-and-committed.md#the-daisyui-verdict).

The retained cross-surface scale is approximately 14px for what the agent and the reader said to
each other, 13px for ordinary content, 12px for dense evidence, 11px for metadata and 10px for the
one register set in tracked capitals — the name of a rail section, the kind of a timeline entry, the
word over a group of capability rows. The shell uses a 44px toolbar, a rail that ranges from 248px
to 296px, approximately 28px ordinary controls and 12px panel gutters. Heroicons Outline is used
sparingly for the agreed toolbar, composer and disclosure controls; its 14px icons sit in standard
compact targets, and every icon-only control has a tooltip and accessible name. Rail, Timeline and
Console yielding, wrapping and internal scrolling are contracted in the stylesheet tests; the smoke
matrix defines manual acceptance at both supported window sizes, where the console stacks under the
timeline rather than beside it.

The checkout tracks rust-sdk `main`, which the spec accepts as a dev-time moving part
([§12](docs/architecture.md#12-dummy-agent-and-test-strategy)). `ACP_RUST_SDK_REV` pins a
revision, `TESTY_BIN` points the tests at a binary built elsewhere.
Use rust-sdk `v2.1.0` or later so Testy itself depends on schema 1.7. For a reproducible schema-1.7
check, build with `ACP_RUST_SDK_REV=v2.1.0 just testy`, then run `just test`.

## License

MIT OR Apache-2.0, dual, at your option — the host repository's license, carried along on the
split.
