# Architecture Specification — ACP Inspector

**Status:** draft, awaiting maintainer review. Assembled 2026-08-01 from the Phoenix wayfinder
effort ([map #48](https://github.com/sagikazarmark/dioxus-chat.orig/issues/48)); every decision
below traces to a closed ticket on that map. This document resolves
[#57](https://github.com/sagikazarmark/dioxus-chat.orig/issues/57), the map's destination
artifact: the spec an implementation effort starts from.

**Scope boundary:** [the MVP cut, #56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56).
The research behind this spec describes a **larger** program than this one — full 25-method v1
coverage, a WebSocket transport, an agent catalog, session load/replay, a web surface, v2. All of
that is charted (as fog or as recorded leanings on the map), none of it is in the MVP, and where
this spec and a ticket disagree, the ticket is right about *what was decided* and this spec is
right about *what the MVP is*.

**Amended 2026-08-02 for the second ring** ([#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75)),
in place rather than by addendum, so that there is one document describing the tool: the session
lifecycle work leaves the deliberately-not-built table
([§1.1](#11-what-is-deliberately-not-built)) and becomes the second ring
([§7.5](#75-the-second-ring-the-session-lifecycle)); the transport seam records what it cannot host
([§6.3](#63-what-the-seam-cannot-host-the-per-session-http-profile)); and validation-versus-observation
is settled by an admission rule ([§15](#15-open-questions--validation-gaps) q7). Everything else
below is the MVP as it was assembled and, where noted, as the build corrected it. The MVP's own
scope boundary above is unchanged — it describes what shipped first, not what the document covers.

**Amended again 2026-08-02 for the third ring** ([#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84)),
in place for the same reason: the two configuration setters leave the deliberately-not-built table
and become the third ring ([§7.6](#76-the-third-ring-session-settings)); gating is re-stated as
**four** shapes rather than two, each named where it is read from ([§7.6](#76-the-third-ring-session-settings));
the MVP's empty client capabilities are **narrowed with their argument** rather than reversed
([§7.3](#73-decline-by-capability)); the annotation list grows by one
([§15](#15-open-questions--validation-gaps) q7); and the terminology leaves this document for a
glossary of the inspector's own ([§2](#2-terminology)).

**Amended again 2026-08-03 for the fourth ring** ([#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96)),
in place for the reason the last two were: the capability panel's rows gain a fourth fact — whether
the advertisement was ever driven and what came back ([§7.7](#77-the-fourth-ring-what-was-driven)) —
and the two advertisements that had no affordance at all, `logout` and `additionalDirectories`,
get one. The deliberately-not-built table's *capability sweep* row **splits rather than clearing**:
the filled-in report is what this ring builds, and the tool-driven pass that would have produced it
is declined outright ([ADR 0002](adr/0002-the-capability-sweep-is-a-record-and-not-a-pass.md)) —
the first standing commitment this document has reversed, which is why it is an ADR and not only a
section. The annotation list does **not** grow ([§15](#15-open-questions--validation-gaps) q7), and
the reason is recorded there rather than left as silence.

**Amended 2026-08-05 for the fifth ring**
([#108](https://github.com/sagikazarmark/dioxus-chat.orig/issues/108),
[#116](https://github.com/sagikazarmark/dioxus-chat.orig/issues/116)): every desktop surface now
uses daisyUI first, Winter and Business own component themes, and the former exact design vocabulary
is retained only where it expresses inspector semantics, evidence, accessibility or behavior
([ADR 0005](adr/0005-daisyui-owns-ordinary-components-and-themes.md)). Core behavior and the domain
language are unchanged.

**Amended 2026-08-05 for the compact native shell**
([#119](https://github.com/sagikazarmark/dioxus-chat.orig/issues/119),
[#120](https://github.com/sagikazarmark/dioxus-chat.orig/issues/120),
[#121](https://github.com/sagikazarmark/dioxus-chat.orig/issues/121),
[#122](https://github.com/sagikazarmark/dioxus-chat.orig/issues/122),
[#123](https://github.com/sagikazarmark/dioxus-chat.orig/issues/123),
[#124](https://github.com/sagikazarmark/dioxus-chat.orig/issues/124),
[#125](https://github.com/sagikazarmark/dioxus-chat.orig/issues/125),
[#126](https://github.com/sagikazarmark/dioxus-chat.orig/issues/126),
[#127](https://github.com/sagikazarmark/dioxus-chat.orig/issues/127)): purpose-built Inspector Light
and Inspector Dark daisyUI themes replace Winter and Business, the toolbar and Agent rail narrow,
the first Heroicons Outline controls establish the icon family, and Session Settings use compact
native controls chosen from the shape of each Config Option. The Console is a flat compact utility
pane, and its collapse plus the Frame and raw-evidence disclosures use the same Heroicons Outline
chevrons. Session lifecycle and roots use flat grouped rows, explicit current, available and stale
states, and the same disclosure chevrons without changing Advertisement gates. System, the native
window frame, native scrollbars, system UI type, generated stylesheet and every behavior above the
presentation seam remain unchanged.

**Amended 2026-08-17 for the drawing**
([ADR 0009](adr/0009-the-window-is-the-drawing.md)): the window is rebuilt to a
drawing, and what moved is where the regions are rather than what any of them is. The Console goes
beside the Timeline instead of under it and a Frame is *selected* rather than expanded, read whole
in a pane at the foot of its list; a left rail holds the Connection and everything scoped to it —
the live Session with its Settings, and the two accounts of what was claimed, behind two tabs — so
the centre screen is the turn and only the turn; the envelope's four shapes gain filter chips beside
the Console's own filter, and a fifth for the frames in none of them; the composer offers the
commands the Agent published and refills the box with one rather than sending it; every affordance
the window has is reachable by name from a command palette; and the palette of the tool itself is
the drawing's — a lime accent that is only ever about the reader, with the two direction colours
swapped to match it. **Rebuilt from the drawing rather than translated into it**: `theme.css` and
`input.css` are written from the design with nothing carried over, the palette is its own values
rather than mixes derived from daisyUI roles, two animations come back for the two things the
drawing animates, and three colours sit below the WCAG AA *text* floor as named, measured exceptions
that every other value is still held to (ADR 0009). Every Advertisement gate, every Affordance,
every sentence and every accessibility contract the rings above argued for is unchanged; §9 below is
amended in place where it names a *place*.

**Provenance of validation:** the Inspector described here is implemented and covered by the core
integration suite, desktop server-rendered component tests and stylesheet contracts; the remaining
platform walkthrough gaps are recorded in
[`desktop-smoke-matrix.md`](desktop-smoke-matrix.md). The external evidence it started from remains:
the MCP Inspector v2 architecture this design copies is shipped, working software
([#49](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)); the ACP v1 method surface
is read from the canonical `schema/v1/meta.json`
([#51](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51)); the liftable prior art in
this repo is shipped and tested
([#53](https://github.com/sagikazarmark/dioxus-chat.orig/issues/53)). See
[§15](#15-open-questions--validation-gaps) for observations that still require a real platform.

**The product name is not fixed.** *ACP Inspector* is the working name from the decomposition
([#55](https://github.com/sagikazarmark/dioxus-chat.orig/issues/55)); product-grade naming
happens when the project splits to its own repo. Crate and binary names below are provisional
the same way the host repo's are. **The application bundle's are not**: `ACP Inspector` and
`dev.sagikazarmark.acp-inspector` are fixed by
[ADR 0010](adr/0010-the-bundle-is-the-one-thing-dx-makes.md), because a bundle identifier is what
macOS keys a user's preferences and permissions to and cannot wait for the split.

This spec sits **beside** the host repo's specs — [`docs/app-architecture.md`](../../../docs/app-architecture.md),
[`docs/bridge-architecture.md`](../../../docs/bridge-architecture.md),
[`docs/architecture.md`](../../../docs/architecture.md) — not on top of them: the inspector
depends on none of the code they describe ([§13](#13-relationship-to-the-host-repos-assets)). It
links rather than restates; research findings are cited by their `docs/research/<name>.md` path
on the corresponding `research/<name>` branch (this repo keeps findings on branches, not `main`).

---

## 1. Scope

**A desktop tool for driving and observing one ACP agent over stdio.** You point it at an agent
command; it spawns the agent, speaks ACP v1 to it as a well-formed client, and shows you both
conversations at once — the *turn* (prompt, streamed updates, permission requests) and the
*wire* (every JSON-RPC frame, both directions, raw). Its subject matter is agent behaviour,
including misbehaviour: traffic the inspector does not recognize is rendered, never dropped.

The MVP is a **single Dioxus desktop app** over a headless **`acp-inspector-core`** crate
([#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56)). It spawns the agent
directly — no tunnel, no launch token, no origin machinery, none of the browser/backend split the
MCP Inspector's web client needs ([#49 §4.3](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)).
Convertibility to a web surface was guaranteed **by the boundary, not built**: core has zero UI
dependencies, so a web surface is additive (MCP Inspector's own v2 pattern). Web is now scheduled,
and the shape it will take is decided rather than open
([ADR 0011](adr/0011-one-ui-crate-a-feature-per-renderer.md)): the same app crate with a second
renderer feature, core compiling for `wasm32` with its I/O behind a gate, and a WebSocket connection
factory behind the seam in [§6](#6-the-transport-seam) — so the browser runs the typed layer, the
stores and the conformance rules itself, and the backend is a relay that spawns the agent and pumps
frames. What that ring builds is its own to specify; this document records the layout it starts from.

### 1.1 What is deliberately not built

Each of these is *decided and deferred (or declined)*, not undecided. The ticket holds the
argument. A row leaves this table when a ring consumes it — the session lifecycle work is the
second ring ([§7.5](#75-the-second-ring-the-session-lifecycle)) and the two configuration setters
are the third ([§7.6](#76-the-third-ring-session-settings)), so neither sits here any longer — and
what a ring defers in turn is recorded beside the MVP's own deferrals rather than left as
silence ([#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75),
[#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84)). A row can also **split**: the
fourth ring builds half of the capability sweep and declines the other half, so that row stays here
with a new reason rather than leaving with a job half done
([#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96)). And a row that names several
things can lose one of them: the fifth ring takes `elicitation/*` out of the declined client
services and leaves the other two where they were, which is a row losing an item rather than a
decision being reversed for all three
([#133](https://github.com/sagikazarmark/dioxus-chat.orig/issues/133)).

| Not built | Status | Where decided |
|---|---|---|
| Persistent agent catalog with import/export | Later; MVP keeps at most a recent-commands convenience | [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56) |
| The capability sweep — a tool-driven pass that exercises every advertised method | **Declined**, not deferred again. The *filled-in report* half of this row is what the fourth ring builds, passively ([§7.7](#77-the-fourth-ring-what-was-driven)); the pass that would drive it is refused, because a tool that manufactures the traffic its subject is judged on has made the record harder to read, and because a report that outlives its frames is the verdict [§15](#15-open-questions--validation-gaps) q7 exists to refuse. The row's old reason was timing and had expired — rings two and three made the methods reachable and the shape was still wrong | [ADR 0002](adr/0002-the-capability-sweep-is-a-record-and-not-a-pass.md), [#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96) |
| Non-PDF binary resource attachments and resource references | Deferred. Picker/drop accept signature-identified PNG/JPEG/GIF/WebP, WAV/MP3, PDFs and UTF-8 text files in one ordered draft: eight rows, 5 MiB each, 6 MiB aggregate, optional prompt text first. Clipboard images join the same draft. Each kind is gated on its own `promptCapabilities.image`, `.audio` or `.embeddedContext` Advertisement; its row records the driven outcome. Text uses embedded resource contents, `text/plain`, preserved BOM/line endings and a literal 2000-character preview. PDFs use binary resource contents, `application/pdf` and original bytes in base64, identified by a PDF 1.0–1.7 or 2.0 header line; no PDF renderer or document validation. Both resources use escaped native file URIs or distinct browser attachment URIs. Embedded context is an Advertisement of content shape, not PDF interpretation. Invalid UTF-8/binary controls in text and other recognized binary formats are refused. Audio controls never autoplay and playback failure does not prevent sending. Complete serialized prompt Frames remain capped at 10 MiB. | [#11](https://github.com/sagikazarmark/acp-inspector/issues/11), [#12](https://github.com/sagikazarmark/acp-inspector/issues/12), [#13](https://github.com/sagikazarmark/acp-inspector/issues/13), [#14](https://github.com/sagikazarmark/acp-inspector/issues/14), [#15](https://github.com/sagikazarmark/acp-inspector/issues/15), [#17](https://github.com/sagikazarmark/acp-inspector/issues/17) |
| Persistent MCP catalog | Deferred. Stdio/HTTP/SSE definitions use one structured, in-memory editor before Launch and in Sessions. Stdio is baseline ACP; HTTP and SSE are independently gated on `mcpCapabilities.http` and `.sse`. Drafts for the next Launch may name either before initialize; syntax is validated before connecting, then the snapshot's transports against the new Agent before opening its first Session. Unsupported definitions remain visible with the Connection available for correction. All new/load/resume routes use the shared draft and validate before switching; reconnect retains it, recent commands and persistent storage do not. Ordered arguments, environment and headers preserve empty values and duplicates. The inspector neither checks executable existence nor fetches URLs. Capability outcomes refer to the Session-opening call, never individual MCP connectivity. New/load serialize an empty list; resume omits it under the pinned schema. | [#18](https://github.com/sagikazarmark/acp-inspector/issues/18), [#19](https://github.com/sagikazarmark/acp-inspector/issues/19) |
| Connecting to more than one agent at once | Deferred, and wants research before a build: a client legitimately holds many MCP servers because that is MCP's normal topology, whereas ACP's is one client and one agent. Whether the shape transfers is an open question, not an assumed yes | [#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75), [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84) |
| Making the client's boolean-config-option claim configurable — a spawn-form switch for what the inspector advertises about itself | Deferred, and the observation it would unlock is real: an agent that degrades its offer for clients which did not claim the capability would be watchable doing it. Declined here anyway, because the first configuration knob on this tool is a decision of its own and this ring should not smuggle one in. The consequence is accepted and stated in [§7.6](#76-the-third-ring-session-settings): a candidate annotation that can never arm is not on the rule list | [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84) |
| A fourth fact for the client's *own* claims — whether any agent took one up on this connection | Deferred with a ring of its own owed. [§7.7](#77-the-fourth-ring-what-was-driven)'s fourth fact says *this tool drove the agent's advertisement and here is what came back*; the client-side analogue says *an agent used what this tool claimed*, which is a different sentence about a different party and needs its own store and its own wording. The fifth ring is what makes it worth having — it takes the client's claims from one to three ([§7.8](#78-the-fifth-ring-elicitation)) — and is exactly why it is not built there: a ring that adds the claims should not also be the ring that grades their uptake | [#133](https://github.com/sagikazarmark/dioxus-chat.orig/issues/133) |
| `session/set_model` | **It does not exist.** There is no `models` field on a session setup response and no such method in the schema — config options superseded them. Recorded so that a reader looking for it stops looking | [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84) |
| `session/fork`'s `modes` and `configOptions` | Not decoded. Fork stays on the raw-first path ([§7.5](#75-the-second-ring-the-session-lifecycle)), so the settings on its response are traffic to read rather than a session setup response to fill the store from | [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84) |
| Logs-as-a-tab beyond the Console's two | Later, and with a rule now that there is a panel to put one in ([§9](#9-screens)): **a Console tab holds what the transport seam produced about one Connection, below the typed layer** — the frames as they crossed, everything else the transport said, and where it says the record has a hole. That is what the Trace and the diagnostic channel have in common, it is why the Timeline is not in the panel — an entry there is what the typed layer *made* of frames, not the frames — and it is the test any third tab has to pass. The rule said *captured* until an audit found that nothing captures a dropped frame, a dropped stderr line, or the trace ring's dropped count, all three of which are in the panel and belong there ([§8](#8-unknown-traffic-the-raw-first-rule)): the boundary is the subject, not the voice, and the tool's own voice is admitted about the integrity of the record and about nothing else. A log about the inspector's own working — a store updated, a file written, a render — is about this tool and not about a connection, and would be somewhere else or nowhere | [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56), [#95](https://github.com/sagikazarmark/dioxus-chat.orig/issues/95) |
| WebSocket transport (remote agents) | Later — a second connection factory behind the seam in [§6](#6-the-transport-seam); the transport RFD's `/acp` upgrade slots there | [#51](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51), [#52](https://github.com/sagikazarmark/dioxus-chat.orig/issues/52), map fog |
| Full 25-method v1 coverage | Later — mechanical enumeration against the coverage checklist ([§7.4](#74-the-coverage-yardstick)) | [#51](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51), map fog |
| v2 support and dual-version decoding | Later — v2 remains a draft with no stated timeline and nothing published since 2026-07-28, when upstream instead *removed* its own version-conversion helpers citing unrepresentable states. The seams stay open ([§11](#11-v2-seams-kept-open)) and nothing is decoded as v2. Its folding of modes into config options — `session/set_mode` gone, a `category` on a config option in its place — is one more thing the seams have to survive rather than something built for | [#51](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51), [#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75), [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84), map fog |
| `fs/*` and `terminal/*` client services | Declined by advertising no capability for either, and **still declined** now that the client claims several things about itself ([§7.3](#73-decline-by-capability)): these are *services* the inspector genuinely cannot perform — there is no filesystem mediation to offer and no terminal embedded. Arriving calls are logged and answered method-not-found. `elicitation/*` left this row with the fifth ring ([§7.8](#78-the-fifth-ring-elicitation)): it had been declined in the same sentence on the strength of one clause about what this tool *had built* rather than what it can do, and a schema-driven form is a thing it draws | [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56), [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84), [#133](https://github.com/sagikazarmark/dioxus-chat.orig/issues/133) |
| Authentication beyond `authenticate` and `logout` | `authMethods` shown, `-32000 auth_required` surfaced as a real state; every practical login runs in the user's own terminal. `logout` leaves this row with the fourth ring ([§7.7](#77-the-fourth-ring-what-was-driven)) — it was a drawn capability row with no method anywhere in the tool, which is the gap the ring's record made impossible to keep quiet | [#51 §2](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51), [#54](https://github.com/sagikazarmark/dioxus-chat.orig/issues/54), [#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96) |
| CLI / TUI surfaces | Out of MVP | [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56) |
| Observing *third-party* client↔agent pairs (MITM) | **Never in this project** — that is ACP Wiretap's charter; the inspector event-logs only its own traffic | [#55](https://github.com/sagikazarmark/dioxus-chat.orig/issues/55) |
| Primitive-catalog tabs as the UI's center, HTTP/OAuth/Network stack, per-frame replay, MCP Apps sandbox | **Never** — MCP Inspector features with no ACP analog | [#49](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49), [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56) |

## 2. Terminology

**The terms live in [`CONTEXT.md`](../CONTEXT.md), not here.** The inspector is a separate project
destined to split out ([#55](https://github.com/sagikazarmark/dioxus-chat.orig/issues/55)), so it
keeps a context of its own — a glossary beside its code, decisions in its own
[`docs/adr/`](adr/), and a [context map](../../../CONTEXT-MAP.md) at the repository root naming both
this context and the host chat library's. Nothing of the inspector's is written into the host repo's
`CONTEXT.md`, which was true from the first draft of this document and is now written down as a
decision ([ADR 0001](adr/0001-the-inspector-keeps-its-own-context.md)).

This section held those definitions until the third ring
([#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84)) found *Capability* meaning three
things at once and *Mode* meaning two — which is what a document that defines terms while using them
cannot see. The glossary holds terms and nothing else; this document holds the decisions and states
them in those terms, and defines none of them here — including the two the third ring introduces,
*Session Settings* and *Advertisement*, which [§7.6](#76-the-third-ring-session-settings) argues for
and the glossary defines.

## 3. Where it lives

Decided in [#55](https://github.com/sagikazarmark/dioxus-chat.orig/issues/55), and since carried out:

- **Its own repository, `acp-inspector`.** It began as `phoenix/inspector/`, a top-level root in the
  host repo that was its own excluded Cargo workspace, destined to split out with no fixed trigger.
  The split has happened; everything the inspector needs came with it, which is what the layout
  below was for all along.
- **Dependencies: the official ACP schema crate only** (`agent-client-protocol-schema` — pure
  types, `v1::`/`v2::` modules). **No dependency on `dioxus-agent-ui*`.** The mapping crate
  (`crates/dioxus-agent-ui-acp`) and the demo's wire panel were copied/adapted as *prior art*,
  never linked ([§13](#13-relationship-to-the-host-repos-assets)). The full `agent-client-protocol`
  SDK crate is not used: the inspector's identity is raw-first frame handling below any typed
  layer, and the SDK's client machinery sits above exactly the seam the trace decorator needs to own.
- **License:** MIT OR Apache-2.0 dual, matching the host repo it came from.

Two crates inside the workspace, split by layer and not by renderer
([ADR 0011](adr/0011-one-ui-crate-a-feature-per-renderer.md)):

```
acp-inspector/
├── Cargo.toml            # the workspace
├── package.json          # Tailwind and daisyUI inputs for the production stylesheet
├── docs/architecture.md  # this spec
└── crates/
    ├── core/             # `acp-inspector-core`: headless, zero Dioxus/UI deps
    └── app/              # `acp-inspector`: the screens and the one binary,
                          # a Cargo feature per renderer (`desktop` today, `web` next)
```

The `package.json` is at the repository root rather than in `crates/app/`, which is where it was
put so that everything the inspector needs travelled with it on the split; what it is for, and why
a Node toolchain does not make Node a dependency of a build, is
[ADR 0003](adr/0003-the-stylesheet-is-generated-and-committed.md).

## 4. Topology

```
 ┌──────────────────────────────┐
 │   inspector (Dioxus desktop) │
 │  ┌────────────────────────┐  │      spawn, stdio, NDJSON
 │  │     acp-inspector-core     │──┼──────────────────────────▶ ┌───────┐
 │  │  connection · stores   │  │   ◀── stdout: JSON-RPC ──  │ agent │
 │  │  trace · pending reqs  │  │   ◀── stderr: diagnostics  └───────┘
 │  └────────────────────────┘  │
 │    UI reads stores, renders  │
 └──────────────────────────────┘
```

**Two processes, one artefact we build.** The inspector spawns the agent as a direct child —
stdio transport, newline-delimited JSON-RPC, stderr piped into the diagnostic channel. No bridge,
no backend, no port. This is the MCP Inspector's CLI/TUI path — the one its architecture proves
works without the remote layer ([#49 §5](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)).

## 5. The core/UI split

The load-bearing design, copied from MCP Inspector v2's `InspectorClient` + observable state
stores ([#49 §4.1](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)) and mapped onto
Rust/Dioxus:

**`acp-inspector-core`** (headless, no Dioxus, no UI types) owns:

- **Connection lifecycle** — construct a connection from a factory ([§6](#6-the-transport-seam)),
  run `initialize`, tear down; spawn failure surfaces through the diagnostic channel, not as a
  silent hang.
- **The typed ACP client layer** — issue the driven methods, correlate JSON-RPC ids, decode
  incoming frames into typed events *where recognized* ([§8](#8-unknown-traffic-the-raw-first-rule)).
  JSON-RPC correlation machinery is copy-adapted from `app/web/src/acp/wire.rs`, which #53 found
  host-testable and transport-shaped for exactly this.
- **Observable state stores** — connection status, agent info/capabilities/authMethods (from
  `initialize`), the session's timeline entries, turn state, the live session's Session Settings
  ([§7.6](#76-the-third-ring-session-settings)), what of the agent's advertisements has been driven
  and what came back ([§7.7](#77-the-fourth-ring-what-was-driven)), the trace log, the diagnostic
  (stderr) log, the blocking requests nobody has answered — permission requests and elicitations,
  two lists ([§7.8](#78-the-fifth-ring-elicitation)). The UI subscribes; core never renders.
- **Pending-request objects** — an agent-initiated `session/request_permission` or
  `elicitation/create` ([§7.8](#78-the-fifth-ring-elicitation)) becomes a value
  holding the request plus a resolver; the UI renders it, the user's choice resolves it, core
  sends the JSON-RPC response. The MCP Inspector's pending-client-request machinery
  ([#49 §5](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)) — with the presentation
  moved inline into the timeline, because in ACP these are the main traffic of a turn, not an
  exceptional alert.
- **Conformance annotations** ([§15](#15-open-questions--validation-gaps) q7) — where the
  specification says **MUST** and captured frames decide it, an annotation is produced here, beside
  the traffic it is about, and asserted without a window. In core because that is where the frames
  are and because the wording is a specification claim rather than a rendering decision: a second
  surface says the same sentences.
- **The JSONL exporter** ([§10](#10-the-jsonl-trace-export)).

**The app crate** is strictly presentational: props in, callbacks out, no protocol logic. If
a behaviour cannot be tested without a window, it belongs in core. It is one crate for every
renderer, with a Cargo feature to pick one, and the desktop shell — the frame, the title bar, the
menu strip, the icon — is the one module in it that imports `dioxus::desktop`, gated on the
`desktop` feature ([ADR 0011](adr/0011-one-ui-crate-a-feature-per-renderer.md)).

## 6. The transport seam

Decided in [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56): the seam lives in
`acp-inspector-core` as a **connection factory yielding framed JSON-RPC messages, not raw bytes**.

### 6.1 The connection contract

A factory produces a connection: **(outgoing frame sink, incoming frame stream, diagnostic
side-channel)**. Framing is the transport's job; everything above the seam deals in whole JSON
messages. The MVP ships exactly one factory:

- **`StdioSpawn { command, args, env, cwd }`** — spawns the child, does newline-delimited
  framing on stdin/stdout, pipes stderr line-by-line into the diagnostic channel. The four
  spawn fields are the MCP Inspector's, unchanged
  ([#49 §2](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)).

WebSocket later is a **second factory**, not a redesign: WS is natively message-framed, and the
transport RFD's `/acp` upgrade (Active, explicitly additive to v1) slots behind the same
contract ([#51 §3](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51)). The native
proxy/conductor RFDs are message-level middleware and do not change this seam
([#52](https://github.com/sagikazarmark/dioxus-chat.orig/issues/52)). The *other* half of that
RFD — the Streamable HTTP profile — does not slot in as cleanly, and
[§6.3](#63-what-the-seam-cannot-host-the-per-session-http-profile) records why.

### 6.2 The trace decorator

A decorator wraps the connection's frame streams and records **every frame with direction and
timestamp before the typed layer sees it** — the MCP Inspector's `MessageTrackingTransport`
pattern, which logs the true wire frame no matter what happens above it
([#49 §4.2](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)). This is the **single
capture mechanism**: the trace view, the JSONL export, and misbehaving-agent logging are three
consumers of one log. Nothing else in the program touches the wire.

### 6.3 What the seam cannot host: the per-session HTTP profile

Recorded by [#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75) so that a later
remote-agent effort finds the finding rather than rediscovering it. **The seam as specified above
is incomplete for one transport, not wrong**, and nothing here is built to fix it.

The transport RFD's Streamable HTTP profile — one `/acp` endpoint, POST plus long-lived GET
streams, `Acp-Connection-Id` and `Acp-Session-Id` carried as transport-level headers
([#51 §3](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51),
[#54 §2](https://github.com/sagikazarmark/dioxus-chat.orig/issues/54), the RFD read at Active) —
**keys its event stream by session identity, below the JSON-RPC layer**
([#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75)'s reading of the profile; the
header names are quoted above from the research, the keying is not re-read here). That identity is
minted *above* the seam — it arrives in a `session/new` response, which only the typed layer
decodes — while the factory below the seam is handed nothing but whole JSON messages and has, by
construction, no idea what any of them mean. A factory speaking that profile therefore cannot know
which stream to open.

Two escape hatches are known, and neither is chosen here:

- **A second frame parser ahead of the trace decorator**, so the transport reads the session id out
  of the traffic itself. It costs a second place in the program that decides what a frame is,
  sitting in front of the one mechanism [§6.2](#62-the-trace-decorator) exists to keep singular.
- **A small upward channel announcing a created session** to the transport. It costs a protocol
  concept below a seam that today knows only whole JSON messages — narrower than a parser, and still
  a crack in the framing-is-the-transport's-job rule.

The WebSocket half of the RFD is unaffected: one socket per connection, natively message-framed,
no session-keyed stream to open. Which hatch is right — if either — belongs to the effort that
actually builds a remote transport, and it should be decided against a real endpoint rather than
here.

## 7. The procedure cut

[#51](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51)'s proposed 9-method subset,
adopted verbatim by [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56). The
capability mechanism is the client's friend: whatever the inspector does not advertise, a
conformant agent MUST NOT call.

### 7.1 Driven (client → agent)

| Method | MVP behaviour |
|---|---|
| `initialize` | `protocolVersion: 1`, honest `clientCapabilities` — empty at MVP, and narrowed to one claim by the third ring ([§7.3](#73-decline-by-capability), [§7.6](#76-the-third-ring-session-settings)) — and `clientInfo`. Its result is the first display artifact: agent info, capabilities, auth methods ([§9](#9-screens)) |
| `session/new` | `cwd` from the spawn form, `mcpServers: []`; `additionalDirectories` from a control the fourth ring adds, where the agent advertised it ([§7.7](#77-the-fourth-ring-what-was-driven)) |
| `session/list` | Only when `sessionCapabilities.list` advertised; paging ignored (no surveyed agent emits `nextCursor`), cursor treated as opaque if one appears. The listing the tool refreshes on its own initiative after a close or delete still reports a failure in the trace and nowhere else ([§7.5](#75-the-second-ring-the-session-lifecycle)) — it is not a call anybody asked for, so it writes no record ([§7.7](#77-the-fourth-ring-what-was-driven)) |
| `session/prompt` | Text-only `ContentBlock[]` composer; render the `stopReason` |
| `session/cancel` | A stop button; the prompt must still resolve with `stopReason: "cancelled"` — a conformance point worth watching in the UI, and one of the rules the inspector annotates ([§15](#15-open-questions--validation-gaps) q7) |
| `authenticate` | Show `authMethods`, let the user pick, surface `-32000 auth_required` as a real "log in" state. No terminal auth — the UI says "run the agent's login in your own terminal". The auth methods are an Advertisement and the buttons gate on them, so this is driven and recorded like the rest — in `AuthState`, which already holds not-driven, accepted-with-a-method, and refused-with-the-agent's-error, and is not duplicated into a second store ([§7.7](#77-the-fourth-ring-what-was-driven)) |
| `logout` | A button in the agent panel wherever `agentCapabilities.auth.logout` is advertised, including where nobody has logged in — the corner is undefined and that is the reason to drive it. Connection-scoped: the request carries no session id. It answers `{}`, so it licenses no claim about where authentication stands; a success returns `AuthState` to *unasked*, because the agent has retracted what it accepted, and does not make the login screen appear ([§7.7](#77-the-fourth-ring-what-was-driven)) |

### 7.2 Serviced (agent → client)

| Method | MVP behaviour |
|---|---|
| `session/update` | The heart of the inspector: every notification shown raw *and* typed where known. Every stable v1 `sessionUpdate` variant decoded — **eleven**, not the twelve this document first said (see below); unknown variants rendered as unrecognized entries ([§8](#8-unknown-traffic-the-raw-first-rule)) |
| `session/request_permission` | Interactive and **inline in the timeline at the point the turn blocks**: render the tool call and the four option kinds (`allow_once`, `allow_always`, `reject_once`, `reject_always`), respond `selected {optionId}`; auto-respond `cancelled` when the turn is cancelled |
| `elicitation/create` | The second blocking request, added by the fifth ring and specified there ([§7.8](#78-the-fifth-ring-elicitation)): a form built from the agent's own schema or a URL shown in full, answered `accept`, `decline` or `cancel`, inline where the agent asked it |
| `elicitation/complete` | Swallowed onto the entry whose `elicitationId` it names ([§7.8](#78-the-fifth-ring-elicitation)). A notification, so nothing answers it; one naming an id nobody issued is ignored and shown ([§8](#8-unknown-traffic-the-raw-first-rule)) |

An option that was activated remains the same focused button while the answer is sent and recorded.
It becomes ARIA-disabled, leaves future Tab order, and guards repeat activation rather than becoming
natively disabled and dropping keyboard focus to the document.

> **Eleven stable variants, corrected by the build.** The twelve this document first said came
> from the surface research's reading of the schema metadata ([§7.4](#74-the-coverage-yardstick)).
> `agent-client-protocol-schema` 1.7 exposes **eleven** `v1::SessionUpdate` variants outside its
> `unstable_*` features; the four it hides (`plan_update`, `plan_removed`, behind
> `unstable_plan_operations`, and `compaction_update`, `compaction_summary_chunk`, behind
> `unstable_session_compaction`) are traffic this client does not claim to understand, so they are
> deliberately not decoded and land in the raw-first path like any other unknown variant
> ([§8](#8-unknown-traffic-the-raw-first-rule)). The count is asserted in both places it is
> claimed: `crates/app/src/update.rs` renders eleven and its test constructs each one, and
> `crates/core/tests/typed.rs` counts the same eleven off a real agent. Both matches end in a `_` arm,
> so the day the schema stabilizes a twelfth it says so rather than silently swallowing it.

### 7.3 Decline by capability

`fs/*` and `terminal/*`: declined by advertising no capability for either of them.
A conformant agent then never calls them — but a misbehaving agent is the inspector's subject
matter, so if such a call arrives anyway it is **logged in the trace as first-class evidence and
answered method-not-found**, never silently swallowed.

> **The MVP's empty client capabilities are narrowed here, not abandoned**
> ([#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84)). The stance the sentence above
> states was always about **services** — work the inspector would have to *perform* on the agent's
> behalf, and genuinely cannot: it has no filesystem mediation to offer and embeds no terminal
> ([§7.1](#71-driven-client--agent)). Both of those stay declined.
>
> **And the third name this list used to carry was not one of them**
> ([#133](https://github.com/sagikazarmark/dioxus-chat.orig/issues/133)). `elicitation/*` was
> declined in the same breath, on the strength of one clause — that this tool *renders no
> elicitation form* — which was a fact about what had been built rather than about what it can do.
> It draws a schema-driven form on the settings screen already. Elicitation is serviced in both
> modes and claimed as what it is, a service this client performs
> ([§7.8](#78-the-fifth-ring-elicitation)); the rule below is what decided that it could be, and
> what it is still held to.
>
> **A data shape the inspector renders is a different question, and it was answered wrongly.** This
> section originally counted config options among the declined, which conflated the two: boolean
> config options are gated by a *client* capability
> (`clientCapabilities.session.configOptions.boolean`), and refusing to claim one does not spare the
> inspector any work — it changes what the subject offers. A surveyed agent ships an option as a
> boolean to clients that claimed the capability and degrades it to a two-value select for everyone
> else, so the inspector was silently reshaping the behaviour it exists to observe, by a claim it had
> never examined, with nothing on screen saying so. For a tool whose thesis is that claims should be
> legible, that was the dishonest answer rather than the honest one. The inspector therefore claims
> **boolean config options and nothing else** ([§7.6](#76-the-third-ring-session-settings)), fixed
> for every connection, and shows the claim beside the agent's ([§9](#9-screens)).
>
> The rule the two halves share is unchanged: **the inspector claims exactly what it will honour.**
> A service it cannot perform is not claimed; a data shape it will render is. Neither half is a
> guess about what an agent would like to hear.

### 7.4 The coverage yardstick

"Complete coverage later" is measured against the 25-method stable v1 table in
`docs/research/acp-v1-surface-and-v2-delta.md` (branch `research/acp-v1-surface-and-v2-delta`),
which reads it from the canonical `schema/v1/meta.json` — coverage is tracked against the
schema's method list, not against this spec.

**Schema 1.7 audit, 2026-09-17** ([#4](https://github.com/sagikazarmark/acp-inspector/issues/4)).
The [1.7.0 changelog](https://github.com/agentclientprotocol/agent-client-protocol/blob/v1.7.0/CHANGELOG.md)
stabilizes elicitation and terminal authentication; making `schemars` optional leaves it in the
default features this workspace uses. Compared with
[`v1.6.0/schema/v1/meta.json`](https://github.com/agentclientprotocol/agent-client-protocol/blob/v1.6.0/schema/v1/meta.json),
[`v1.7.0/schema/v1/meta.json`](https://github.com/agentclientprotocol/agent-client-protocol/blob/v1.7.0/schema/v1/meta.json)
adds only `elicitation/create` and `elicitation/complete` — already serviced under §7.8, which read
the canonical stable list ahead of the crate's release. The released list now has the same 25
methods: thirteen agent methods, eleven client methods and `$/cancel_request`.

- **The eleven decoded `session/update` variants are unchanged.** The new `compaction_update` and
  `compaction_summary_chunk` variants remain unstable and raw-first under §8, alongside
  `plan_update` and `plan_removed`; no unstable feature is enabled.
- **Terminal authentication adds a Client Capability, not a method.** Schema 1.7 serializes
  `clientCapabilities.auth.terminal: false` by default. The inspector displays that declined claim
  in its own `auth` group, separately from `terminal/*`, under §7.3's rule that it claims exactly
  what it will honour. §7.1's own-terminal login rule still applies: the inspector cannot reproduce
  the Agent invocation in an interactive terminal. The schema can now decode terminal auth entries
  on `initialize`; the Trace retains their full data and the existing auth-method list displays
  their names, without launching a terminal.
- **The capability-coverage tripwire changes only for that new `auth.terminal` claim**, and the
  wire expectations include its default `false`. The elicitation tests are unchanged. Testy built
  from rust-sdk [`v2.1.0` (`726c503`)](https://github.com/agentclientprotocol/rust-sdk/tree/726c503),
  itself using schema 1.7.0, runs both the `callbacks` and `full` scenarios to `end_turn`.

### 7.5 The second ring: the session lifecycle

Decided in [#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75), which re-cuts the
value order [§7.4](#74-the-coverage-yardstick) first proposed for this ring. **Every session
lifecycle method ACP v1 defines as stable becomes an affordance**, each gated on the agent's own
advertisement and on nothing else — including where the inspector believes the operation cannot
succeed, because an agent's behaviour in a corner the specification leaves undefined is exactly
what a user came here to discover
([§7.3](#73-decline-by-capability)'s gating rule, extended):

| Method | Second-ring behaviour |
|---|---|
| `session/new` | Callable on demand, not only as part of launching. The handshake keeps its shape and the new affordance is a second caller of the same operation; the session it makes becomes the live one |
| `session/list` | The listing becomes *actionable* — a listed session is a way into that session rather than text to read — and refreshes after a close or a delete. Paging keeps driving the opaque cursor as it does today ([§7.1](#71-driven-client--agent)) |
| `session/load` / `session/resume` | **One screen with a replay property**, not two buttons. Their request and response types are field-for-field identical and the sole observable difference is that load MUST replay the conversation as `session/update` notifications before answering and resume MUST NOT. Where both are advertised, load is preferred; where only resume is, the screen says plainly that this agent cannot show previous messages, so an empty timeline reads as correct |
| `session/close` | Exercisable when advertised, and what the agent does with an in-flight turn is observed rather than assumed |
| `session/delete` | Exercisable when advertised; whether the session then leaves a subsequent listing is the thing worth watching |

The inspector holds **one live session at a time**. Opening another discards the timeline and
rebuilds it from whatever the agent replays — which is what `session/load` exists to make possible —
after cancelling any in-flight turn and answering every waiting blocking request `cancelled` —
every permission request, and every elicitation whatever it was scoped to
([§7.8](#78-the-fifth-ring-elicitation)). The
trace is untouched by any of this: it keeps every frame from every connection, so a switch costs the
view and never the evidence.

> **Load and resume are built, as one operation with a property.** `Restore` is that property
> (`crates/core/src/restore.rs`): which call an agent's advertisement entitles the inspector to make, and
> whether it replays. The two requests are sent from one place with one difference between them, and
> the session's own `cwd` and reported roots are what is handed back by default. **That last clause
> was stated as a protocol constraint and is not one**, which the fourth ring corrects: the schema
> says a reopen's list *"may differ from any previously used or reported list as long as the request
> `cwd` matches the session's `cwd`"*, so declining to send a different one was this tool deciding on
> the agent's behalf what it should be asked — the move the rest of this ring exists to forbid. The
> reported roots stay the default because a reopen should reopen; the user may now change them where
> the agent advertised the capability ([§7.7](#77-the-fourth-ring-what-was-driven)). One thing is
> ordered differently from a `session/new` switch and it is the point of the call: **the timeline is
> discarded before the ask, not after it**, because a load replays *ahead* of its answer and a view
> emptied afterwards would be emptied of the replay. What that costs is stated rather than hidden:
> whatever the session being left says on its way out lands in the view being rebuilt.
> **Resume is discarded before the ask too**, though nothing resume does requires it — a resume that
> replays anyway has broken a MUST NOT, and a view emptied after the answer would throw those
> updates away, showing less than the agent sent
> ([§8](#8-unknown-traffic-the-raw-first-rule)). One rule for both is also what makes them one
> operation on screen. It costs nothing where there was no session to leave, which is `session/new`'s
> rule unchanged: a connection that changed keeps what the last agent said. A refused open leaves the
> session it could not replace live, with whatever was replayed before the refusal still readable —
> and an open that could never be *asked* for, on a connection already gone, costs the view nothing
> at all, because the frame is prepared before anything is given up.

> **Close and delete are built, and they are two operations rather than one with a property.**
> Close frees a session the agent still holds; delete takes it away. That is a difference in what
> the agent *does*, unlike `session/load` and `session/resume`, whose one difference is an
> ordering — so they are two affordances behind two claims, and what they share is only what
> happens on this side afterwards (`Client::end_session`). Each is gated on its own advertisement
> and on nothing else, including where the inspector believes the call cannot succeed.
>
> **The listing is asked for again after either**, from the start rather than from a cursor,
> because a fresh question deserves a fresh answer and the last one is the agent's old news. It is
> asked whatever the agent answered, including a refusal — an agent that declined to close a
> session and dropped it anyway is a finding this could only hide by not looking. This is the one
> call the inspector makes that nobody clicked, so it is the one place an advertisement gates the
> *sending* rather than the affordance: an agent that never claimed `session/list` is not asked a
> question of the tool's own invention.
>
> **Ending the live session leaves no live session, and that reads as itself.** The timeline is the
> view of the live session, so it goes with it; its empty screen says nothing is open to prompt
> into, names the two ways to open one and carries the control that does — and the composer, which
> has nothing to prompt into, is not drawn at all; the trace keeps every frame. Nothing is asked to stop first
> — a `session/cancel` ahead of the close would answer the question the close was sent to ask —
> so the turn is let go of rather than cancelled, and what the agent does with it lands in the
> trace like everything else. Every waiting blocking request is still answered `cancelled`,
> because that debt is to *them* and it comes due when this client stops being the one that
> answers them; it is paid after the call is answered, so it cannot be mistaken for a client
> cancelling ahead of it. **A session the agent would not end is a session that did not end**: the
> refusal is the caller's answer and the live session stays live, which is the rule a refused open
> already follows.
>
> **An agent that goes on talking about a session it closed is shown doing it**, because showing
> less than the agent sent is the one thing this layer may not do
> ([§8](#8-unknown-traffic-the-raw-first-rule)) — and *having no live session* is said by the
> session store and by the composer reading it, never by the timeline happening to be empty, which
> is what lets both be true at once. What an agent says in the moment between the answer and the
> discard goes with the view, which is the cost a switch already states.
>
> **A turn that fails after its session ended says nothing on a screen that has none.** The answer
> to a prompt nobody is listening for reaches its caller and the trace; the turn state is the live
> session's, so a failure whose turn was walked away from is not written over whatever is live now.
> The guard is the id the turn went out under, which every way of leaving a session empties.
>
> **No annotation is drawn by any of it** ([§15 q7](#15-open-questions--validation-gaps)). There is
> no MUST anywhere in this ring's undefined corners, and the admission rule requires one.
>
> **Both affordances hang off a listing row**, which is where a session the agent claims to hold is
> named — so an agent that advertises close or delete without advertising `session/list` has the
> capability displayed and no row to press it on. That is a consequence of where the affordance
> lives rather than a gate on state, and it is recorded rather than worked around: no surveyed
> agent advertises either without listing, and inventing a second place to name a session is a
> screen this ring was not asked for. **The fourth ring made that consequence legible rather than
> fixing it** ([§7.7](#77-the-fourth-ring-what-was-driven)): such a row draws no was-it-driven fact
> and says why, because *not driven* would be a sentence about the agent when the fact is about
> where this tool put the button. `session/resume` reads the same way wherever load is advertised
> beside it and preferred.

**The two capability shapes this ring meets are surfaced as they are.** `session/load` is gated by a
top-level boolean (`agentCapabilities.loadSession`) while resume, close, delete, list and
additional-directories are gated by the *presence* of a capability object under
`sessionCapabilities`. The schema crate records that the asymmetry is expected to be unified in a
later protocol version; until then the inspector reports both shapes honestly rather than
flattening them into one notion of support. They are **two of four**, not the whole set — the third
ring meets the other two and [§7.6](#76-the-third-ring-session-settings) names all four together, so
that this paragraph is not read as an inventory.

**`session/fork` stays on the raw-first path** ([§8](#8-unknown-traffic-the-raw-first-rule)). It is
absent from the canonical stable `schema/v1/meta.json` and present only in
`schema/v1/meta.unstable.json`, so it is not decoded as a typed method and its traffic renders as an
unrecognized entry like any other unknown — an unstable method is visibly unstable rather than
silently half-supported.

Two entries from [§7.4](#74-the-coverage-yardstick)'s order leave the ring rather than disappear
from it. **`session/set_config_option` and `session/set_mode`** get a ring of their own, which is
[§7.6](#76-the-third-ring-session-settings) — that is about *driving* the two setters, a separate
question from [§7.3](#73-decline-by-capability)'s declining of what an agent may send us.
**Elicitation form mode** is untouched by this ring and stays exactly where §7.3 left it, with no
ring assigned to it yet.

### 7.6 The third ring: Session Settings

Decided in [#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84), and it takes up the
two rows the previous ring pushed out of its own. The inspector could drive a session's whole
lifecycle and could not touch its configuration: every surveyed agent publishes settings of some
kind, and the inspector answered all of it by rendering an announcement as a passing sentence in the
timeline and throwing the rest away. **The modes and the config options an agent puts on a session
setup response stop being discarded and become Session Settings** — a right details rail beside the
Timeline showing what the session is configured as and what it could be configured as, with each
row driving the method that actually changes it.

| Method | Third-ring behaviour |
|---|---|
| `session/set_mode` | Driven from a mode row. Answers `{}`, and no rule obliges the agent to restate the mode afterwards — so a success the agent never confirms is shown as exactly that ("Nothing is applied optimistically", below) |
| `session/set_config_option` | Driven from a config-option row. Answers with the **complete replacement option set**, which the store takes as the agent's word |

**Session Settings is the name of the pair, and Mode and Config Option keep the protocol's
meanings.** The protocol names one half — *config option* — and has no word for both together, while
the surface, the store and this ring all need one. Where an agent publishes the same setting both
ways — one surveyed agent's config option list opens with an entry whose id is literally `mode` —
**both rows appear**, because a duplicated control is a fact about that agent and not a mess to tidy
away.

**Four gating shapes, and where each is read from.** [§7.5](#75-the-second-ring-the-session-lifecycle)
surfaced two and this ring meets the other two, so the set is named here in one place rather than
left reading as though there were two:

| Shape | Read from | Gates |
|---|---|---|
| A boolean on the agent's own capabilities | `agentCapabilities.loadSession` in the `initialize` result | `session/load` |
| The *presence* of an object under the agent's session capabilities | `sessionCapabilities.{list,resume,close,delete,…}` in the `initialize` result | list, resume, close, delete, additional directories |
| The *presence of data* on a **session setup response** | `modes` and `configOptions` on the answer to `session/new`, `session/load` or `session/resume` | the mode rows and the config option rows of the live session |
| A gate on **this client's own claim** | `clientCapabilities.session.configOptions.boolean` in the `initialize` request | whether a conformant agent may publish a boolean config option to this client at all |

The third shape is why these controls appear **per-session** while the lifecycle controls appear
per-connection: modes and config options are not advertised in `initialize` and are not capabilities
at all — they are data on the setup response of the session that is currently live. The fourth runs
the other way round from every shape before it: the claim being read is the inspector's own
([§7.3](#73-decline-by-capability)), which is why **the client's advertisement is shown beside the
agent's** ([§9](#9-screens)). Without that, a boolean option arriving from an agent that gates on the
claim has no visible cause, and a tool whose thesis is that claims should be legible has been
auditing only one of the two parties.

**The advertisement is not configurable**, and what that costs is stated rather than hidden
([§1.1](#11-what-is-deliberately-not-built)). A second candidate annotation — an agent sending a
boolean config option to a client that never claimed one — can therefore never arm, so it does not
go on the rule list ([§15](#15-open-questions--validation-gaps) q7) at all.

**One store, two driven methods.** The two setters are two calls with different parameters and
different responses, and merging them would hide that; they fill **one** store, because what the user
reads is one thing. The store has **three sources, and the setup response outranks the others**: the
`modes` and `configOptions` fields of the session setup response, the responses to the two setters,
and the `current_mode_update` and `config_option_update` notifications. Where a `session/load`
replays a mode change before answering and the answer then **omits** `modes`, the answer wins: there
is no advertisement, so there is no affordance, and the surface says plainly that the agent replayed
a mode change it did not offer. Inferring an affordance from replayed traffic would be the inspector
deciding on the agent's behalf what it supports, which is the rule the previous ring exists to
establish.

**The setup response's own settings are written where the frame is read**, not by whoever awaited the
call — which is the same rule the turn already follows ([§11](#11-v2-seams-kept-open) seam 2) and, for
this store, the only ordering that can be right. An agent is free to announce a mode change in the
breath after it answers, and the one task reading the connection has that notification in hand before
the caller has woken up: a caller writing the answer's settings afterwards would paint over an
announcement that came *later* on the wire. Reading both from the reader keeps the store in wire
order, which is what "the setup response outranks the others" means and all it means — it is a rule
about a *replay that preceded the answer*, never a licence to overwrite what the agent said next.

**The two notifications are not read the same way, and the asymmetry is the protocol's.** A
`current_mode_update` from an agent whose setup response published no `modes` leaves the store
alone: it names **one mode and no set**, so there is nothing in it an affordance could be built
from, and building one anyway would be the inspector deciding on the agent's behalf what it
supports — the rule the previous ring exists to establish. A `config_option_update` from an agent
whose setup response published no `configOptions` **is taken**, because it carries **the whole
list**: taking it is taking the agent's word about its own session, and refusing it would be a
surface showing less than the agent sent, which is the one thing this layer may not do
([§8](#8-unknown-traffic-the-raw-first-rule)). So the gating rule below — an affordance follows the
advertisement — is stated of the *setup response versus a replay that preceded it*, and the
advertisement a later announcement carries is the announcement's own. An agent that publishes
nothing and then restates a full option set has advertised one; an agent that publishes nothing and
then names a mode has not. Both are driven in `crates/core/tests/session_settings.rs`.

**Nothing is applied optimistically.** `session/set_config_option` answers with the complete
replacement set and the store takes it. `session/set_mode` answers `{}`, so a success the agent never
follows up on leaves the surface showing **the last mode the agent stated**, marked as a change
*acknowledged and not confirmed*. Displaying the requested mode would be the inspector stating a
configuration its agent never claimed; showing nothing at all would read as a broken control. This is
the shape [§7.5](#75-the-second-ring-the-session-lifecycle) used to tell a resume-only user that an
empty timeline was correct. A `current_mode_update` settles it, which is the agent that does announce
its change being rewarded with a surface that agrees with it.

**What a setter's answer produced is written where the frame is read**, for the reason the setup
response's settings are: the ordering the surface reports is the wire's. An agent may announce the
mode it just set in the breath *before* it answers, and the one task reading the connection has both
frames in hand before whoever asked has woken up — so a caller marking the set
*acknowledged and not restated* afterwards would be contradicting an announcement already read. The
rule that follows is stated as broadly as it is coded: **any `current_mode_update` settles an
acknowledgement**, even one naming a mode nobody asked for, because what that marking claims is that
the agent has said nothing about the mode since and an agent that stated a mode has said something.
Like the load rule reading a replay, that can only make the marking quieter than it might have been,
never wrong about an agent. **A refusal is not settled by one**: it claims the agent answered *no* to
a particular ask, which stays true however many modes it states afterwards, and taking it back would
leave a user who asked for a mode they cannot have with nothing on screen saying so. It goes when the
next set replaces it, or with the session it was asked in.

**A failed setter costs the attempt and nothing else.** The row stays at the last value the agent
stated — nothing was applied, so nothing is rolled back — and the error is reported beside the
surface, naming the setting that failed. The frame is in the trace either way, because a refusal is
evidence rather than a disappearance. **An answer that will never arrive is one of those failures**:
a connection that ends under a set in flight refuses it with the news that the agent is gone, the way
the same teardown ends a turn that was running ([§6.1](#61-the-connection-contract)) — a
control left waiting on an answer nobody is left to give would be the window showing a question that
has stopped being one. **A `session/set_config_option` refusal reads the same way a `set_mode` one
does**, including surviving a `config_option_update` that arrives after it, and for the same reason:
what it claims is that the agent answered *no* to a particular ask, which stays true however many
option sets it states afterwards. It goes when the next set replaces it, or with the session it was
asked in.

**A rejected `session/set_config_option` is said under the option it was about — or beside the
surface, when there is no longer such an option.** The answer to this setter is the complete
replacement set, so an agent is entitled to stop publishing the very option a set was refused for,
and a refusal drawn only under a matching row would vanish with the row. That would be the window
keeping the secret the store exists to tell, so the sentence moves rather than disappears.

**A select's value goes out as a bare value id and a boolean's goes out under a `type`
discriminator** — the two shapes the schema serializes a config-option value as, because the
discriminator describes the *shape of a value* rather than the kind of an option. A boolean write is
not a select's with a different value in it: `"type":"boolean"` beside an unquoted `value`, and a
bare value id in its place is a shape a real client got wrong in a way that fails against a real
agent. Which shape goes out is decided by the control that knows the option's kind, and carried from
there as the schema's own value type rather than as a value id something downstream has to interpret.
**The boolean shape is on the wire at all only because of the client capability this ring's fourth
gating shape names**: a conformant agent must not publish a boolean config option to a client that
never claimed one, so there was nothing to write before the claim went out.

**Switching sessions discards and rebuilds the Settings**, the same rule as the timeline and for the
same reason: what is on screen describes the session that is open. There is no carry-over and no
merge, and what refills the surface is the newly-opened session's own setup response — on create,
load and resume alike, because the three are one thing to a reader. **The rebuild is the discard**:
the answer replaces the whole value rather than emptying it first, which is what keeps the surface
describing the session that is still live while the next one is being opened. A load that is
*refused* opened no session, so the session it could not replace is still the live one and its
settings are still what describes it — where the timeline, discarded ahead of the ask it could not
have been rebuilt without, states its cost instead ([§7.5](#75-the-second-ring-the-session-lifecycle)).

**A replay is read as the opening session's only where it names that session, and it does not touch
the store.** The session being left is entitled to go on talking — the last words of a turn the
switch just cancelled — and attributing those to the session that is opening would report an
asymmetry the agent never produced. What the store holds until the answer arrives is the *live*
session, which is still the one being left, so an announcement naming the session being opened is
left to the answer that will rebuild the store from the setup response whole: writing it here would
put the opening session's mode on the row of the session being left, and a load the agent then
**refused** would leave it there for good — a session showing a mode it never stated. `session/load`
and `session/resume` say which session they want back, so their replay is identifiable;
`session/new` mints its id in its own answer, so nothing crossing before that can be shown to be
about it and it records none. That is under-reporting rather than mis-reporting, which is the
direction a rule decided from frames is allowed to fail in — the same direction the load rule and
the acknowledgement rule already fail in.

**And the surface says what crossed rather than which call it was.** The collision is `session/load`'s
because load is the call that MUST replay, but the same two frames in the same order can arrive under
a `session/resume`, which MUST NOT replay at all — an agent that did it there did this with a rule
broken on top. So the mode is held and reported for either way of reopening a session, and the
sentence claims what the frames decide: the agent stated a mode ahead of the answer, and the answer
offered none. Which call carried it is the trace's to say.

**Testy cannot produce the collision, and that was settled by driving it.** The upstream test agent
answers `session/new`, `session/load` and `session/resume` with the same two fields off the same
session state, and replays nothing at all — so it can neither publish modes on one setup path and
omit them on another nor announce a mode ahead of an answer. The scripted agent in
`crates/core/tests/session_settings.rs` is therefore the only way to reach the case, and a test drives Testy
through all three paths to keep that a finding rather than an assumption.

**Affordances gate on the advertisement, never on state**
([§7.3](#73-decline-by-capability)'s rule, unchanged). Setting a mode or a config option **during a
live turn is permitted**, as is any other change the specification leaves undefined: the inspector
sends what it is asked to send and reports what comes back. An agent that advertises a setting it
cannot actually change is driven anyway, and the refusal is the finding. An agent that published no
modes gets **no mode control at all**, so absence reads as its answer rather than as an empty widget.

**Two option kinds are settable and the rest are visible.** Select and boolean options are rendered
as controls; an option that decodes to neither is rendered as a **read-only row** rather than
dropped, because an option shape the inspector cannot drive is still one the user should see. Two
short select values form a segmented group; three or more, long or grouped values use a native
select with one compact *Set* action. Choosing in the select prepares the ask and *Set* sends its
selected value — including the current one — repeatedly; the current value remains a separate fact
stated on the row, so the ask is never presented as an Agent-confirmed change. A
boolean uses one native checkbox-backed toggle. Its checked state continues to be the Agent's stated
value, and activation asks for the inverse without changing that display until the store receives
the Agent's answer. While either setter waits, the affected row states the pending protocol method
and that setter family's controls are ARIA-disabled, removed from future Tab order and guarded
against another call without discarding whichever control already has focus. A native select keeps
its node while its options are unavailable. Its value chooser and Set action have distinct names;
accepted, refused and failed outcomes remain the settings store's account. The affordance remains
gated on the Advertisement, never on what the window thinks the Agent's state makes sensible.

**And the read-only row is a forward-compatibility arm, which is now driven rather than read.** An
option whose kind is a *third* string never decodes to an unknown kind at all: v1's kind is
internally tagged with exactly `select` and `boolean` and no catch-all, and every option list on the
wire is deserialized with the item-skipping combinator, so such an option **vanishes from the list
before core sees it** while the options beside it survive. A scripted agent publishing one is in
`crates/core/tests/session_settings.rs` and that is what it finds. The rule stands as stated — the day the
schema grows a variant this client does not drive, the option is on the list and read-only — and
until then the row cannot be armed, which is exactly the under-reporting the next paragraph names.
**Two tests were wanted for it and neither is writable**: the scripted fixture through seam 1, which
is now the test that proves it is not writable, and a component test through seam 2, which cannot be
written either — the kind is a `#[non_exhaustive]` enum in another crate with no third variant, so
nothing outside that crate can build one to render. The arm is carried by the compiler rather than by
a test, and this paragraph is the record of that. This settles
[§15](#15-open-questions--validation-gaps) q10.

**The Settings surface is a decoded view, and this document says so.** The schema skips invalid
entries in an option list during deserialization, so a malformed or future-shaped option can vanish
before core ever sees it. The raw-first rule is not weakened — the frame is in the trace, complete
([§8](#8-unknown-traffic-the-raw-first-rule)) — but the surface would under-report without saying so,
and naming that limit is better than implying a completeness the typed layer cannot deliver.

**The timeline entries for `current_mode_update` and `config_option_update` are kept as they are.**
The surface shows what a setting *is now*; the timeline is where *when it changed* stays on the
record. Neither replaces the other.

**One annotation is added**, under the admission rule
([§15](#15-open-questions--validation-gaps) q7) and no more than one. Details are there with the
other rules; the reason it qualifies is that a `session/set_config_option` which *succeeded* for a
given option id and answered with a set not containing that id is decidable from two frames, needs no
model of agent state, and cannot be the complete set the specification requires — the option provably
exists, because it was just set.

### 7.7 The fourth ring: what was driven

Decided in [#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96). The capability panel
could say what an agent claimed and could not say what happened when anybody took it up: an agent
that advertises `session/close` and refuses every one is indistinguishable, on the screen built to
audit its claims, from an agent nobody ever pressed the button on. **The panel's rows gain a fourth
fact — whether the advertisement was driven on this connection, and what came back.**

It is **passive**. It sends nothing; it records what the affordances already there caused. The
tool-driven pass [§1.1](#11-what-is-deliberately-not-built) promised is declined rather than
deferred, and the argument is in
[ADR 0002](adr/0002-the-capability-sweep-is-a-record-and-not-a-pass.md).

**Three outcomes, and a call nobody answered is one of the three.** A row reads *not driven*,
*driven and answered*, or *driven and refused* — the last carrying the agent's own error, because a
client paraphrasing a refusal is a client standing between the reader and the wire. A connection
that died under a call in flight lands in **refused**, which is where `AuthState` and the two
setters already put it ([§7.6](#76-the-third-ring-session-settings)); the split
`conformance.rs`'s `Ending` makes exists because a MUST forced it, and nothing here is forced. The
latest outcome is what the row shows. **A refusal is not sticky**: the next drive of the same
advertisement replaces it, which is the rule a `session/set_config_option` answer already follows —
what survives an unrelated announcement there is a refusal of *a particular ask*, and a capability
is not one.

**The fact is drawn only where an affordance can reach.** Five of the thirteen rows have no
affordance at all — the three prompt content kinds and the two MCP transports, both deferred with
rings owed ([§1.1](#11-what-is-deliberately-not-built)) — and two more can be advertised with nothing
that reaches them: `session/resume` where load is advertised too and preferred, and `session/close`
or `session/delete` where `session/list` is not advertised and there is no row to press them on. On
those rows the fourth fact is **absent and says why**, because *not driven* would be a sentence
about the agent, and the true sentence is about this tool. Which those are is a fact about the
affordances, so it is not a state a store holds.

**Where a whole run shares the one reason, the run says it once.** The three prompt content shapes
are one reason and the two MCP transports are another — a run is the claims made in one shape at one
place in the `initialize` result, which is the unit that can share one — and each of them said it
word for word on every row it had. In the 280px rail these were drawn in then, that is one fact
about the inspector drawn as three about the agent, and the repetition is what made it read as a property of each capability.
So the sentence is hoisted to a caption beside the shape's, in the plural — *this tool cannot drive
any of these* — and the rows under it carry no fourth fact at all. **The correction is not dropped,
only said once**: a reader meeting rows with no outcome still has to be told the blankness is this
tool's. Two conditions, both narrow: every row of the run has the same reason, and there is more than
one row — a run of one has nothing to say twice, and the session lifecycle, where an unreachable
`session/resume` sits beside a `session/list` something here drives, keeps the reason on the rows
that have one. What the run says is held to the same rule the rows are: in words, in the
accessibility tree, never a class. It reads in the caption's register rather than the row's, which is
the trade — the sentence is about this tool rather than about what the agent claimed, and `--faint`
clears AA everywhere ([ADR 0003](adr/0003-the-stylesheet-is-generated-and-committed.md)).

**And the fourth fact takes a line of its own on the row that carries one.** A refusal in the agent's
own words and a reason nothing reaches the row are both sentences, and a sentence that wrapped took
the field annotation with it: `.field` sits at the far edge, so it pinned itself to the top
right while three lines wrapped under the name. The row is a grid — mark, name and field on the line
a reader scans, the fourth fact on the line below, indented under the name it is about. It costs a
line on the rows that carry one and nothing on the rows that do not.

> **Thirteen, not the twelve this document first said.** The panel drew twelve when this section was
> written and the auth row was not one of them: `authenticate` had no row, so the sentence below
> about it rendering from `AuthState` had nothing to render into. The row is the thirteenth, claimed
> in a shape neither of the other two is — the `authMethods` list on the `initialize` result, beside
> `agentCapabilities` rather than inside it, which is exactly why *Advertisement* is a wider word
> than *Capability* (`CONTEXT.md`). The five with no affordance at all are unchanged.

**And a row the agent never advertised says nothing about driving.** There is no claim to have
driven, so *not driven* would answer a question nobody asked; the row's answer is the *not
advertised* it already gives. The five with no affordance on any agent are the exception, and say so
whatever the agent claimed — that sentence is about this tool, and an agent's silence does not
change it. Those five are exactly the two runs that say it above their rows, so on the panel the
exception reads as a caption over a silent run rather than as five rows answering a question nobody
asked.

**Two advertisements stop being unreachable in this ring**, which is what makes the record worth
drawing at all rather than mostly a report on the inspector:

| Advertisement | Fourth-ring behaviour |
|---|---|
| `agentCapabilities.auth.logout` | A button in the agent panel, connection-scoped like the call ([§7.1](#71-driven-client--agent)) |
| `sessionCapabilities.additionalDirectories` | A list of roots the user supplies, beside the `session/new` button and on the reopen row prefilled with what the listing reported |

**A root the user typed relative is resolved against the session's `cwd`, not sent as typed.** The
schema says each path must be absolute and, two lines earlier, that `cwd` *"remains the base for
relative paths"* — so the base is the session's, not the inspector's working directory, which is
where the spawn form's own `cwd` resolves against for a reason that does not transfer (the child
process is started there). This tool speaks ACP as a well-formed client ([§1](#1-scope)); sending a
request the schema calls malformed would contaminate every annotation drawn against that agent with
the client's own violation. Driving the corners the specification leaves *undefined* is the rule
([§7.6](#76-the-third-ring-session-settings)) and this is not one of them.

**Whether `additionalDirectories` was driven is read off the outgoing frame.** The field is omitted
when empty, so an empty list puts nothing on the wire and drives nothing — and the default reopen,
which hands the agent back the roots it reported, drives the capability exactly when that list was
non-empty. Reading the frame rather than the intent is the rule the option-set annotation already
uses: *what crossed must not turn on what this client recognized*. The reopen goes on sending the
reported roots whether or not the capability was advertised, because handing an agent its own words
back is fidelity to the session being reopened rather than a claim about a capability.

**A call nobody asked for writes no record.** The listing refreshed on the tool's own initiative
after a close or a delete keeps the promise §7.5 made for it — a failure there says so in the trace
and nowhere else. The record is about what an agent did when it was asked; a row marked by a call
the user never made, that no later listing of theirs could clear, would be the panel reporting the
tool's own housekeeping as the agent's answer.

**One store, and `authenticate` is not in it.** `AuthState` is already this record for the auth
methods — not driven, accepted with the method, refused with the agent's own error, per connection —
so the auth row renders from it rather than from a second store holding the same three things. That
is [§7.6](#76-the-third-ring-session-settings)'s one-store argument applied to a store that already
existed.

**Where it is written, and what clears it.** In core, beside the other per-connection stores
([§5](#5-the-coreui-split)) and never derived from the trace: the trace is a bounded ring that drops
its oldest frames ([§15](#15-open-questions--validation-gaps) q3), so a record scanned out of it
would report *not driven* about an agent that was driven, on exactly the long connection where
somebody wants to read it. Answers are written where the frame is read, the rule
[§7.6](#76-the-third-ring-session-settings) set; a failure that never became a frame is written by
the caller under the same id guard the turn and the two setters already use, because there is no
frame for a reader to see. The registration that maps an answer back to its capability is emptied
when the connection goes, which is what stops a dying connection's late answer landing in the next
one's record — the mechanism five id-keyed slots already rely on, rather than a new ordinal for one
store and not the others. **Cleared on connect, kept when the agent dies** — the panel outlives the
agent it describes, and blanking the record at the moment the run ended would erase the evidence
just as someone turned to read it — and **untouched by a session switch**, because what it is about
is the connection's claims.

**The capability set must not become a third hand-written list.** Core's list and the panel's are
deliberately separate and each fails on its own, with the schema asked directly whether either has
gone stale (`crates/core/tests/capability.rs`). Whatever keys this record must stay inside that
arrangement: a set core enumerates and the panel matches loosely would render a capability upstream
added as *unknown* rather than breaking a build, which is the failure that test exists to prevent.

**No annotation comes with any of it**, and the reason is [§15](#15-open-questions--validation-gaps)
q7's rather than an omission — an agent that advertises a capability and refuses it breaks no MUST,
because ACP v1 states none. It is a finding, read off the row.

**And the record adds no term to the glossary.** It has no surface of its own, no artifact, and no
second party to audit; it names a property an Advertisement carries rather than something a screen
is about. *Exercisable* stays what [§7.5](#75-the-second-ring-the-session-lifecycle) already means by
it — that there is a button — and this section says *driven* for the other thing.

### 7.8 The fifth ring: elicitation

Decided in [#133](https://github.com/sagikazarmark/dioxus-chat.orig/issues/133). The permission
request stops a turn and this tool answers it; the three calls Testy makes after it are answered
*method not found* on the reader's behalf, and then the prompt dies at `-32602` and the turn ends
with no stop reason. No scenario the reference agent ships has ever run to `end_turn` here.
**`elicitation/create` becomes serviced — both modes — and this client claims
`clientCapabilities.elicitation` for exactly what it will render.**

| Method | Fifth-ring behaviour |
|---|---|
| `elicitation/create` | Interactive and **inline in the timeline at the point the turn blocks**, like the permission request beside it ([§7.2](#72-serviced-agent--client)): form mode renders a form from the agent's own schema, url mode shows the host and the full URL, and both answer `accept`, `decline` or `cancel` |
| `elicitation/complete` | Swallowed onto the entry whose `elicitationId` it names — the notification is the out-of-band half closing, and it carries no response |

**What §7.3 declined, and what it did not.** That section's list was always about *services* — work
this tool would have to perform and genuinely cannot. There is no filesystem to mediate and no
terminal to embed, so those two are unchanged and now say so in their own right. The third was
argued the same way on the strength of one clause — that this tool *"renders no elicitation form"* —
which was a fact about what had been built and not about what this tool is. It renders a
schema-driven form on the settings screen already. The rule §7.3 states survives intact and is what
this ring is held to: **the inspector claims exactly what it will honour.**

**And the yardstick had already moved.** [§7.4](#74-the-coverage-yardstick) measures coverage against
the canonical `schema/v1/meta.json`, which lists eleven client methods with `elicitation/create` and
`elicitation/complete` among them — while `session/fork`, which [§7.5](#75-the-second-ring-the-session-lifecycle)
parks on the raw-first path, is absent from that file and present only in `meta.unstable.json`. The
former `unstable_elicitation` cargo feature was the schema crate's packaging, not the protocol's
status. [ADR 0008](adr/0008-the-stable-method-list-is-the-gate-not-the-feature-name.md) records why
it was enabled and why schema 1.7's stabilization removes it: the default build now includes both
methods. [§8](#8-unknown-traffic-the-raw-first-rule)
is unweakened: what lands in the raw bucket is what the stable schema does not define, which is the
sentence that rule was always making.

**Both scopes are serviced, and the scope is stated rather than assumed.** A session-scoped
elicitation names its session and, where it carries one, its tool call; a request-scoped one names
its request id, because it has no session by construction. Neither is filtered against the live
session's id — a permission request has carried a session id since the second ring and this client
has never checked it, and a filter here would be the tool deciding which of its subject's traffic is
legitimate.

**The timeline draws a blocking request whether or not a session is open.** A request-scoped
elicitation may arrive with no live session at all, which is the case that scope exists for. The
alternative is answering `cancel` on arrival, and that is this ring's one real temptation:
**`decline` and `cancel` are both claims about a user.** Answering either because no screen had been
built would have this client assert that somebody dismissed a question nothing ever drew. So the
entry list is what the screen shows the moment there is an entry, and the empty state — which is the
thing that says no session is open — gives way to it; the panel names the request it is tied to
rather than a session it never had. The rule this states once for the whole tool: **this client
answers for the reader only where it showed the reader something.**

**Which is also how a mode this client never advertised is answered.** `ElicitationMode` carries an
untagged escape hatch, so an unknown mode decodes cleanly and the refusal is a choice rather than a
decode failure. *Accept* would assert an interaction that never happened and the other two would
speak for the reader, so the answer is `-32602` — the error the specification names for exactly this
— and the frame is a first-class entry like every refusal §7.3 already makes. Nothing about it is an
annotation ([§15](#15-open-questions--validation-gaps) q7).

The form is drawn by schemaform 0.5.0 and the registry's daisyUI renderer, with
advisory submission and literal requestedSchema input
([ADR 0013](adr/0013-schemaform-draws-the-elicitation-form.md)). The unmerged
[spike](schemaform-spike.md) records the measured costs and the blockers fixed before adoption.

**Constraints are reported and never enforced.** `required`, ranges, lengths, patterns and item
counts are evaluated by the form engine and findings are stated beside their fields and in a named
summary; the answer sends
anyway. This is [§7.5](#75-the-second-ring-the-session-lifecycle)'s gating rule turned on the
reader's own input: a tool that cannot send `age: 999` to an agent that asked for `0..120` cannot
find out what that agent does with it, and finding that out is what somebody came here for.

The host's finding vocabulary states the rule and limit beside the current value:
“Above the maximum of 120”, “Too few characters; at least 3 stated”, “Required,
nothing filled in”. Numeric minimum/maximum, string minLength/maxLength,
multi-select minItems/maxItems, required presence and `pattern` are checked by
schemaform; corrections remove the associated notes. `format` selects an input
widget but is not checked, and its description says so. Numeric parse findings
say “Not a number” or “Not a whole number”: that text remains an edit buffer,
outside committed form data, while the advisory answer carries the committed
values and Raw can send a string instead. Native required metadata is descriptive;
the form's `novalidate` and advisory submission prevent it gating Accept. Findings
are referenced by the field's accessible descriptions, and summary links include
the field label and path. They are not Conformance Annotations.

**And a raw tab, because the typed answer cannot say everything.** `content` is typed as a map of a
closed untagged value set, which has no spelling for a nested object, a null, or a value of the
wrong type — so typed controls alone would quietly restore the gate the paragraph above removes,
for every violation that is about a *type* rather than a range. The answer therefore carries
arbitrary JSON and core encodes the response body directly. Decoding stays strictly typed, which is
what [§11](#11-v2-seams-kept-open) seam 1 is about; **encoding this tool's own answer is where the
raw-first rule applies to a frame it writes rather than one it reads.** The tab holds a JSON object
or nothing — the one shape the method has, and the only thing an acceptance can refuse.

> **The surface being shown is the surface that sends, and it says so.** This section first said
> *whichever was edited last*, which is a rule a reader cannot see the state of: write something by
> hand, click back to the form, accept, and an answer they had typed would have gone nowhere with
> nothing having said it would. What is built is the visible one — the reader accepts what is in
> front of them — with a line under the tabs stating it, and with each surface keeping what was
> written in it so that switching is not losing. An empty *form* sends no content, because nothing
> was filled in; an empty *object*, typed by hand, sends `{}`, because that is a thing the reader
> wrote and rewriting it would be this window editing an answer on its way out.

> **And it is what takes the window from four dependencies to five.** `crates/app/Cargo.toml` counted
> its dependencies out loud as the shape of the crate, and this ring adds `serde_json` to that list.
> The answer to an elicitation *is* a JSON object by the protocol's own definition, core's answer
> path is written in those types, and the raw tab is a text box a reader edits one in — so the
> alternative was asking core for a JSON builder nothing but this panel would ever call, which is a
> seam invented to keep a count. The rule §5 states is unweakened: this is a data format, not a
> protocol, and frames are still strings all the way through the window.

**A property type this client does not know draws no control at all.** The schema instructs clients
not to render an unknown property type as a known input, and the reason is this tool's own: a guess
at somebody's extension, rendered as though understood, is the failure [§8](#8-unknown-traffic-the-raw-first-rule)
exists to prevent. The field shows its raw schema and points at the tab that can fill it.

**URL mode costs no dependency and is compliant by construction.** The requirement is that the
client open the URL where the client itself cannot inspect it, having shown the full URL and taken
consent, and having prefetched nothing. `dioxus-desktop` installs a navigation handler that refuses
`http(s)` navigation inside the WebView and hands the URL to the system browser, so a plain anchor
on the `url` field *is* the hand-off, the page never loads in a context this tool could read, and
the window keeps its four dependencies ([§5](#5-the-coreui-split)). An `accept` means the reader
consented and never that the interaction finished — `elicitation/complete` is a MAY and may never
arrive, and the panel says which of the two it is showing. The hand-off is the framework's behaviour
with no seam to assert it through, so it is a row in
[`desktop-smoke-matrix.md`](desktop-smoke-matrix.md) rather than a test.

**Scope decides the lifetime, because the screen decides who can pay the debt.** Session-scoped
mirrors the permission request exactly: answered `cancel` when the reader cancels the turn,
abandoned when the turn settles unanswered, abandoned on disconnect. A request-scoped one outlives
its turn — nothing in the protocol ties it to one, and its row survives a turn boundary like every
row. A session switch answers **everything** outstanding `cancel`, request-scoped included, for the
reason §7.5 already gives about the permission debt: it comes due when this client stops being the
one that answers, and the timeline that would have answered it is discarded whole.

**The plumbing is shared and the types are not.** `Pending` becomes generic over the small contract
the four teardown sites use, so those sites keep one implementation and cannot drift; the request
itself is its own public type with its own states — waiting, answering, answered with one of three
actions, abandoned. A state machine with three answers is not the permission one bent into shape,
and merging them would hide that the way §7.6 says merging the two setters would have.

> **Completion is a second fact, not a fifth state — corrected by the build.** This section first
> said the states ran *waiting, answering, answered, completed, abandoned*, with completion after the
> answer. They are not one sequence. A reader consents and an agent says the out-of-band half
> finished, and neither implies the other in either direction: `elicitation/complete` is a MAY that
> may never arrive after an accepted URL, and it may equally arrive *before* anybody has clicked —
> the interaction happens in a browser this tool cannot see, so the agent can know it is done while
> the panel is still waiting. Folding it into the answer state would have forced a lie in that
> ordering: either drop the completion, or mark the request completed and lose the answer the
> JSON-RPC request is still owed. So the request carries both, independently, and the panel says
> which of the two it is showing.

**One sentence in the live region, for either kind.** [§9](#9-screens)'s region says the agent is
waiting and offers the way to the row it stopped on; it now says so for a blocking request rather
than for a permission request, with one control pointing at the newest waiting entry of either kind.
Two sentences and two controls would split the reader's next action along a distinction that does
not change it. The composer's line loses the same word.

**Where the outstanding set lives, and what leaves it.** Per connection, keyed by `ElicitationId`,
in core beside the pending list. *Outstanding* is the annotation's own word and it is not a synonym
for unanswered, which is what decides the lifetime in both directions. An id leaves when a matching
`complete` arrives, when the reader **refuses** it, when the request is abandoned, or when the
connection goes. It does **not** leave on consent: an accepted URL elicitation is one whose
interaction may be running right now, somewhere this tool cannot see, and only the agent's own
statement ends that — which is the same reason completion is a second fact rather than a state after
the answer. A `complete` naming an id two entries hold marks both: that is only reachable once the
agent has already tripped the annotation below, and picking one would be this tool guessing where it
has just reported that it cannot know. A `complete` for an id nobody issued is unsolicited traffic,
shown and otherwise ignored, which is what the specification asks of a client there.

> **A refusal frees the id, and this section first said it did not — corrected by the build.** The
> rule above was written as *not when the reader answers*, which reads decline and cancel as consent
> because they are answers. They are the interaction **not** happening: nothing is running, and the
> agent is free to mint the id again. A client that went on holding it annotated that agent for
> breaking a MUST it had not broken — the failure [§15](#15-open-questions--validation-gaps) q7's
> admission rule exists to prevent, and a worse one than the missing annotation would have been,
> because a tool that reports rules the specification does not state is one whose reports cannot be
> trusted anywhere. The annotation list in §15 states the corrected rule; this is the paragraph it
> was corrected from.

**One annotation is added, and two candidates are named and left off.** Added: **an agent MUST keep
each `elicitationId` unique among outstanding URL elicitations** — a MUST, decided from captured
frames alone, needing no model of agent state, which is [§15](#15-open-questions--validation-gaps)
q7's admission rule met exactly. Left off: *an agent requesting a mode this client did not
advertise*, because the claim is fixed and always made, so the rule can only arm on an unknown mode
that §8 already covers as unknown traffic — the boolean config option precedent
([§7.6](#76-the-third-ring-session-settings)) unchanged; and *URL mode required for sensitive
interactions*, which cannot be decided without judging what is sensitive, and judging is a grade.

**Testy is the subject and needs no build change.** `agent-client-protocol-test` enables its unstable
features by default, so the binary [§12](#12-dummy-agent-and-test-strategy)'s script already builds
ships an `elicitations` scenario that sends five requests in a fixed order — form with a tool call,
form declined, form request-scoped, url with its `complete`, url request-scoped — which is every
happy path this ring has. `callbacks` and `full` reaching `end_turn` is the end-to-end proof, and it
is new. Scripted `/bin/sh` one-liners cover what a reference agent will not send: a duplicate
`elicitationId`, an unknown mode, an unknown property type, params v1 cannot read, a `complete` for
an id nobody issued, an elicitation the turn ends without answering, and one naming a session that
is not the live one.

**Two terms come with it, and one is amended.** `CONTEXT.md` gains **Blocking request** — the
category this screen has rendered since the second ring and the glossary never named — and
**Elicitation**, defined over both modes rather than the host context's form-only sense. **Timeline**
is amended: it is the live session's stream *and* the blocking requests the connection is waiting
on, which is what the no-session region above makes true.

**What this ring declines.** A fourth fact for the client's own claims — *an agent actually used this
claim on this connection* is a different sentence from §7.7's, worth having, and a ring of its own
rather than one smuggled in here ([§1.1](#11-what-is-deliberately-not-built)).

## 8. Unknown traffic: the raw-first rule

Decided in [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56), and it is the
spec's central invariant:

> **Every frame renders raw JSON, always. A typed layer decorates frames it recognizes. Unknown
> methods and unknown `session/update` variants render as first-class unrecognized entries —
> never dropped, never errors.**

Consequences: `_`-prefixed extension methods and `_meta` payloads (real agents use them) display
verbatim; unstable-v1 methods land in the same bucket, which is coverage enough until they
stabilize; and v2 traffic arriving early degrades to visible-raw instead of breaking anything.
This rule is what lets v2 arrive without redesign, and it is why the typed layer *decorates* the
trace rather than replacing it.

**Numbers enter protocol types through one canonicalizing seam**
([#3](https://github.com/sagikazarmark/acp-inspector/issues/3), `crates/core/src/decode.rs`).
As Frame text becomes a JSON value, the seam canonicalizes numbers for every subsequent typed
decode of params, results and error bodies: machine integers stay exact, and every other number is re-spelled as the `f64`
it rounds to under **stock serde_json 1.0.151's numeric policy**, recursively through objects and
arrays. This preserves the representation the original stock build produced. It protects against
a dependency enabling `serde_json/arbitrary_precision` for the build: the private maps that feature uses for spellings
such as `0.10` and `1e2` cannot be read by the protocol crate's buffered types (`tag`, `untagged`,
`flatten`). Some fields then refuse to decode; default-on-error fields can instead **silently drop**
cost or annotation priority while the enclosing update still decodes. The canonical copy is for
typed decoding only: the Trace, Console and re-serialized evidence views keep their existing Frame
source. A renderer asking for a literal value must receive it from the Frame, never through this
seam. `ElicitationRequest::raw_form()` supplies requestedSchema tokens to the renderer.
The numeric tripwire starts with literal Frame text and
asserts presence and value in the public stores; a source-level tripwire keeps protocol decoding
inside this module.
The release-backed spike found a second leak ([#8](https://github.com/sagikazarmark/acp-inspector/issues/8)):
`serde_json/float_roundtrip` changes `from_str::<f64>` itself. For example,
`0.146675314082485333` becomes `0.14667531408248533` instead of stock's
`0.14667531408248535`. The seam now reads `RawValue` fragments before either feature can discard
the original number token. Objects and arrays are built explicitly; only strings, booleans and
null use the ordinary Value reader. Core enables `raw_value`, not `arbitrary_precision`, so this
fix does not change the number policy of unrelated renderer input readers.
The private `decode/stock_number.rs` implements the stock u64-significand, truncation and decimal
scaling policy independently of those Cargo features. This deliberately preserves stock rounding,
not the nearest representable decimal value. Machine integers are still never rounded. No new
crate is needed. Fixed expected values were captured from an isolated default-feature reader,
not computed with the same feature-sensitive parser being tested. `bash scripts/check-number-features.sh`
checks all four feature selections, including both features together.
Literal `-0` becomes floating-point `-0.0` in the typed copy. Numbers are interpreted exactly
once: typed decoding does not re-round their canonical spelling. Nesting remains bounded and
out-of-range numbers fail before typed decoding, as they did under the stock reader.

**Where a frame is drawn raw is the wire log, amended 2026-08-17**
([ADR 0009](adr/0009-the-window-is-the-drawing.md)). The rule above is about the *record* and is
unchanged: every frame is captured, counted, exportable and readable as it crossed. What changed is
where a timeline entry's frames are read. Each entry used to carry a disclosure holding its own
frames; now the entry **leads** to them — pressing the row selects its frames in the wire log
beside it, where the pane reads one whole and laid out — and the disclosure is gone from the flow.
The two cases where the frames are not evidence *for* a rendering but the rendering itself, traffic
this window cannot name and a conformance annotation whose claim is about the bytes, still draw them
under the row without asking. Nothing is one press further away than it was: the control that used
to open a disclosure is the row.

**The one thing a reader may ask for is whitespace.** The Indentation switch ([§9](#9-screens)) draws
a payload's JSON spaced out, off by default, adding whitespace and changing nothing else — anything
that is not JSON is drawn exactly as it arrived, and what is recorded, exported and copied is the
bytes that crossed. That is the rule above kept rather than qualified: a reading aid may not be the
reason somebody is shown something other than what the agent sent.

## 9. Screens

The feature cut from [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56). The
center of the UI is the **turn timeline** — not a primitive catalog; ACP has no enumerable
primitives to browse ([#49 §5](https://github.com/sagikazarmark/dioxus-chat.orig/issues/49)).

> **Where the regions are, amended 2026-08-17**
> ([ADR 0009](adr/0009-the-window-is-the-drawing.md)). This section is written as
> though the Console were a row under the screens and the two accounts a tablist across the centre.
> They are not, and what follows below is otherwise unchanged — every rule about what a surface may
> hold, what a control is gated on and what a row must say still binds. The arrangement is:
>
> - **A toolbar**, one row: the tool's own mark and name, the Connection as a chip, the protocol
>   version and the live Session, the **spine** (Split | JSON-RPC full — whether the two screens
>   share the width or the wire takes all of it, replacing the Console's collapse), and the two
>   window preferences.
> - **A rail**, fixed at 244px: the Connection as a card — its state, the transport, the invocation
>   and its `cwd`, and the acts on it, which is where connecting and disconnecting live — then two
>   tabs holding the live Session with its Settings, and the Agent's and the Client's accounts of
>   what they claimed. Nothing about the turn or the wire is in it. At the foot, the way into the
>   command palette and how many commands the Agent published.
> - **The Timeline**, in the middle and alone: its own header says which turn the rows belong to,
>   names the live Session and carries where the turn stands; the entries have a gutter of marks;
>   the composer is under them and offers the commands the Agent published.
> - **The Console**, beside it: the same two tabs, the same counts, the same two verbs on its own
>   bar, plus filter chips for the envelope's four shapes and one for the frames in none of them —
>   and a **pane at the foot of the list** holding the frame a reader selected, laid out and
>   coloured, because a Frame is selected here rather than expanded in place.
> - **Over all of it**: the launch dialog, the sessions dialog, the command palette, and the one
>   sentence the window says about itself.
> - **Below 1080px** there is room for one region rather than three, so the body shows one and a
>   segmented bar at its foot chooses between them (Details | Session | Messages). Nothing is lost
>   at any width, and the chooser does not exist while all three are on screen.
>
> Two controls the section below describes are drawn as the design draws them rather than as an
> earlier ring argued: a **Mode** is a full-width row per mode in the rail *and* a cycling control in
> the composer's footer — one setting, one call, offered where a reader stands when they want it
> changed — and a config option's ungrouped values are chips, with the native select kept for a
> *grouped* list, which is the one shape a row of chips cannot say.

- **Spawn form** (a dialog over the window) — command, args, env, cwd; at most a recent-commands
  convenience (no persistent catalog). Failure auto-surfaces its evidence: an agent that is gone without being asked to go —
  one that never ran, and one that ran and died — expands the Console and selects its Diagnostics tab
  on the captured output, the MCP Inspector flow worth copying verbatim. Both states, because both
  mean the agent is gone and nobody asked it to go; it is the one thing on the window that takes a
  surface the reader may have chosen, and it is allowed to because an agent that never started
  produced no frames.
  > **The convenience is built**, and stops where the sentence above does: the last ten
  > invocations whose agent **answered `initialize`**, newest first, one click to put one back in
  > the fields — nothing named, edited, imported or exported. The mark is the handshake and not a
  > session, because an agent that will not open one until somebody logs in is the case this most
  > exists for; where it is kept, what it holds, and why that is the mark is
  > [§15](#15-open-questions--validation-gaps) q6, answered there.
  > **Refill confirms itself where it happened.** A persistent polite status beside Recent names
  > the invocation put back in the fields and says that Launch is still the next step; editing any
  > field or launching clears it, so it never describes values that have since changed. Empty
  > multiline fields wrap and hide placeholder overflow, while typed argument and environment text
  > retains its local horizontal scroller and leaves verbatim.
  > **It is a dialog, because launching is done once per connection.** Written once, read never
  > afterwards, and measured as a permanent column it was a fifth of the window's width with forty
  > per cent of that column empty — so it folded, and a form that folds is a form that has to be
  > unfolded. A dialog that is not open takes no room at all, which answers the same measurement
  > without the fold, the chevron that worked it, or a line naming the running agent that only
  > existed because the fields might be hiding it. The toolbar carries the way in and the
  > disconnected Timeline's empty state opens the same dialog; what is running is the toolbar's
  > subject, said whatever else is on screen.
  >
  > **And the way in and the way out are one control.** It is **Connect…** with no agent and
  > **Disconnect** with one, in the same place, and the word is the whole of the difference. It was
  > two buttons side by side and one of them was always unavailable — Connect… refuses while an
  > agent runs, because the launch form does (a dialog that opens onto a disabled Launch is a
  > control that says yes and means no), and Disconnect had nothing to end for the whole of a first
  > run — so the window's most prominent chrome permanently carried a control nobody could press.
  > *Connect…* and *Disconnect* read as one control in two states, which is a thing to be rather
  > than a pair to arrange. Neither state is a refusal, so the ARIA guard the pair needed is gone
  > with it. The word is also the reason it is not *Stop*: that is the composer's word for
  > cancelling a Turn, and the two on screen together with the same word, icon and red were the
  > cheap mistake that turns out to be the expensive one. **The platform owns the modality**: a native
  > `<dialog>` shown with `showModal` traps focus, dims the window behind it, closes on Escape and
  > gives focus back to whatever opened it — four behaviours this window would otherwise write, get
  > subtly wrong and have to test. Whether it is showing lives on the element and in no signal,
  > because Escape and the backdrop close it without telling anybody, and a window that thought it
  > was still open would answer the next click by setting a value that had not changed. It closes
  > itself on one thing that is not the reader: an agent that answered.
  > **Initial focus is initial.** Command carries `autofocus`, which is what `showModal` looks for
  > when it decides where the keyboard goes — so the fields are focused once per opening rather
  > than once per mount, and a dialog that is shut cannot take focus from Trace, Diagnostics or
  > another control at all. Launch remains the same focused button while the
  > handshake is pending; like Disconnect, permission choices, elicitation answers, setting controls and
  > empty Trace actions,
  > it becomes ARIA-disabled and guards activation instead of becoming natively disabled.
- **Console** — the **Trace** and the **diagnostic channel** as two tabs,
  showing one at a time, spanning the full width of the window under both screens. What
  belongs in it is what the transport seam produced *about one Connection*, below the typed layer,
  which is what those two have in common and why the timeline is not in it — nothing decoded is in
  it ([§1.1](#11-what-is-deliberately-not-built) holds the admission test and what it refuses). A
  Frame's row may lead with what the *envelope* says it is — its method, its id, and which of
  JSON-RPC's four shapes it is in — because that is the transport's own vocabulary rather than v1's;
  a Frame in none of those shapes is labelled with nothing and drawn as bytes
  ([ADR 0006](adr/0006-the-window-is-drawn-as-an-application.md)).
  Bottom rather than right: both tabs' content unit is a long line, and what a long line needs is
  width.
  > **Trace is selected when a window opens**, and the selection is window-session state that is
  > never written to `settings.json` — a window must not open onto an empty Diagnostics tab because of
  > what was being debugged yesterday.
  > **It collapses as one**, to a header strip carrying both counts: frames (with the trace's
  > dropped count) and diagnostic lines, so that *frames are arriving* and *the agent is saying
  > things* stay legible while it is shut. One click restores it, on the tab that was selected. The
  > diagnostic channel has no collapse of its own; one collapse belongs to the panel.
  > **The compact native refinement keeps that shape literal** ([#125](https://github.com/sagikazarmark/dioxus-chat.orig/issues/125)):
  > the Console is a flat bottom utility pane rather than a card, its tabs use the compact underline
  > strip, and one accessibly named Heroicons Outline chevron remains the only collapse control.
  > **Tabs carry counts and nothing else** — no unread badge, no activity dot, no severity mark
  > when a `bad` diagnostic line arrives. A count is a fact; a badge is a claim that the reader
  > should look, and this tool reports rather than grades, which is the same rule a conformance
  > annotation is held to. The one escalation the tool makes is the status-driven one the spawn
  > form's entry states: a connection state decides it, never volume or tone.
  > **One height**, sized for the Trace rather than for stderr — it is the denser surface and its
  > rows expand. No splitter, no drag, no maximise; what the height should be is a question real
  > use answers, and a drag handle is what gets built instead of answering it.
  > **Real use answered it, and the answer is a number**: 28vh, about a dozen Trace rows, in place
  > of the 40vh this said when nothing had been driven yet. The old number was arrived at as a share
  > of the window rather than from what is read on the surface, and what a reader does here is watch
  > the newest few rows and expand one — twenty rows is not a thing anybody reads at once, and two
  > fifths of the window is what it cost to offer them. The sentence above is kept rather than
  > struck: the mechanism it refuses is still refused, and a smaller number is the shape of answer
  > it asked for. A second height would be the drag handle it names, with detents.
  > **And it is the row that gives way.** The window's three rows were a fixed bar, a flexible
  > middle and a fixed Console, which made the Timeline's entry list the residual of the whole
  > window — it paid for the Console, the Session Settings and the composer in turn, and a window
  > dragged shorter computed it to nothing while every other row kept its share. That is the
  > wrong residual for a tool whose centre screen this is, so the screens carry a floor and the
  > Console takes what is left over: a window too short for both loses Console rows, which is a log
  > that scrolls either way, rather than losing the turn. The composer never gives way, and Session
  > Settings use an independently scrolling right rail instead of taking height from either
  > ([§7.6](#76-the-third-ring-session-settings)). A floor is a promise the window has to be able to
  > keep, so the shell now states a minimum size as well as a starting one.
- **Trace tab** — every frame, both directions, expandable raw JSON, clear; unrecognized
  entries first-class. Plus **JSONL export** ([§10](#10-the-jsonl-trace-export)). Export and Clear
  sit on the Console's own bar beside the tabs, not on a second strip under them; the frame count
  and the dropped count sit on the tab, and neither is drawn at zero — what an empty surface has to
  say is the empty state's. The bar also carries the **filter**, which narrows whichever
  surface is showing and says what it is hiding — presentation only, so both stores go on capturing
  and counting everything and an export writes the whole trace
  ([ADR 0006](adr/0006-the-window-is-drawn-as-an-application.md)).
  > **Evidence disclosures share one compact visual contract** ([#125](https://github.com/sagikazarmark/dioxus-chat.orig/issues/125)):
  > Frames, Timeline raw Frames and tool-call raw input/output use native disclosure state, complete
  > accessible names and tooltips, visible focus, and the same Heroicons Outline chevron. A Frame's
  > name is its ordinal, direction, Connection and timestamp plus the disclosure action, never the
  > complete raw Frame preview; the bytes remain visible and available under the disclosure. Opening
  > them moves no evidence under transition. Connection identity is ordinary monospace metadata;
  > badges remain for compact counts and bounded states rather than every fact in a dense row.
  > **Every row says which connection it crossed.** The trace outlives any one agent — `Trace::tap`
  > exists for that, and the trace deliberately survives a disconnect
  > ([§7.5](#75-the-second-ring-the-session-lifecycle)) — so one trace holds two agents' traffic
  > while both agents' JSON-RPC ids start again at 1. The ordinal the tap assigns is rendered on
  > every frame, and the row where one connection's traffic gives way to the next draws the
  > boundary, so two agents read as two conversations. It is the same ordinal
  > [§10](#10-the-jsonl-trace-export)'s records carry, unrenumbered, so a row and a record of the
  > same frame agree. Presentational and nothing else: no frame is captured, retained or exported
  > differently, and the diagnostic channel carries no connection identity and is left alone.
  > **And every row says where it is, so the two screens can point at each other.** The ordinal a
  > frame was captured under — `dropped + position`, which the row was already keyed by and the
  > export already writes — is carried up out of the trace with the frame, so a Timeline entry
  > knows the rows it was decoded from and a row knows whether it became an entry. Both directions
  > are offered: an entry reaches its frames, which opens the Console on the Trace tab — an
  > affordance that scrolled a panel nobody can see would be a control that does nothing — and a
  > row that became an entry reaches it. What either does is **mark and scroll to**, never select,
  > filter or hide: what a reader asked to see is not a reason to stop showing them the traffic
  > around it. The mark takes the *far* edge of the row, because the near one already carries which
  > direction a frame crossed and whether an entry is unrecognized traffic or an annotation — a
  > navigation aid that erased one of those would be buying a jump with the evidence. A row no
  > entry was made of carries no control rather than a dead one: an answer to a call this window
  > made is no less real for having no entry, and neither is a frame this client *sent*, whose
  > ordinal the send path does not carry back up today.
  > **A frame can be taken away one at a time.** The export is the whole trace
  > ([§10](#10-the-jsonl-trace-export)) and what a reader wants is usually one line, which they
  > were otherwise selecting by hand out of a list appending underneath them. The rule is the
  > density rule read as an affordance: **wherever the content is bytes, it can be copied** — a
  > frame here, an entry's raw frames on the Timeline, a line on the Diagnostics tab. What
  > lands on the clipboard is the bytes that crossed, for the reason nothing is rewritten on the way
  > in ([§8](#8-unknown-traffic-the-raw-first-rule)) — including where the Indentation switch is
  > drawing them spaced out, because a paste has to be evidence about the agent and not about how
  > somebody had this window set.
  > **And the window says so, for two seconds, in a corner.** A copy is the one thing this tool
  > does that leaves no mark on any screen — every other control changes something a reader can
  > look at, and this one changes something in another application — so it is the one thing the
  > window is allowed a transient voice for. It is said **only when the write is confirmed**: a
  > clipboard that did not take is still not reported, because what a failure looks like is the
  > paste that does not happen, one keystroke away from being tried again. Nothing else goes
  > there. A refusal is drawn on the row it was refused on, an export's error under the button
  > that asked for it, an agent's own reason on the tab that carries it — a message which takes
  > itself away is a message a reader can miss, and none of the record may depend on having been
  > watched.
- **Diagnostics tab** — the diagnostic channel, live, timestamped per line. Each row says whose
  voice it carries in the accessibility tree: **Agent stderr** for the Agent's verbatim line,
  **Transport** for the connection's own remarks. The compact coloured mark repeats tone only and
  remains decorative; Copy still takes the evidence text rather than its presentation label.
- **Timeline** (center screen) — prompt composer; the streamed `session/update` sequence
  (message/thought chunks, tool-call cards with status updates, plans, the rest of the eleven —
  [§7.2](#72-serviced-agent--client) on why eleven); inline blocking-request panels that block
  where the turn blocks — a permission request, or an elicitation and the form or URL it asks with
  ([§7.8](#78-the-fifth-ring-elicitation)); turn state and `stopReason` rendered from the stream.
  > **The rows are grouped by the turn they arrived in.** Every entry carries the turn it was made
  > during and the store keeps a record of each turn with what it came to, so the screen draws a
  > quiet line under a turn that is over carrying the agent's own `stopReason` — a session of six
  > exchanges reads as six turns rather than as forty rows and one status about the last of them.
  > A turn that ended having said nothing still gets its line, because a prompt that vanished is
  > worse than an empty group. **And traffic belonging to no turn is marked as such**: an agent
  > that talks before anything is prompted, goes on streaming after it resolved the turn, or
  > replays a session under `session/load` is producing rows that look identical to the rows above
  > them on a flat list. Grouping is read from core's stamp and never inferred from arrival order
  > ([ADR 0007](adr/0007-a-turn-is-a-record-and-not-only-a-position.md)).
  > **A conformance annotation reads as a row here**, in the flow where the traffic it describes
  > was, carrying what the specification requires, what was observed instead, and the frames that
  > decide it — open, not a click away. It is the one entry that is the inspector's own voice, and
  > it is weighted like every other: no lane, no severity, no count anywhere. The admission rule
  > and what is annotated today are [§15](#15-open-questions--validation-gaps) q7.
  > **A turn waiting on the reader says so out loud, and the request can be reached.** The entry
  > list is deliberately not scrolled for anybody — it stays where the reader put it — so a
  > blocking request arriving while they read further back arrives off screen, and the only thing
  > that said so was a line in the composer calling it *above*, which is true of the composer and
  > wrong for a reader who has scrolled. What replaces it is a live region at the near edge of the
  > list: one sentence saying the agent is waiting, announced because this is the one moment the
  > tool has stopped and is waiting to be told what to do, and a control that goes to the row it
  > stopped on. The region is in the document whether or not anything is waiting, because an
  > announcement is made by text *changing* inside one; empty, it draws as nothing. It is not a
  > badge and grades nothing: a blocking request is the turn stopping, not a fault of the
  > agent's, and what this says is where it stopped and how to get there. **One sentence for either
  > kind, and one control** ([§7.8](#78-the-fifth-ring-elicitation)): which sort of question stopped
  > the tool is not a distinction the reader's next action turns on, and two of each would split it
  > along one that does not change it. An elicitation may also be waiting with no live session at
  > all, which is the other reason this region says *waiting on you* rather than naming a turn.
  > **And the way back to the newest row is offered rather than taken.** The Timeline, the Trace and
  > the Diagnostic channel are all laid out bottom-up and none of them is scrolled for anybody, so a
  > reader who scrolled back stays there while rows arrive — correct, and it left them scrolling to
  > return. Each list carries a control that appears once it is not at its tail and does nothing
  > until it is pressed
  > ([ADR 0006](adr/0006-the-window-is-drawn-as-an-application.md)).
  > **Plan and diff meaning is text-borne.** A plan's marks remain compact and visible but are
  > decorative; each entry also carries the words *pending*, *in progress* or *completed*, and
  > names its priority as a priority. A tool-call diff is a region named by its path whose old and
  > new verbatim blocks are labelled and represented by `del` and `ins`. Colour repeats those
  > distinctions and never owns them.
- **Session Settings** (right details rail beside the Timeline) — the live session's modes and config options
  ([§7.6](#76-the-third-ring-session-settings)): what it is configured as, what it could be
  configured as, and a control per row that sends the method which changes it. A mode row says it
  goes out as `session/set_mode` and a config option row says `session/set_config_option`, so which
  method a control drives is readable without going to the trace.
  > **It sits in a right details rail because it belongs to the Session**, and it is the first thing
  > that does. The Agent screen is Connection-scoped — the Agent's identity, its capabilities, its
  > auth, its Session listing — so putting Session Settings there would say they belong to the Connection,
  > which is exactly the misreading [§7.6](#76-the-third-ring-session-settings)'s third gating shape
  > exists to prevent. The rail is 288px at ordinary desktop widths and narrows to 248px at the 960px
  > minimum, leaving the Timeline flexible; its vertically stacked rows scroll independently. A Mode
  > is also what a user sets immediately before prompting, so it remains next to the prompt it will
  > change the answer to. A setting the Agent did not advertise has no row and no
  > widget; a mode acknowledged but never confirmed says so on the row; a failed setter reports its
  > error here, beside the setting that failed, rather than only in the trace. And the one absence
  > that has an explanation is given one: where a replay stated a mode the answer that opened the
  > session never offered, the surface says so where the row would have been, naming what the agent
  > did.
- **A connection that ended keeps its record and says so.** Nothing is cleared when an agent goes:
  the timeline, the trace, the diagnostic channel, the settings it published and the sessions it
  reported are what the tool was run to produce, and an agent that died mid-turn is the finding
  ([§5](#5-crate-boundaries)). What the window owes instead is to say which of the two states it is
  in, wherever there are controls it is about — the same sentence in the same register on Session
  Settings, in the login, and in the sessions dialog — with every control ARIA-disabled beside it.
  And **Connect is not offered while an agent is running**: one connection at a time, so the way
  forward is Disconnect, and a Connect that opened onto a Launch which refuses would be a control that says
  yes and means no.
- **Sessions** (a dialog, opened from the Timeline's header) — `session/new` with its
  `additionalDirectories` control, `session/list`, and a row per session the agent named carrying
  the one way in it advertised, `session/close` and `session/delete`
  ([§7.5](#75-the-second-ring-the-session-lifecycle) holds what each of those costs).
  > **It is a dialog because the rows are not a menu.** The sketch it came from was a dropdown on
  > the session id, and a listed session carries too much for one: an id, a title, where it runs, a
  > roots control of its own, three calls and a page cursor. A menu holding that is a panel wearing
  > a menu's clothes; a menu that dropped it would be abridging affordances §7.5 gates one claim at
  > a time. The platform's `<dialog>` gives the focus trap, the backdrop and Escape, exactly as it
  > does for the launch form. It shuts itself on one thing that is not the reader — a session that
  > opened, which is what every control in it that opens one was pressed for. A session *ending*
  > leaves it open, because closing or deleting one is managing the list rather than leaving it.
  > **Three parts, in the order a reader needs them**: the session they are in, the sessions they
  > could be in, and the way to a new one. It was one flat run — two buttons, five paragraphs, then
  > every session the agent named as one list with the live one marked by a word halfway down its
  > row — which is the shape a rail leaves behind, where a block is read top to bottom once. A
  > surface opened to *change* the session is read by looking for the one you are in and the one you
  > want. **The live session is stated whether or not it was listed**: it used to appear only if
  > `session/list` had been pressed, so a dialog opened on a working connection could say nothing
  > about the session behind it. Its id is not the listing's to give, and the two calls that end a
  > session carry nothing else — so an agent that never advertised listing still has a session here
  > that can be closed. While a listing is in flight that row keeps its id and loses its description,
  > because where it runs *was* the answer being replaced. **The way to a new session is at the
  > foot**, where a dialog's own action goes, with its roots control beside it.
  > **The platform is given the click outside and the first focus, too.** `showModal` gives the focus
  > trap, the backdrop, Escape and focus restored — but not dismissal by clicking away, which every
  > application anybody has used does, so the backdrop is a `<form method="dialog">` filling the space
  > behind the box and its submit button is the click. And the keyboard arrives at the dialog rather
  > than on its Close button, which is where `showModal` puts it by default: the least likely thing a
  > reader came for and the one whose accidental Enter undoes the opening. The launch dialog is the
  > exception and keeps its first field focused, because typing is what it is opened for.
  > **A row leads with the call and not with what the call carries.** The
  > `additionalDirectories` disclosure was drawn first and was the widest thing on the row — a
  > modifier read as the row's subject, with the session's own working directory squeezed to an
  > ellipsis beside it. It sits under the reopen it belongs to. **And the calls that end a session
  > are quiet until the row is wanted**, the rule the Timeline's rows already state
  > ([ADR 0006](adr/0006-the-window-is-drawn-as-an-application.md)): a reader opens this to change
  > session, not to destroy one, and three calls at equal weight on every row made a list of sessions
  > read as a list of controls — so what changes is the weight and not the presence: `session/load`
  > is the row's own size and the two ways out are a step down from it. **Hover-reveal is for a
  > control that repeats something already on the row** ([ADR 0006](adr/0006-the-window-is-drawn-as-an-application.md)),
  > like a Timeline entry's raw disclosure; it is the wrong rule for the only way to do a thing, since
  > what is not drawn cannot be looked for and a list swept with a pointer cannot be read at a glance.
  > `style.rs` holds the boundary.
  > **What each call costs is on the call.** Five paragraphs of consequence stood above the list,
  > written for a rail that was read once; they are the controls' own descriptions now — on the
  > tooltip for a pointer, in the document for `aria-describedby` — so the accessibility tree
  > carries exactly what it carried before. The one that stays visible is the one that is not about
  > a control at all: an agent that advertised no way back into a session has an absence to explain,
  > and an absence has nothing to hang a description on.
- **Capability/auth display** — agent info, capabilities, and auth methods from the
  `initialize` result; capability-gated affordances (e.g. the `session/list` button) appear only
  when advertised, gated on the advertisement, not on current content.
  > **What this client advertised is shown beside what the agent advertised**
  > ([§7.6](#76-the-third-ring-session-settings)). Three claims — boolean config options, and the two
  > elicitation modes the fifth ring added ([§7.8](#78-the-fifth-ring-elicitation)) — and the panel is
  > where an agent author finds out which of their agent's choices were responses to this client's
  > claim: an agent that degrades a boolean option for clients which did not claim the capability, or
  > that will not attempt a URL elicitation against one that did not, offers a shape whose cause is
  > otherwise invisible. The modes are two rows rather than one, because an agent gates on them
  > separately. The two sides are not merged into one list of "capabilities": who claimed a thing is
  > the point of showing it.
  > **Each account is a screen of the centre, and the window has no third column.** Drawn in a rail
  > they were 1430px of a 1994px column in a 568px viewport — three and a half screens, of which the four
  > things a reader presses were four hundred pixels, so the panel for driving an agent put its own
  > specification between the reader and its buttons. Collapsing them bought the height and none of
  > the width: a capability row is four facts — the claim, the field it was made in, whether it was
  > advertised and what became of driving it — and 288px of rail holds three, which is why every row
  > was two lines. The centre carries a switch —
  > **Timeline | Agent | Client** — in the tablist the Console already uses for surfaces behind one
  > panel, and each account has a screen to itself where a row is one line, in tracks stated once
  > for the account so that the four facts align down the whole of it. The identity is the screen's
  > heading rather than a list of three rows, and the composer is drawn on the turn's view alone.
  > **Beside now means one click and never merged.** The client's claim is fixed for every
  > connection, so sharing the agent's screen spent half of it on a block that is the same on every
  > launch of every agent; the two are still drawn identically, still never folded into one list of
  > "capabilities", and the causal reading §7.6 wants — a boolean config option whose shape is an
  > answer to this client's claim — costs a tab rather than a glance. **And the inspector's own
  > commentary is not drawn as a pull-quote.** A hairline down the near edge marked every aside this
  > tool writes — what a call costs, the shape a run was made in, why nothing here drives it — which
  > was a rail's answer: a pixel wide, no room to spare, and a way to skip a sentence already read.
  > On the claims screen the same rule lands between every group heading and its rows, where a table
  > wants nothing at all, and quotes the annotation about the rows as though it came from somewhere
  > else. The register is the size and the tone, which is what carried it all along. Each account
  > carries how many
  > of its claims were advertised and how many were refused, on the line above its rows.
  > **And what a reader presses moved with them.** Sessions and authentication stayed behind in the
  > rail for one revision, which left a column holding three unrelated one-off tasks: a form used
  > once per connection, a listing read once, and a login used once. The launch form is a dialog and
  > the column is gone; the window is a toolbar, one centre screen with its tabs, Session Settings
  > beside it while a session is live, and the Console.
  > **The three then went to three different places, because they are three different kinds of
  > thing.** A capability listing is a reference table, a session listing is a set of objects, and a
  > login is a state — and one screen holding all three drew `Sessions` twice, three hundred pixels
  > apart, as an action group and as a capability run. **Sessions are a dialog off the Timeline's own
  > header** ([§7.5](#75-the-second-ring-the-session-lifecycle)): a session is what the Timeline
  > draws, so opening, switching and ending one all happen where their effect is visible, and the
  > header names the live session as it always did. **A login is a word on the identity line** —
  > *Login required*, *Authenticated*, *Login refused* — with the methods and the two calls under it
  > and nothing at all where the agent advertised no method, which is most agents; what was *done*
  > keeps the agent's own words for it. **And the claims are the whole of the screen**: the four
  > facts are four columns with the slack after the last of them rather than between the first two,
  > the group headings stay on screen while their rows scroll, and a chip row narrows the table to
  > the claims that were advertised, the claims that were not, or the claims that were refused. The
  > narrowing is presentation and nothing else — the count on the summary line goes on counting the
  > whole account, because that count is the agent's answer rather than the filter's, and a narrowed
  > table says what it is hiding so that a hidden row and a claim never made do not look alike
  > ([ADR 0006](adr/0006-the-window-is-drawn-as-an-application.md)).
  > **The session lifecycle is shown whole**, and it is the one group where that is a decision
  > rather than a formality: all six capabilities v1 defines — load, list, resume, close, delete,
  > `additionalDirectories` — each stating whether *this* agent advertised it, because agents
  > disagree sharply here and what an agent left out is as much of an answer as what it claimed.
  > A capability it did not advertise is drawn as not advertised rather than omitted — and drawn
  > at the same contrast floor as one it did, because a half of the answer rendered as an aside
  > is a half of the answer the reader skims past. Which half a row is is carried by **weight**
  > on screen and by **words** in the accessibility tree, not by dimness; the mark beside it
  > repeats both and is decoration in either colour scheme, hidden from that tree. The two
  > gating shapes ([§7.5](#75-the-second-ring-the-session-lifecycle)) are kept apart and named
  > where they live — a boolean on `agentCapabilities`, the presence of an object under
  > `sessionCapabilities` — and a row whose capability is not called by the field it is claimed
  > in says which field that is, so an agent author can see why their own agent advertises
  > support in two different shapes. It is display only: the affordances that gate on it are the
  > rest of the ring.
  > **And a row says whether it was ever driven** ([§7.7](#77-the-fourth-ring-what-was-driven)) —
  > not driven, driven and answered, or driven and refused with the agent's own error beside it.
  > Weighted like the rest of the row and no louder: a refusal gets no colour of its own, no count
  > and no ordering, because it is the third thing that happened rather than the bad one, and an
  > agent that advertised a capability and then refused it is a finding the reader draws, not a
  > grade this tool awards. It is absent — with a word saying why — on the rows no affordance
  > reaches, since *not driven* there would be a claim about the agent when the fact is about this
  > tool — and where every row of one run has that same reason, the run says it once above them and
  > the rows say nothing, because three rows repeating one sentence about the inspector is what makes
  > it read as three facts about the agent. Both of those are sentences, so the fourth fact takes a
  > line of its own under the name it is about: a refusal wrapping inside a 280px rail used to
  > collide with the field annotation pinned at the far edge. The auth row draws its own from the
  > auth state, which has held exactly this since the MVP.
  > **`logout` sits with the auth methods**, not among the session controls: the call carries no
  > session id, so it belongs to the connection the panel already describes. It is offered wherever
  > the agent advertised it, including with nobody logged in, and what the agent does to a live
  > session it has logged out of is watched rather than pre-empted — the specification guarantees
  > nothing there, which is the reason to look.
  > **`session/new` is the affordance beside it that gates on nothing**, because the protocol
  > gates it on nothing: every v1 agent opens sessions, so the button is there whenever an agent
  > has described itself and the connection is live, and a connection that went away between the
  > render and the click reports that rather than doing nothing. The session it opens becomes the
  > live one. Creating one while a session is already live is a **switch**
  > ([§7.5](#75-the-second-ring-the-session-lifecycle)), and the switch is what the panel says it
  > is: an in-flight turn cancelled first, every waiting permission request answered `cancelled`
  > — `session/cancel`'s own sequence, reused rather than written twice ([§7.2](#72-serviced-agent--client))
  > — and the timeline discarded and rebuilt, with the trace untouched. It does not wait for the
  > cancelled turn to end: the specification's MUST is a thing an agent may break and this tool is
  > pointed at the ones that do, so the turn is left behind rather than waited on, and what answers
  > it afterwards answers the caller and not the screen — the turn state is the live session's. The
  > cancellation rule ([§15](#15-open-questions--validation-gaps) q7) is disarmed with it, because
  > a rule decided from captured frames cannot be decided by a client that stopped listening; the
  > frames are in the trace either way. The launch handshake is unchanged and is simply the other
  > caller of the same operation.
  > **And the view is emptied wherever the session it is a view of is left behind.** A `session/new`
  > on the same agent has always cleared the timeline; a relaunch — a new agent, a new connection and
  > a new session — did not, so the dead agent's rows stayed above the live one's, undifferentiated,
  > under a composer that sends to the live one. That is the larger of the two discontinuities
  > keeping the rows the smaller one clears. It is emptied at the connection, where the session is
  > abandoned, and only where there *was* one: an agent that talked before any session existed
  > ([§8](#8-unknown-traffic-the-raw-first-rule)), or whose handshake never got that far, leaves rows
  > belonging to no session, and a relaunch after it has nothing to replace them with. Nothing that
  > was evidence goes: every frame behind every entry is in the trace, which is untouched and marks
  > where one connection's traffic ends and the next's begins.
  > **A listed session is a way into that session**, which is what makes the listing actionable
  > rather than text ([§7.5](#75-the-second-ring-the-session-lifecycle)). Every row the agent named
  > carries one affordance and never two: `session/load` and `session/resume` restore the same thing
  > and differ only in whether the conversation replays first, so the *call* carries the difference
  > and the screen states it in words above the rows. Where the agent advertises both, load — it is
  > the one that can rebuild the conversation, it is what a shipped ACP client prefers, and it is
  > where the protocol is going. Where it advertises only resume, the screen says plainly that
  > **this agent does not support viewing previous messages**, because a resumed session's empty
  > timeline is *correct* and a reader left to work that out reads it as a fault in the inspector.
  > Where it advertises neither there is nothing to click, and the row says why. The button is
  > labelled with the method it sends, gated on the advertisement like everything else here — an
  > agent that advertised a method it never implemented is driven anyway, and its refusal is the
  > finding — and opening one is the same switch `session/new` is, stated in the same place.
  > **A listed session is also a way out of one.** `session/close` and `session/delete` are two
  > buttons on the row, each drawn because the agent claimed that call and absent where it did not
  > — two claims and two operations, not one with a property, because close frees a session the
  > agent still holds and delete takes it away. Said once above the rows: that the listing is asked
  > for again afterwards, because what the agent *now* claims to hold is why anyone pressed the
  > button; that ending the live session is allowed and leaves no session open, with the timeline
  > going with it and the trace keeping every frame; and that the calls go out whatever state the
  > session is in, because the corners the specification leaves undefined are the ones worth
  > driving. Nothing on the screen says what should have happened, because the specification does
  > not. A failure names the method that failed — four affordances sit here and three of them may
  > have worked, so an agent that advertised a capability and then refused it is a finding about
  > *that* call. **Having no live session reads as itself** in the centre screen: the timeline says
  > there is nothing for it to be a view of and where the record went, and the composer says
  > nothing is open to prompt into and names the two ways to open one — never "launch an agent",
  > which is a different absence and is right there running.
- **Appearance** — a three-way System/Light/Dark switch in the top bar, remembered across
  restarts in `settings.json` beside the recent commands. System is the default and leaves
  `data-theme` absent, allowing daisyUI's preferred-dark selection. Light sets
  `data-theme="inspector-light"`; Dark sets `data-theme="inspector-dark"`. The persisted and
  user-facing vocabulary remains System/Light/Dark. The five meaning-carrying colours — what we
  sent, what the agent said, trouble,
  live, and traffic nobody has a name for — mean the same thing in both schemes, or a screenshot
  stops being readable evidence.
- **Indentation** — a switch in the top bar, off by default, remembered across restarts in the same
  `settings.json`. On, a payload that is JSON is drawn with whitespace between its tokens wherever
  this window draws bytes to be read: a Frame on the Trace tab, the frames under an entry this
  window has no rendering for, a tool call's raw input and output.
  > **Only where somebody is looking.** Most of those payloads sit behind a `<details>`, most of
  > those are closed, and a Trace holding ten thousand Frames would otherwise lay out ten thousand
  > documents for nobody on every render. The disclosures stay the platform's — nothing sets whether
  > one is open — so what the window keeps is which of them the reader has *turned over* from the
  > state their markup was drawn in, named per surface by the Frame's ordinal, the entry's identity
  > or the tool call's id. An entry whose frames *are* its rendering has no disclosure over them at
  > all and is laid out from the first render, as is the wire log's own pane; a bit that goes astray
  > costs a payload its whitespace and nothing else, because what it falls back to is the wire.
  > **It adds whitespace and changes nothing else.** The text is validated as JSON and then
  > re-spaced character by character rather than decoded and printed back — a round trip through a
  > JSON value sorts an object's keys, collapses two fields that share a name and respells numbers
  > and escapes, every one of which is the tool quietly editing the evidence. So what a reader sees
  > indented is what crossed, in the agent's own order and spelling.
  > **A payload that is not JSON is drawn as it arrived**, which is
  > [§8](#8-unknown-traffic-the-raw-first-rule) rather than an exception to it: a Frame can be
  > anything, and half a frame, a line of English on a JSON-RPC pipe or a transport's own noise is
  > evidence the switch may not hide, refuse or annotate.
  > **It is a reading aid and stops at the reader's eyes.** The Trace records, the JSONL export
  > writes and every Copy takes the bytes that crossed, whatever the switch says. And a Frame row's
  > one-line preview stays the wire's own line: the first line of an indented document is `{`, which
  > would say nothing about any of a thousand rows.
  > **Neither preference is a settings panel.** Two switches in the bar the window already has do
  > not earn a surface, which is the reason the first one was put there and is unchanged by there
  > being a second.
- **How all of it is painted** is daisyUI-first
  ([ADR 0005](adr/0005-daisyui-owns-ordinary-components-and-themes.md)). Standard components and
  variants own ordinary control presentation, proportions, radii, elevation and timing; Inspector
  Light and Inspector Dark own their theme values. Custom CSS remains for layout, inspector
  meaning, evidence, accessibility, ordering and behavior, not to transform standard classes back
  into the previous design. The committed generated stylesheet and Node-free checkout-to-binary
  contract remain
  [ADR 0003](adr/0003-the-stylesheet-is-generated-and-committed.md)'s decision. ADR 0003's original
  comparison and palette measurements remain historical evidence; ADR 0004's exact scales are a
  historical baseline rather than global component law.
  > **Monospace is kept wherever the content is bytes.** Raw JSON, Frames, ids and diagnostic lines
  > are read character by character, which proportional type makes harder. daisyUI may use its
  > roomier proportions for ordinary controls without weakening evidence presentation.
  > **Motion is interactive state only** — hover, focus, press, disabled, expand, collapse.
  > **Nothing animates arriving content**: not a frame landing in the trace, not a chunk appending
  > to the timeline, not a stderr line. [§15](#15-open-questions--validation-gaps) q8 records that
  > this app's rendering behaviour over a fast-appending log is unmeasured, and animating that
  > stream is the fastest way to find out badly — an animation per arrival is work per arrival,
  > and it also lies about timing, since what the eye reads as the frame arriving is the transition
  > finishing. The rule is written down so that a fade does not get added back later on the grounds
  > that it looks nice.
  > **The five colours' meanings are fixed; their values belong to the themes.** What we sent, what the agent
  > said, trouble, live and traffic nobody has a name for keep those meanings in both schemes, and
  > a hue may move within its family — a different amber, a different blue — provided the four
  > signals those five carry (direction, trouble, liveness, unnamed traffic) stay distinguishable
  > from one another and the AA floor holds. A move is owed to both schemes at once: the same
  > meaning rendered as two unrelated colours is what makes a screenshot stop being evidence.
  > **The floor is the whole floor, and both schemes clear it.** Dark's known shortfall was a debt
  > to a render that the redesign stopped preserving, and it was paid there rather than carried
  > through: 4.5:1 for text and 3:1 for a signal, measured against every ground the token rests on.
  > ADR 0003 keeps the former palette's exact measurements as historical evidence; current aliases
  > derive from Inspector Light and Inspector Dark roles and retain these floors.
  > **Two commitments already argued survive any redesign.** A capability row says which half of
  > the answer it is by **weight** and by **words**, never by dimness — the rule the capability
  > panel's entry above states — and the reason it is repeated here is that dimming an
  > unadvertised row is the first thing a visual pass reaches for. And a conformance annotation
  > gains **no lane, no severity and no count**
  > ([§15](#15-open-questions--validation-gaps) q7): it is weighted like the entry beside it,
  > because this tool reports rather than grades and a style that shouts is a grade.

## 10. The JSONL trace export

Pulled *into* the MVP deliberately ([#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56)):
the export is **the seed of the event log** that ACP Wiretap and future replay tooling wait on
(the surveyed ecosystem's open gap is exactly live capture + JSONL replay + validation in one
tool, [#50](https://github.com/sagikazarmark/dioxus-chat.orig/issues/50)).

One record per frame, from the trace decorator's captured fields: timestamp, direction, and the
verbatim frame. Prior art for the record shape is the bridge's `--trace-frames` capture
(`{connection, direction, encoding, frame}`, self-describing header, nothing redacted —
`app/bridge/src/shell/trace.rs`). The exact schema, and whether it aligns with the bridge's, is
an open question ([§15](#15-open-questions--validation-gaps)) — it becomes a contract the moment
Wiretap consumes it, so it should be pinned during implementation, not grown.

> **Pinned, as [`trace-export.md`](trace-export.md).** The export shipped, and that document is
> the contract: the header and record schemas, the decision to align with the bridge's shape
> field for field (and what the inspector adds to it), what clearing means for an export already
> taken, and the bounding policy. It answers [§15](#15-open-questions--validation-gaps) q2 and
> q3; this section stays as the argument for why the export exists.

## 11. v2 seams kept open

The MVP targets v1 only. [#51 §3](https://github.com/sagikazarmark/dioxus-chat.orig/issues/51)
condensed the v2 delta into five seams; this spec builds the first, second, fourth and fifth as
**structural commitments** and the third as a modelling rule:

1. **Version-tagged decoding** — the schema crate already namespaces `v1::`/`v2::`; the typed
   layer decodes *as v1* rather than assuming there is only one dialect.
2. **Turn state is a stream-driven field**, not a request/response bracket. v1's
   response-ends-turn is one producer of that field; v2's `state_update` becomes another. A
   timeline whose spine is "request returned, therefore turn over" has the wrong spine.
3. **Timeline entities are id-keyed and merge-updated**, never keyed by arrival order alone —
   v2 makes every id-carrying update an upsert patch.
4. **Raw-first rendering** ([§8](#8-unknown-traffic-the-raw-first-rule)) — unknown = displayed.
5. **Transport is a message pipe** ([§6](#6-the-transport-seam)) — WebSocket/streamable-HTTP is
   an additive factory, not a v2 wait.

## 12. Dummy agent and test strategy

Decided in [#56](https://github.com/sagikazarmark/dioxus-chat.orig/issues/56): **adopt Testy**
from the ACP rust-sdk (`agent-client-protocol-test`, merged in
[rust-sdk#300](https://github.com/agentclientprotocol/rust-sdk/pull/300)) — a deterministic
stdio agent with prompt-driven scenarios (`echo`, `wait_for_cancel`, `session_updates`, `elicitations`,
`tool_calls`, `callbacks`, `full`) covering the whole stable v1 surface, plus a draft-v2 mode
behind `unstable_protocol_v2`. **No fixture agent of our own** — the host repo's three partial
candidates stay where they are.

Accepted consequences, recorded so the build doesn't rediscover them:

- The crate is `publish = false`: Testy is built from a rust-sdk checkout —
  `cargo build -p agent-client-protocol-test --bin testy` — and the project carries a justfile
  (or equivalent) task plus a CI step that does exactly this.
- Integration tests build Testy from git, accepting rust-sdk `main` as a dev-time moving part.
- Its v2 flag is a free test target when v2's turn comes.

Test shape follows from the architecture: `acp-inspector-core` is fully testable against Testy with
no UI (spawn → drive the 9 methods → assert stores and trace); the app crate stays thin
enough that its testing burden is presentational. **Real agents** (Claude Code ACP, Gemini CLI)
are named smoke targets only — driven by hand, not in CI.

## 13. Relationship to the host repo's assets

Settled by [#55](https://github.com/sagikazarmark/dioxus-chat.orig/issues/55) and
[#53](https://github.com/sagikazarmark/dioxus-chat.orig/issues/53). The standing rule: **copy
patterns, never link code.** The library keeps waiting for a genuinely independent second
consumer; the inspector is not it.

| Host asset | Relationship |
|---|---|
| `crates/dioxus-agent-ui-acp` (v1 ⇄ domain mapping, `Unknown { raw }` arms throughout) | Prior art for the typed layer — copied/adapted, not depended on |
| `demo/src/acp/wire.rs` + `demo/src/ui/rail.rs` wire panel | Prior art for the trace view, at proof-of-concept fidelity |
| `app/web/src/acp/wire.rs` (JSON-RPC correlation, coalesced-frame splitting) | Copy-shaped extraction into core's client layer |
| `app/web/src/acp/adapter.rs` | Reference for the full ACP choreography; app-owned, not a lift |
| `app/bridge` | Untouched. The inspector spawns agents itself; the bridge keeps serving `app/web`. Its `--trace-frames` JSONL is prior art for [§10](#10-the-jsonl-trace-export) |
| Agent UI (`app/web` + the library) | A separate project continuing its own backlog. Recorded **leaning**, not commitment: extract a shared event-log crate only when both consumers are real and the shape is proven |
| ACP Wiretap | Owns third-party MITM observation; will reuse the inspector's event log once it exists. The inspector never grows a listen-in mode |

One boundary knowingly re-crossed, for the record: the prior app effort ruled an inspector out
of scope (`.scratch/acp-app/map.md`, ticket 15); Phoenix reopened it deliberately as its primary
project ([#55](https://github.com/sagikazarmark/dioxus-chat.orig/issues/55)).

## 14. Build order (suggested)

The kickoff's standing constraint is small iterations — something working early. This ordering
is the spec's suggestion, not a decision; the implementation effort may re-slice it.

1. **Frame layer first**: `acp-inspector-core`'s connection contract, `StdioSpawn`, the trace
   decorator — proven against Testy with no typed layer at all. At this point the program can
   already spawn an agent and show raw traffic, which is a working inspector in miniature.
2. **Desktop shell over it**: spawn form, stderr console, trace view. First visible artifact;
   the failure-auto-surfacing flow lands here.
3. **Typed layer**: `initialize` / `session/new` / `session/prompt` / `session/cancel`,
   `session/update` decoding with the raw-first rule, capability display.
4. **Timeline + blocking requests**: the center screen, inline pending-request panels,
   `session/list` gating, `authenticate` display.
5. **JSONL export** (schema pinned here, [§10](#10-the-jsonl-trace-export)) and polish.

## 15. Open questions / validation gaps

Honestly, per the assembly ticket's charge. These are the seed of the implementation effort's
backlog.

1. ~~**Dioxus desktop is unproven in this repo.**~~ Everything shipped here is Dioxus *web*; the
   app's desktop leg was decided-then-tabled and never built. The MVP is the repo's first
   desktop/WebView artifact — spawn-from-UI, an async child-process pump under Dioxus desktop's
   runtime, and packaging are all first-time ground. A day-one spike of step 1+2 above is the
   cheapest way to retire this.
   **Retired by the build** (`crates/app/`, and the spike was the day-one one this suggested): a
   plain `cargo run -p acp-inspector` window, no `dx` and ~~no bundling step~~ (narrowed
   below), whose event loop is the tokio runtime core's pumps spawn onto — so spawning an agent
   from the UI and streaming its frames into it is one runtime and not a bridge between two. What
   it cost is a platform toolchain rather than a design: WebKitGTK, libxdo and OpenSSL on Linux,
   which the README documents and CI installs; nothing on macOS. ~~**Packaging is still untried**~~
   — retired by the first `just bundle-macos` run, below; what that run left open is recorded there —
   and the *rendering* half of q8 below is the part of this question the build did not answer.
   **What "no bundling step" promised has since been narrowed**, and the narrowing is the whole of
   the change: the window's stylesheet is generated Tailwind/daisyUI output, produced by a Node tool and
   **committed to the tree and compiled into the binary**
   ([ADR 0003](adr/0003-the-stylesheet-is-generated-and-committed.md), landed by the ring after the
   one that wrote this paragraph). There is still no `dx` *in the build*, nothing resolved at run
   time and nothing bundled into the binary, and `cargo run -p acp-inspector` is still the whole
   build — what is no longer true is that every input to it is produced by the Rust toolchain. Node
   changes the stylesheet and CI verifies that the committed sheet is what its source generates; it is
   not required to compile or run a valid checkout.
   **Packaging has a recipe and one run**
   ([ADR 0010](adr/0010-the-bundle-is-the-one-thing-dx-makes.md)): `just bundle-macos` has `dx bundle`
   assemble a macOS application from `crates/app/Dioxus.toml` and Apple's own tools sign, notarize and
   image it. It is made *from* the build and is not part of it — no other recipe, check or `cargo`
   command touches `dx`, and the node-free proof still blocks it. The one asset the bundle needs and
   the build does not, the icon, is in the tree at `crates/app/bundle/icon.icns`: written by `just icon`
   from the same drawing the window puts in its own icon (`crates/app/src/mark.rs`), committed the way
   the stylesheet is, and read by nothing but the bundler. The recipe was written on Linux; its first
   run, unsigned on an Apple Silicon Mac with `dx` 0.7.9, produced an `.app` and a disk image, and
   settled two of the three things the ADR said it would: the icon lands in `Contents/Resources`
   byte for byte, and `[application] name` is *not* read as the product name — the bundle came out
   as `InspectorDesktop.app`, `CFBundleName` and all, with only the identifier honoured. Fixing
   the name, and a signed and notarized run, are still owed. What the recipe already knows it will
   surface is recorded there: a Finder-launched application inherits launchd's `PATH`, and an agent
   named by a bare command that spawns from `cargo run` will not spawn from the bundle.
2. ~~**The JSONL record schema is not pinned**~~ ([§10](#10-the-jsonl-trace-export)) — including
   whether it aligns with the bridge's `--trace-frames` shape, and what a self-describing header
   carries. Must be settled before Wiretap consumes it; should be settled when export lands.
   **Answered by [`trace-export.md`](trace-export.md):** `acp-inspector-trace` v1, aligned with
   the bridge's shape field for field and extended with a record type, a per-frame timestamp,
   and a connection ordinal.
3. ~~**Trace log bounding is undecided.**~~ The MCP Inspector caps at 1000 entries and clears on
   disconnect; a long agent turn can produce a large interleaved stream. Cap, ring, or unbounded
   — and what "clear" means for an export-anchored log — is implementation's call to propose.
   **Answered by [`trace-export.md`](trace-export.md):** a 10 000-frame ring that counts what it
   drops, no clearing on disconnect, and a clear that cannot reach an export already taken while
   every later export admits what it lost.
4. **Turn-state modelling detail** ([§11](#11-v2-seams-kept-open) seam 2) is a shape, not a
   design: the concrete state enum and its producers need working out when the typed layer lands.
5. ~~**Testy's actual coverage is taken on faith**~~ from rust-sdk#300's description; nobody here
   has driven it. First contact may surface gaps (e.g. scenarios that don't exercise
   `session/request_permission` the way the timeline needs) — the no-fixture-of-our-own decision
   is only as good as Testy holds up.
   **Answered by first contact**, reported in full on
   [#60](https://github.com/sagikazarmark/dioxus-chat.orig/issues/60) as
   [#66](https://github.com/sagikazarmark/dioxus-chat.orig/issues/66) was asked to: Testy blocks
   a real turn on `session/request_permission` and resolves a cancelled one with
   `stopReason: "cancelled"`, which is the flow the inline panel exists for. Two gaps, neither
   fatal: its request offers **two** of the four option kinds (`allow_once`, `reject_once`), and
   its `callbacks`/`full` scenarios **could not end normally against a client advertising empty
   capabilities** — they went on to `elicitation/*` and failed the prompt with `-32602`. Both are
   covered by scripted one-liners through the same seam (`crates/core/tests/permission.rs`), the way
   `crates/core/tests/misbehaviour.rs` covers what a conformant agent cannot emit. **The
   no-fixture-of-our-own decision holds.**

   **The second gap is closed rather than covered**, by the fifth ring
   ([§7.8](#78-the-fifth-ring-elicitation)): a client that services elicitation in both modes is
   one Testy's `callbacks` and `full` run to `end_turn` against, which no scenario it ships had
   ever done here. Its `elicitations` scenario is the fixture for the five shapes the protocol has,
   and `crates/core/tests/elicitation.rs` drives it. What still needs a scripted agent is what a
   conformant one will not send: a reused `elicitationId`, a mode nobody advertised, params v1
   cannot read, a completion for an id nobody issued, and an ask a turn walks away from.

   **Two more answers, from driving it for the session lifecycle ring** (#81 asked for both, and
   asked that neither be taken on faith):

   - **Testy answers `session/load` without replaying the conversation** — confirmed against a
     session it had just streamed a message in, and asserted as such
     (`crates/core/tests/conformance.rs`'s `testy_answers_a_load_without_replaying_the_conversation_it_held`).
     It is therefore the *first real subject* of the load-ordering annotation (q7) rather than
     another conformant fixture, and the test that pins the finding is the test that pins the rule.
   - **Testy does implement `session/resume`**, contrary to the expectation that it advertises the
     capability with no handler behind it. rust-sdk `main` answers both calls, so the
     advertisement-that-is-a-lie fixture is a scripted one-liner
     (`crates/core/tests/restore.rs`) and Testy is the conformant agent for both calls instead. The rule it
     was wanted for — that affordances gate on the advertisement and not on the tool's opinion of
     state — is unchanged and asserted against the scripted agent.

   Both were read from the checked-out source first and both readings were then driven; the second
   one was wrong, which is the reason the ticket asked.

   **Four more answers, from driving it for close and delete** ([#82](https://github.com/sagikazarmark/dioxus-chat.orig/issues/82)
   asked that the first of them be observed rather than assumed). None of them is a conformance
   claim: the specification states no MUST about any of it, so nothing here is annotated
   ([q7](#15-open-questions--validation-gaps)) and what follows is a report about one agent.

   - **A `session/close` against an in-flight prompt answers the close first, and then resolves
     the prompt with `stopReason: "cancelled"`.** The inspector sends no `session/cancel` ahead of
     it — that would answer the question on the agent's behalf — so this is what the close alone
     did. Asserted, ordering included, in
     `crates/core/tests/lifecycle.rs`'s `what_a_close_does_to_an_in_flight_prompt_is_observed_rather_than_assumed`.
   - **A closed session leaves Testy's listing**, and so does a deleted one — the thing #75 named
     as worth watching, answered for this agent and no other.
   - **Testy objects to none of the undefined corners**: closing a session that is closed already,
     closing or deleting one it never opened, deleting one that has been closed — every one of them
     answers `{}`.
   - **A closed session can still be loaded**, though it can no longer be prompted into
     (`-32602 unknown session`). Observed while driving the corners; not asserted, because it is a
     fact about Testy that no rule here turns on.

   **And two more, from driving it for the settings ring's boolean options**
   ([#90](https://github.com/sagikazarmark/dioxus-chat.orig/issues/90)):

   - **Testy publishes one config option and it is a select**, so it cannot put the inspector on
     the boolean path at all — which is why the boolean fixtures are scripted one-liners through
     the same seam, the way `crates/core/tests/misbehaviour.rs` covers what a conformant agent cannot
     emit. Sent a boolean payload for that select anyway, it refuses it as a value it does not
     have (`-32602`, *unsupported verbosity value `true`*) rather than objecting to the shape;
     asserted in `crates/core/tests/session_settings.rs`, because a refusal that costs the attempt and
     nothing else is the rule whatever drew it.
   - **An option whose kind is a third string never reaches core**, which is
     [q10](#15-open-questions--validation-gaps) settled by driving a scripted agent rather than by
     reading the schema, and what decides that the ring's third scripted fixture is not writable
     at all.

   **And one more, which is the first close finding above read as a gap**
   ([#94](https://github.com/sagikazarmark/dioxus-chat.orig/issues/94)):

   - **Testy cannot hold a prompt open across a close**, so it cannot be the agent for what a
     close owes a waiting permission request (§7.2). The prompt it resolves off its own close
     settles the turn, and a turn that settles abandons the request that blocked it — two
     producers for one request, and a test asserting that the *close* got there first passed on
     whichever usually won and failed intermittently under the whole suite's load. Covered by a
     scripted one-liner that blocks its prompt on a `session/request_permission` and never
     answers the prompt, which leaves the close as the only producer that can reach it
     (`crates/core/tests/lifecycle.rs`'s
     `closing_the_live_session_answers_every_waiting_permission_request_cancelled`), the way the
     gaps above it are. Nothing about the client changed: both orderings are correct, and neither
     producer is serialized against the other. Testy's own ordering is still observed by
     `what_a_close_does_to_an_in_flight_prompt_is_observed_rather_than_assumed`, which is the
     finding this is the gap in.
6. ~~**Recent-commands persistence** (the spawn form's convenience) has no decided storage
   location or shape; keep it trivial and local.~~ **Answered by the build** (`crates/core/src/recent.rs`,
   asserted in `crates/core/tests/recent.rs`):

   - **Where.** One file, `recent.json`, under a directory of the inspector's own inside the
     platform's directory for a program's *state* — `$XDG_STATE_HOME/acp-inspector/`
     (`~/.local/state/acp-inspector/`) on Linux, and the data directory on macOS and Windows,
     which have no state one (`~/Library/Application Support/acp-inspector/`, `%APPDATA%\acp-inspector\`).
     State rather than config because nobody wrote it and nobody is meant to edit it. On Unix it
     is created `0600`: a remembered invocation carries its environment, which is where an API key
     would be. A machine with no home directory keeps the list for the run and says it cannot keep
     it longer.
   - **What.** `{"format":"acp-inspector-recent","version":1,"commands":[{command, args, env, cwd}]}`
     — the spawn form's four fields, verbatim, so refilling the form is copying them back. The
     format tag and version exist so a later shape can *ignore* an older file rather than misread
     one; **this is not a contract** the way the trace export is (§10), because nothing else is
     meant to read it. Anything that will not parse, or says it is something else, is read as no
     list at all — a broken convenience is not evidence about an agent.
   - **How much.** Ten, newest first, oldest dropped. An invocation is recorded when an agent
     *answered `initialize`* — the command exists, it ran, and it speaks ACP — which a handshake
     that then failed at `session/new` does not undo: an agent that wants a login first (§7.1) is
     the case the convenience most exists for, since the login happens in another terminal. A
     command that never started said nothing and is not recorded: that is a typo, and it is still
     in the fields. What is recorded is normalized to what the spawn would actually have run
     (blank lines and surrounding whitespace gone), and a re-launch of one already there moves it
     to the front instead of joining itself. Written beside the file and moved into place, so an interrupted
     save leaves the old list rather than half of a new one.
7. ~~**`session/cancel` conformance display**~~ — the spec says the prompt must still resolve
   `cancelled`; how the UI flags an agent that violates this (a conformance annotation in the
   trace?) is open, and is the first instance of a broader later question: how far the inspector
   goes toward *validation* (the ecosystem gap #50 found) versus plain observation.
   **Answered by [#75](https://github.com/sagikazarmark/dioxus-chat.orig/issues/75)**, and the
   broader question is answered with it, by an **admission rule** rather than case by case:

   > The inspector annotates only where the specification states a **MUST** *and* the violation is
   > decidable from frames the trace already holds. Nothing requiring a model of agent state
   > qualifies.

   Three rules qualify today, and they are named rather than derived:

   - a turn that was cancelled and whose `session/prompt` did **not** resolve with
     `stopReason: "cancelled"` (the point this question was asked about);
   - a `session/load` that answered **before** replaying the conversation as `session/update`
     notifications — the ordering is normative, and it is the difference that decides whether a
     client can rebuild a session at all;
   - a `session/set_config_option` that **succeeded** for an option id and answered with a set that
     does not contain that id ([§7.6](#76-the-third-ring-session-settings)) — the response must be
     the complete set of options, and the one just set provably exists.

   Annotations are a **core** concern, computed where the frames are, and render **inline beside
   the traffic they describe** — never a modal, a toast, or a lane of their own; reading an
   annotation is reading the record. There is **no rule engine, no severity grading, no aggregate
   verdict, no pass/fail score and no configuration**, and behaviour the specification leaves
   undefined draws no annotation at all — so the absence of a complaint is not mistaken for
   approval, and its presence is not mistaken for a rule that does not exist. The list grows only
   when the specification supplies another qualifying MUST.

   > **The mechanism and the cancel rule are built.** An annotation is a timeline entry
   > ([§9](#9-screens)) carrying the frames it is about — the `session/cancel` this client sent,
   > and whatever answered the turn — so the claim and its evidence are read together. The rule is
   > armed by a cancel that found the turn still running and judged where the turn resolves: a
   > different stop reason, an answer that was no stop reason at all, or a connection that ended
   > with the prompt unanswered. That last shape is decidable **only when the connection ends** —
   > until then the trace holds no evidence that the answer is not still coming, which is what the
   > turn sitting in `Cancelling` says on screen.
   >
   > **And the load-ordering rule is built beside it**, which cost what that sentence promised: a
   > variant next to `Annotation::Cancellation`, and no change to how any of it is surfaced. The
   > specification states it twice — the agent *"MUST replay the entire conversation to the Client in
   > the form of `session/update` notifications"*, and *"when all the conversation entries have been
   > streamed to the Client, the Agent MUST respond to the original `session/load` request"* — and
   > the rule is armed by a `session/load` this client sent and judged where its answer crosses.
   >
   > **What is decidable from frames is the boundary, not the whole**, and the admission rule is what
   > draws the line: nobody here can know what "the entire conversation" was, so the annotation is
   > made in one shape only — the load answered with *nothing* replayed, while the record already
   > holds a conversation in that session. Three things draw nothing, each for a stated reason. A
   > load whose replay crossed first did what was asked. A load the agent *refused* restored no
   > session, so no conversation was owed. And a load of a session this connection has never heard a
   > word in replayed nothing into a session nothing is known to have been said in — which the trace
   > cannot tell from an agent withholding a conversation, and annotating there would be inventing a
   > rule rather than reporting one. Silence, here as everywhere, is not approval.
   >
   > `session/resume` draws nothing at all: it MUST NOT replay, so an agent that replayed nothing
   > into a resumed session did what resume is for. A resume that replays anyway is visible in the
   > timeline and in the trace, and is not annotated — the rule list grows when the specification
   > supplies another qualifying MUST, and this ring landed one.

   **The fourth ring adds no rule, and three candidates are named rather than left silent**
   ([#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96)). The admission rule has two
   halves and each of these fails a different one, which is worth seeing together:

   - **An agent that advertised a capability and then refused it.** Decidable from two frames, and
     there is no MUST: the schema says agents MUST support `session/new`, `session/prompt`,
     `session/cancel` and `session/update` as a baseline and **MAY** support anything else "by
     specifying additional capabilities". Advertising is an offer, not a promise the specification
     enforces. It is a finding on the panel row ([§7.7](#77-the-fourth-ring-what-was-driven)) and
     nothing more. This is the second candidate to fail on the MUST half rather than the
     decidability half, which is the pattern: what this tool can *see* has outrun what the
     specification is willing to *require*.
   - **An agent that called a client method this client declined.** This document has said in its
     own voice that "whatever the inspector does not advertise, a conformant agent MUST NOT call"
     ([§7](#7-the-procedure-cut)) — and the schema does not say it. What it says is "Only available
     if the client advertises `fs.writeTextFile`", which is the same construction it uses for
     *"Only available if the Agent supports the `sessionCapabilities.close` capability"* — a
     construction this tool deliberately reads as non-binding every time it sends a call an agent
     never advertised ([§7.5](#75-the-second-ring-the-session-lifecycle)). Annotating an agent for
     a rule this tool exempts itself from, sourced from a sentence this tool wrote, is not a report
     of a rule. Such a call stays what [§7.3](#73-decline-by-capability) already makes it: logged,
     answered method-not-found, and first-class evidence.
   - **The baseline MUST itself** — an agent answering `-32601` to `session/new` or
     `session/prompt`. This one **qualifies**, and it is recorded here rather than added, because it
     is a rule about the procedure cut and not about this ring, and because only two of the four
     baseline methods can arm it: `session/cancel` is a notification and can never be answered at
     all, and `session/update` is the agent's to send, so "does not support it" would be a model of
     agent state. It wants its own ticket.

   **The third rule arrives with the session settings ring**
   ([#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84)), and the admission rule is
   again what decides its shape rather than its subject:

   > It is armed by a `session/set_config_option` this client sent that **succeeded**, and judged on
   > the option set the answer carried: a response omitting the very id just set cannot be the
   > complete set the specification requires, because that option provably exists — this client had
   > just changed it. Two frames decide it and no model of agent state is needed, which is the whole
   > of the admission rule.
   >
   > **The looser formulation is rejected**: annotating *any* response that omits a previously
   > advertised option would fire on lawful behaviour, because an agent whose option set legitimately
   > shrinks is indistinguishable from one sending a delta, and the trace cannot tell them apart. A
   > rule that cannot be wrong about a conformant agent is the only kind admitted.
   >
   > **No rule is added for an agent that stays silent after `session/set_mode`.** The response is
   > `{}` and nothing obliges the agent to restate the mode, so permitted silence is not reported as
   > a fault — it is reported as *acknowledged and unconfirmed* on the surface
   > ([§7.6](#76-the-third-ring-session-settings)), which is a statement about what is known rather
   > than a complaint.
   >
   > And a **fourth candidate is named and left off the list**: an agent sending a boolean config
   > option to a client that never claimed the capability breaks a MUST NOT and would be decidable
   > from frames — but the inspector's own claim is fixed and always made
   > ([§7.3](#73-decline-by-capability)), so the rule could never arm. It is recorded here because a
   > later ring that makes the claim configurable ([§1.1](#11-what-is-deliberately-not-built)) gets
   > this annotation as part of the bargain.
   >
   > **A fourth rule is admitted by the fifth ring, and two more candidates are left off with it**
   > ([§7.8](#78-the-fifth-ring-elicitation)). Admitted: an agent that opens a second URL
   > elicitation under an `elicitationId` the first is still outstanding under. The specification
   > states it as a MUST, a client is required to treat the id as opaque — so the id is the only
   > thing a completion names the interaction by — and two frames decide it with no model of agent
   > state. *Outstanding* is the rule's own word and this client keeps it literally: a completed,
   > refused or abandoned elicitation has left the set, and an id whose elicitation was declined is
   > free again, because a refusal is the interaction not happening.
   >
   > Left off, and for the two different reasons this list already distinguishes. **An agent
   > requesting a mode the client did not advertise** breaks a stated MUST NOT and is decidable —
   > but this client claims both modes, fixed for every connection, so the rule could only arm on a
   > mode outside the two, which [§8](#8-unknown-traffic-the-raw-first-rule) already treats as
   > unknown traffic rather than misconduct. That is the boolean-config-option case above, in a
   > second shape, and it comes back the same way if the claim ever becomes configurable. **URL mode
   > required for sensitive interactions** — the specification forbids asking for secrets through a
   > form — cannot be decided without judging which fields are sensitive, and judging is a grade
   > rather than a rule.

   **And the third rule is built**, at the cost the second one promised the day another qualifying
   MUST was admitted: a variant beside `Annotation::Cancellation` and `Annotation::Replay`, and no
   change to how any of it is surfaced. It is judged where the answer to the set is read, from the
   two frames that decide it — the `session/set_config_option` this client sent, and the answer that
   said it succeeded.

   > **The option list is read off the frame, not off the decoded answer**, and that is the one
   > decision the rule's statement does not already contain. The typed layer skips option entries it
   > cannot deserialize ([§7.6](#76-the-third-ring-session-settings)), so an option an agent really
   > sent in a shape this client cannot read is absent from the store while sitting in the trace —
   > and a rule reading the store would claim the agent omitted something the record shows it sent.
   > A rule about what crossed must not turn on what this client recognized
   > ([§8](#8-unknown-traffic-the-raw-first-rule)); the load rule reads a session id off the envelope
   > for the same reason.
   >
   > **The floor is a `configOptions` being there at all.** An answer without one carries no option
   > set to judge, and what the surface does with it is say the set did not happen — claiming a
   > shortfall in a list the agent never sent would be inventing a rule rather than reporting one.
   > One that *is* there and is not a list of options is judged like any other list the id is not
   > in: the id is provably not in it, no conformant agent could have sent it, and the typed layer
   > reads it as no options at all — so the row leaves the surface, and the annotation is what says
   > why.
   >
   > **Testy is the conformant agent here and cannot be the subject** — it publishes one select
   > option and answers a set with its whole list — so the violation is a scripted one-liner through
   > the same seam, and the rejected looser formulation is asserted as rejected: an agent that drops
   > a *different* option from the answer draws nothing, with the store showing the shrink it took
   > the agent's word for.
8. **No performance facts.** Frame volume per turn, stderr volume under a chatty agent, and
   Dioxus desktop's rendering behaviour over a fast-appending log are all unmeasured; the
   research's numbers stop at the protocol.
9. **System appearance has a release check and still needs a rendered desktop result.** The switch
   ([§9](#9-screens)) and its production stylesheet are now one release check
   ([desktop smoke matrix](desktop-smoke-matrix.md)): System removes `data-theme` and the generated
   preferred-dark selector applies Inspector Dark only under the operating-system media query;
   Light and Dark set Inspector Light and Inspector Dark explicitly. Persistence,
   malformed/future settings fallback and all
   three DOM mappings are automated. The headless build environment could not render WebKit under
   Xvfb, so the matrix records that the live OS-light/OS-dark switch at both sizes remains owed.
   One limitation is recorded rather than worked around: on Linux, WebKitGTK derives
   `prefers-color-scheme` from GTK, so a desktop that sets neither
   `gtk-application-prefer-dark-theme` nor a GTK theme name ending in a dark suffix reports Light
   whatever the user actually prefers. If that turns out to be the common case it is a follow-up,
   not a defect in the switch. **The window's own chrome is deliberately left alone** for the same
   reason: tao's `set_theme` writes `gtk-application-prefer-dark-theme` on Linux — the very signal
   System reads — so matching the title bar to the choice would corrupt the answer, and picking
   Light once could leave System reporting Light forever after.
10. **Three things the session settings ring carries as unverified**
    ([§7.6](#76-the-third-ring-session-settings)). None of them may be presented as fact until
    something drives them. **All three have now been driven**, and the entries below say what each
    one found — but the first is an observation about *one agent* rather than a rule, and stays
    written as one:

    - **What an agent does with a mode or config option change arriving mid-turn.** The
      specification is silent, so the inspector permits the change and reports what happens; it does
      not claim what should happen. **Both halves have now been driven, against one agent**: Testy
      answers a `session/set_mode` and a `session/set_config_option` sent into a live turn from the
      same connection, mutates its state, and — for the mode — says nothing about it afterwards; its
      prompt goes on waiting and ends the way it was going to end
      (`crates/core/tests/session_settings.rs`). That is one agent's behaviour observed, not a rule
      discovered, and it is still all this document may say about the case.
    - **Whether the upstream test agent can be made to publish `modes` on one session setup path and
      omit them on another** — the case that proves the replay-versus-response rule, where a
      `session/load` replays a mode change into a response carrying no `modes`.
      ~~It should be confirmed by driving Testy before a scripted agent is written for it.~~ —
      **driven, and it cannot.** Testy answers `session/new`, `session/load` and `session/resume`
      with the same two fields off the same session state, and replays nothing at all, so it can
      neither omit `modes` on one path alone nor announce a mode ahead of an answer
      (`crates/core/tests/session_settings.rs`'s
      `testy_publishes_its_modes_on_every_path_that_opens_a_session`, which drives all three paths so
      that this stays a finding rather than an assumption). The collision is therefore a scripted
      agent's, and `a_replay_that_states_a_mode_the_answer_did_not_offer_yields_no_mode_control` is
      where it is driven ([§7.6](#76-the-third-ring-session-settings)).
    - **What "an option kind that is neither select nor boolean" actually reaches core as.**
      ~~Read from `agent-client-protocol-schema` 1.6 and not yet driven~~ — **driven, and the
      reading held.** `SessionConfigKind` is an internally-tagged enum with exactly `select` and
      `boolean` and no catch-all arm, and every option list on the wire is deserialized with the
      item-skipping combinator, so an option whose `type` is a *third* string does not decode to an
      unknown kind: a scripted agent publishing one beside a select gets the select through and the
      third-kind option **vanishes from the list before core sees it**
      (`crates/core/tests/session_settings.rs`). The read-only row
      ([§7.6](#76-the-third-ring-session-settings)) is therefore a forward-compatibility arm for the
      day the schema adds a variant this client does not drive — the same shape as the `_` arm on the
      eleven update variants ([§7.2](#72-serviced-agent--client)) — and not something a scripted
      agent can arm today, which is why the ring shipped two scripted fixtures and not three. The
      seam-2 component test the ring also wanted for it is not writable either, for a second reason:
      the kind is a `#[non_exhaustive]` enum in another crate with no third variant, so nothing
      outside that crate can build one to render. The arm is carried by the compiler rather than by
      a test. The surface's under-reporting is exactly the limit that section names, and the frame is
      in the trace, whole.
11. **Three things the fourth ring carries as unverified**
    ([#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96)), named the way the ring
    before it named its own:

    - **A capability key v1 does not define is invisible on the panel, and this ring did not fix
      it.** `AgentCapabilities` has no catch-all, so an agent advertising an extension key has it
      discarded by serde before core sees anything; core keeps no raw copy of the `initialize`
      answer; the trace has the frame whole but drops its oldest, and a second capture would cross
      [§6.2](#62-the-trace-decorator)'s single-capture rule. Naming such a key on the panel was
      wanted and is not buildable through any seam this tool has, so the limit is stated rather than
      implied — the same admission [§7.6](#76-the-third-ring-session-settings) makes about the
      settings surface being a decoded view. The frame is in the trace, complete
      ([§8](#8-unknown-traffic-the-raw-first-rule)), and that is where an unrecognized claim is read
      today. Note the `unstable_*` capabilities (`fork`, `providers`, `nes`) are **not** this case:
      v1 defines them and this client deliberately does not decode them, which the panel already
      says.
    - **Testy cannot be the subject for `logout`.** It clears its authenticated methods, always
      succeeds, and nothing it holds ever reads that set — so a logout against it has no observable
      consequence and cannot be made to refuse. Every refusal, and anything about what a logout does
      to a live session, needs a scripted one-liner through the same seam, the way the boolean
      config option fixtures did. What Testy *can* answer is whether a logout is answered at all.
    - **What the panel's error line and the record each own is undecided in one place.** The
      desktop's `Failed` signal holds the last lifecycle refusal, latest-wins across four
      affordances and cleared when the user retries, and it reports `session/new` — which is gated
      on nothing and therefore has no capability row to live on ([§9](#9-screens)). It stays: the
      record is per-capability and cannot carry either of those. But the same refusal is now stated
      twice on one panel with two lifetimes, and whether that reads as thorough or as two facts
      disagreeing is a question the built version answers and this document cannot.
12. **The accessibility posture now has a release matrix, but its platform screen-reader pass remains owed.** The
    matrix covers native controls, every Affordance, disclosures, complete Console tab navigation,
    visible focus and reduced motion; automated source/markup contracts pass, while the headless
    environment could not supply a rendered accessibility tree. The host repo's smoke pass
    ([`docs/a11y-smoke-pass.md`](../../../docs/a11y-smoke-pass.md))
    is about the library's *web* build under Chromium and says so; it excludes the WebView leg,
    which is the only leg this tool has, so it is precedent rather than coverage. The remaining
    platform questions are recorded rather than confused with keyboard coverage; the source-level
    navigation gaps found by that review are resolved:
    - ~~**The failed-spawn escalation is silent.**~~ The one automatic screen change in the window
      ([§9](#9-screens)) now changes text in a persistent polite live region: “Failed to start.
      Diagnostics opened.” or “Connection lost. Diagnostics opened.” It does not make the evidence
      list live. Focus elsewhere in the window stays put; focus already inside the Trace surface
      being replaced moves to the Diagnostics tab instead of falling back to the document.
    - ~~**A reveal moves the viewport and never focus.**~~ A Timeline-to-Trace reveal focuses the
      named Frame's native summary after opening the Console; a Trace-to-Timeline reveal focuses the
      marked entry, which is programmatically focusable but remains outside ordinary Tab order. Both
      destinations retain the visible selected edge as well as the shared keyboard focus treatment;
      Reach is absent once every referenced Frame has left the bounded Trace.
    - ~~**The permission shortcut moves only the viewport.**~~ “go to it” uses that same Timeline
      destination path, so the request's row receives focus instead of moving out from under the
      keyboard reader.
    - ~~**The composer replaces its focused Affordance.**~~ Send and Stop are one persistent button whose
      behavior and accessible name follow the Turn. Plain Enter sends, while Shift+Enter and Enter
      during IME composition remain text input. A separate persistent polite region announces only
      Turn endings; streamed content is never live.
    - ~~**Status changes and native disabling can discard the active control.**~~ Command autofocuses
      only on the Launch form's first mount. Launch, Disconnect, permission choices, elicitation
      answers, setting controls and
      Trace actions keep their DOM nodes, use guarded ARIA-disabled states and leave future Tab order
      without ejecting focus already on them; native setting selects keep their node and temporarily
      disable the options beneath it.
    - ~~**The clipboard fallback borrows focus and does not return it.**~~ The fallback remembers the
      active control before attempting the asynchronous Clipboard API and restores it without moving
      the viewport, including when the legacy copy call throws.
    - ~~**A listing row's buttons do not say which session they are about.**~~ Load/resume, close and
      delete now carry target-specific accessible names, and the roots disclosure names its Session.
13. **The window's layout nouns are unsettled, and one word collides.** *Panel* is consistent across
    this document and the markup — a bounded, named box with a header — and *screen* is §9's word
    for a named area of the app; the two nest, and the Console is both. *Region* is the one genuine
    collision: it was this document's word for a grid row and it is also ARIA's, in load-bearing
    prose in §9. The grid-row uses are now *row*, which retires the collision without coining
    anything. No glossary term is owed: [`CONTEXT.md`](../CONTEXT.md) holds language about claims
    and traffic, and a layout noun is neither — if one is ever owed it belongs beside the rest of
    the design vocabulary in [ADR 0004](adr/0004-the-design-vocabulary.md). What would force it is a
    yield rule stated *once* over all the window's parts ("every X gives way except the composer"),
    since today each part states its own.
