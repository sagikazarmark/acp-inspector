# User guide

[Documentation index](../README.md#documentation)

Start with the [quick start](../README.md#quick-start), then choose your task:

- [Launch an agent](#launching-an-agent)
- [Try every screen with Testy](#pointing-it-at-testy)
- [Find your way around the window](#the-window)
- [Manage sessions](#sessions)
- [Answer permission requests and elicitations](#permission-requests-and-elicitations)
- [Supply MCP servers](#mcp-servers)
- [Attach files to a prompt](#attachments)
- [Read, clear and export the trace](#the-trace)
- [Change the appearance](#appearance)
- [Install on macOS, Linux or Windows](#supported-platforms)
- [Know the limits](#known-limitations)

The architecture spec is [`architecture.md`](architecture.md), where every decision below is
argued, in the words of [`CONTEXT.md`](../CONTEXT.md), the project's glossary.

## Launching an agent

```sh
just run      # cargo run -p acp-inspector
```

Fill the spawn form with an **executable only** in Command (for example `npx`), and the package
on its own line in Arguments. Each nonblank argument line is one argument: a flag and its value go
on separate lines, while a path containing spaces stays on one line **without shell quotes**. No
shell expansion is performed; surrounding whitespace is trimmed. Environment takes one
`KEY=VALUE` per line. An empty working directory uses the inspector's own; a relative directory
resolves against it. The Agent starts there and `session/new` receives an absolute path. Launch,
and the console fills as frames cross; its other tab carries the agent's stderr, and the console
expands onto it if the agent never starts. Once the session is open, type a prompt and press enter
(shift-enter for a newline) to watch the turn stream in.

Launching an agent runs the whole handshake, so the composer is ready as soon as there is a
session; a prompt streams into the timeline as the agent sends it, tool calls merge their updates
into one card, the stop button sends `session/cancel`, and the turn's state and its `stopReason`
are read from the stream rather than from the prompt call returning.

For reproducible attachment/MCP acceptance scenarios without building Testy,
launch the Python 3.9+ standard-library fixture `scripts/acceptance-agent.py` as
the Agent. It offers selectable Advertisements, delayed/refused responses and
ordered attachment hash receipts. See the [native acceptance runbook](native-acceptance.md#portable-attachmentmcp-acceptance-agent)
for exact launch fields and scenarios; `just test-acceptance-agent` verifies its
stdio protocol. Testy remains the fixture for Elicitations and permission requests.

### Remembered commands

The spawn form remembers what answered. An invocation whose agent described itself — a session
opened, or an agent that wants a login before it will open one — joins a short list under the
form and on the first-run card: newest first, ten of them, one click to put one back in the
fields, a play button to launch it directly, and a button to remove it. A command that never
started said nothing and is not remembered, because that is a typo and it is still in the fields.
Environment variables are remembered by name only unless **Remember values** was ticked for that
launch; a row whose values were not kept fills the form in with empty values to complete rather
than launching with them blank. It is a convenience and stays one
([§1.1](architecture.md#11-what-is-deliberately-not-built)): nothing to name, edit,
import or export, and the persistent agent catalog is still deferred. The list lives in one JSON
file under the platform's own state directory — `~/.local/state/acp-inspector/recent.json` on
Linux, `~/Library/Application Support/acp-inspector/` on macOS, `%APPDATA%\acp-inspector\` on
Windows, `0600` on Unix because a remembered invocation can carry its environment's values. A file
that cannot be read is moved aside as `recent.json.bak` and the window says so. What it holds and
why is [§15 q6](architecture.md#15-open-questions--validation-gaps).

## Pointing it at Testy

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
them are **declined by capability** by default — this client mediates no filesystem and embeds no
terminal ([§7.3](architecture.md#73-decline-by-capability)) — so each is answered
method-not-found and recorded as a first-class unrecognized entry, which is
[§8](architecture.md#8-unknown-traffic-the-raw-first-rule) doing its job rather than a
failure. Claim them under **Client services** at launch and each becomes a card you answer
instead ([client services](client-services.md)). Answer them and the scenario runs to `end_turn`
([§7.8](architecture.md#78-the-fifth-ring-elicitation)); leave one unanswered and the turn
stays stopped, and the line above the composer says the agent is waiting on you.

## The window

The window is a rail and two screens ([ADR 0009](adr/0009-the-window-is-the-drawing.md)):
the connection on the left with the live session's settings and both parties' capabilities behind
two tabs, the turn timeline in the middle, and the console beside it — the trace and the agent's
stderr as two tabs, opening on the trace, with the frame a reader selects read in a pane at
the foot of the list, and expanding onto stderr by itself when an agent is gone without being asked
to go. A **Split | JSON-RPC full** control in the toolbar says whether the two screens share the
width or the wire takes all of it; a window too narrow for three columns shows one region at a time
and puts a **Details | Session | Messages** bar at its foot. Every affordance the window has is
reachable by name from a command palette (`Ctrl`/`⌘` + `K`).

The typed layer decorates the trace; it never replaces it
([§8](architecture.md#8-unknown-traffic-the-raw-first-rule)). Traffic it does not recognize
— an extension method, an update variant v1 has no name for, a client service the inspector
declined by advertising no capability — becomes a first-class unrecognized entry carrying the
frame verbatim, and an agent that calls a declined service is answered method-not-found with the
call recorded as evidence. An unknown notification, such as an adapter's `_auth/status_update`,
is recorded and left unanswered, since nothing answers a notification. The launch dialog's **Client services** section can instead claim file
reads, file writes and terminals, and answer those calls — or any other unhandled request — by
hand or from a rules file ([client services](client-services.md)).

### Capabilities and authentication

What the agent claimed in its `initialize` result is a tab of the rail: its name and version,
every v1 capability it did and did not advertise, and the auth methods it offers. Beside it, the
client identity and every Client Capability sent in the request remain equally explicit. The
session lifecycle capabilities (`session/load`, `list`, `resume`, `close`, `delete` and
`additionalDirectories`) are each shown with the field they were read from, whether or not the
agent advertised them. **Affordances are gated on the advertisement, not on content.**
Authentication is display-only: `authenticate` can be sent with any advertised method, and a
`-32000 auth_required` becomes a login-needed state that names the advertised methods and points
to the agent's own login, because the inspector embeds no terminal.

### Conformance annotations

Conformance annotations are drawn where the captured traffic decides a MUST and nowhere else:
for example, a `session/load` that answers having replayed nothing, into a session this
connection has already heard the agent talk in. Behaviour the specification leaves open is
recorded, not graded.

## Sessions

Sessions can be created, listed, loaded or resumed, closed and deleted without relaunching the
agent, each where the agent advertised it
([§7.5](architecture.md#75-the-second-ring-the-session-lifecycle)). Opening a session while
another is live is a switch: a running turn is cancelled, waiting requests are answered
`cancelled`, and the timeline is rebuilt, while the trace is untouched. Where both load and resume
are advertised, load is used because it replays the conversation; where only resume is, the screen
says this agent does not support viewing previous messages. The roots a session opens with are
yours to supply where the agent advertised `sessionCapabilities.additionalDirectories`. Corners
the specification leaves undefined (closing the live session, deleting one never opened) are
offered deliberately, and the inspector reports what came back without judging it. The live
session's modes and config options are in the rail, sent with `session/set_mode` and
`session/set_config_option` ([§7.6](architecture.md#76-the-third-ring-session-settings)).

## Permission requests and elicitations

Permission requests are answered where the turn stops for them. A `session/request_permission`
becomes a panel **inline in the timeline**, at the point the turn blocked: the tool call under
question, and a button for every option the agent offered, in the order it offered them. Clicking
one sends `selected` with that option id and the turn goes on; pressing stop while a request is
waiting sends `session/cancel` and then answers every waiting request `cancelled`, which is what
a client owes them and the only way an agent blocked on one gets to end its turn. No modal and no
toast: in ACP these are the main traffic of a turn, not an alert beside it.

An agent's `elicitation/create` in form mode is drawn as a form built from the JSON Schema it sent,
with schemaform 0.6.1 and the registry's daisyUI renderer. Field-local findings and named summary
links are advisory (the email, URI, date and date-time formats are checked too), and the **Raw**
tab holds the exact object the answer will send, so it can send values the schema forbids. A URL
elicitation shows the full URL and its host and opens in your own browser only when you click it.
The adoption decision is [ADR 0013](adr/0013-schemaform-draws-the-elicitation-form.md), and where
the form components come from is
[`crates/app/src/components/PROVENANCE.md`](../crates/app/src/components/PROVENANCE.md).

## MCP servers

**Stdio, HTTP and SSE MCP definitions** can be entered before Launch or in Sessions.
One in-memory list is supplied on subsequent `session/new`, `session/load` and
`session/resume` calls, including shortcuts. Give each server a nonblank name.
Stdio uses an absolute executable path on the Agent's machine; the inspector does
not check that it exists or run it. HTTP/SSE use an absolute HTTP(S) URL with a host;
the inspector does not fetch it. Arguments, environment pairs and headers are ordered
rows: empty arguments/values and duplicate names are preserved. Switching transport
retains the other fields for editing but sends only the selected transport's fields.
Rows can be added, moved and removed. Invalid input blocks opening and identifies
the field to correct.

Editing does not reconfigure the live Session. Explicit reconnect retains the
list; it is not stored in recent commands or saved across application restarts.
The shared editor always states how many MCP definitions the next opening will
carry; clear the list when it should not be reused. MCP input is inspector-supplied,
not recovered from a listed Session. The Agent owns MCP execution, and an answered
Session-opening call does not establish individual MCP connectivity. Definitions
and environment/header values appear verbatim in Trace and exports. HTTP and SSE
are independently gated on `mcpCapabilities.http` and `.sse`. Launch allows drafting
either transport before the next Agent is known, then validates its snapshot after
initialize. Unsupported definitions keep the Connection open with no first Session
and the Sessions editor available for correction; nothing is silently omitted.
Later opens validate before switching Sessions. Their capability rows report the
Session call's answer/refusal, not MCP connectivity. New/load send an empty
`mcpServers` array when the list is empty;
the pinned schema omits the empty field on resume.

## Attachments

Where the Agent advertises `promptCapabilities.image`, the composer accepts PNG,
JPEG, GIF and WebP; `promptCapabilities.audio` admits WAV and MP3;
`promptCapabilities.embeddedContext` admits UTF-8 text/source files and PDFs. Mixed drafts
share a limit of eight attachments, 5 MiB each and 6 MiB of original bytes total.
Select multiple files, drop them onto the composer, or paste clipboard images
where the Agent advertises images. Ordinary text paste is preserved, including
text alongside an image. Clipboard bytes are those supplied by the platform;
the inspector does not re-encode them. Batches append in arrival
order; files keep the order the picker/drop source supplied. Each row has a name,
loading/failure state or preview/MIME/byte count, and an individual Remove control.
Repeated selections intentionally add duplicates. Send waits for reads to finish
and failed rows or overflow notices to be dismissed, so a bad file is never
silently omitted. Send attachments alone or after a text block. Image/audio MIME comes
from the signature; original media bytes are base64 encoded without resizing or transcoding.

Text files use embedded `resource` blocks with `text/plain`, exact UTF-8 text (including
BOM and line endings), and an escaped native file URI or unique browser attachment URI.
The literal text preview shows at most 2000 characters. PDFs are identified by a
PDF 1.0–1.7 or 2.0 header line and embedded as binary resources with `application/pdf`,
original bytes in base64, and a file or attachment URI. Their rows show metadata;
the inspector does not render PDFs or validate the rest of the document. The
embedded-context Advertisement does not promise the Agent can interpret PDFs.
Text input rejects invalid UTF-8 and binary controls; other recognized binary formats
are refused. Audio has native
playback controls without autoplay. Playback errors are shown independently of sending;
signature identification does not guarantee that the WebView can decode the file.

A complete outgoing prompt over 10 MiB is refused before
it crosses the wire. Attachment drafts reset on Session switches.
Local refusal retains the draft for correction. Admission enqueues the Frame
synchronously for the selected Session before a later Stop or Session switch;
the Agent's eventual answer is awaited separately.

On Linux, native file selection needs a desktop file-picker portal or `zenity`
(included in `devenv shell`). Audio playback needs GStreamer's base, good and ugly
plugins, also exposed by the shell. Non-PDF binary resource attachments and clipboard audio input
remain deferred. Non-file text/URL drops are not attachments; file drops outside
the composer do not navigate the inspector window.

## The trace

### Export

The trace exports as evidence. **Export** writes every frame the trace holds as JSONL — one
record per frame with its timestamp, its connection, its direction and the frame verbatim, behind
a self-describing header that warns nothing is redacted. The shell shows the temporary file's
full path with **Copy export path** and desktop **Open containing folder**, reporting pending
operations and failures beside it. Copy or move the file elsewhere to keep it: system temporary
storage is not a durable archive. Each export creates a new file, never overwriting earlier evidence.
The schema is pinned in [trace export](trace-export.md), because replay and
analysis tooling are meant to read it.

### Clear and reconnect

**Clear** drops the frames captured so far and nothing else — an export already written is
untouched, an export taken afterwards carries only what followed and says in its header how many
frames it never had. The trace is never cleared by a disconnect, because an agent that just
died is when its frames are worth the most. Which is why **every row says which connection it
crossed** — `#1`, `#2`, the ordinal the export writes for that same frame — and the row where one
agent gave way to the next draws the boundary: one trace can hold two agents' traffic, and both
agents' JSON-RPC ids start again at 1.

### Retention

**Retention is bounded and loss is explicit.** Trace keeps the newest **10,000 Frames within
64 MiB of raw UTF-8**, dropping whole oldest Frames until both budgets fit. Its missing-Frame
count stays visible and appears in every export, including Frames removed by Clear. Diagnostics
keep 10,000 entries within 8 MiB, with their own missing-entry count. The Timeline keeps up to
1,000 entries, 2,000 raw Frames and 16 MiB of raw evidence; whole entries leave with their
evidence, and removed entries, Frames, bytes and Turn records are counted on screen. Pending
and answering Blocking Requests and the latest command Advertisement are exempt, so requests
remain answerable and can exceed those budgets. Turn records needed by retained entries or the
open Turn stay; otherwise up to 1,000 are retained. These are evidence budgets, not an RSS limit
or a durable archive. See [architecture §10](architecture.md#10-the-jsonl-trace-export).

### Reading large traces

Console lists use **200-row pages** with Older, Newer and Latest controls; filters search all
retained evidence and reveal-by-ordinal opens the destination page. Trace and Diagnostic row
previews are literal prefixes of at most 512 UTF-8 bytes. A longer Diagnostic offers **Read
complete line**, opening adjacent raw byte windows. Frames larger than 16 KiB also use literal
byte windows in the selected-frame pane, rather than laying out the entire payload at once.
**Copy always takes the complete original Frame or Diagnostic line**, regardless of the window
being read. Export takes the complete retained Trace snapshot on activation and writes it in the
background, showing pending status and then the path or failure.

## Appearance

The window has a light theme and a dark one. A **System / Light / Dark** switch in the top bar
chooses between them; System is the default and follows the operating system, and the choice is
remembered across restarts in `settings.json` beside the recent commands — its own file, ordinary
permissions, and one this cannot read leaves the window on System rather than refusing to open.
Light maps to the purpose-built `inspector-light` daisyUI theme and Dark maps to
`inspector-dark`; System leaves `data-theme` absent so the operating-system preference selects
between them.
The five colours that mean something — what we sent, what the agent said, trouble, live, and
traffic nobody has a name for — mean the same thing in both schemes, so a screenshot stays readable
evidence whichever one it was taken in. Whether System tracks the operating system is
unverified on a real desktop, along with one known Linux limitation:
[§15 q9](architecture.md#15-open-questions--validation-gaps).

An **Indent** switch sits beside it, off by default and remembered in the same file. On, a payload
that is JSON is drawn with whitespace between its tokens wherever this window draws bytes to be
read — the frames under an entry this window has no rendering for, a tool call's raw input and
output — and **only where somebody is looking**, so a trace of ten thousand frames lays out the one
being read rather than all of them. Frames up to 16 KiB are laid out and coloured by what they are
made of; larger raw Frames use the literal, unformatted byte windows described
[above](#reading-large-traces). Indentation **adds whitespace and changes nothing else** — the
text is validated as JSON and then re-spaced rather than decoded and printed back, so an object's
keys keep the agent's order, two fields that share a name both survive, and numbers and escapes
stay spelled the way they arrived. And it stops at the reader's eyes: the trace records, the
export writes and every copy takes the bytes that crossed, whatever the switch says.

## Supported platforms

Download the app from the [latest release](https://github.com/sagikazarmark/acp-inspector/releases/latest).

| Platform | Download | Status |
| --- | --- | --- |
| **macOS 11 or later**, Apple Silicon and Intel | `.dmg` | Supported. |
| **Linux**, x86_64 and arm64 | `.deb` (Ubuntu 22.04, Debian 12 or newer), or a `.tar.xz` of the executable | CI builds and tests it on Ubuntu. |
| **Windows 10 or 11**, x86_64 | `.msi`, or a `.zip` of the executable | Untested: CI only checks that it compiles. |

The [headless CLI](cli.md) is in the same release, as an archive per platform.

Every download carries `THIRD_PARTY_LICENSES.html`, the notices for every dependency compiled in,
and the release lists each file's SHA-256 in `SHA256SUMS`. The downloads are **not signed**, so
macOS and Windows ask before the first launch.

**macOS.** Open the disk image and drag **ACP Inspector** onto Applications. The first time it
opens, macOS says it cannot verify the app: choose **Done**, then open **System Settings → Privacy
& Security** and choose **Open Anyway** for ACP Inspector. Or clear the quarantine flag yourself:

```sh
xattr -dr com.apple.quarantine "/Applications/ACP Inspector.app"
```

**Linux.** Install the package with its dependencies, WebKitGTK 4.1 among them:

```sh
sudo apt install ./acp-inspector_*.deb
```

The `.tar.xz` holds the bare executable for other distributions; it needs WebKitGTK 4.1, GTK 3 and
libxdo installed.

**Windows.** SmartScreen says "Windows protected your PC": choose **More info**, then **Run
anyway**. The app uses the WebView2 runtime, part of current Windows 10 and 11; the `.msi`
installs it where it is missing.

To build from source instead, see [development](development.md#prerequisites).

## Known limitations

- **The inspector does not mediate files or terminals.** By default it advertises no `fs` or
  `terminal` capability and answers those requests method-not-found, so a real agent reads and
  writes files and runs commands **itself, directly, in its working directory, with your
  privileges**. Nothing is sandboxed or confirmed except what the agent chooses to ask permission
  for. Point it at a directory you are prepared to have changed. A launch can claim these services
  so you can [answer each call by hand or from a rules file](client-services.md), but the
  inspector never performs them: it does not read or write your files or run commands for the
  agent.
- **Disconnect ends the agent's process group, not every descendant.** A process the agent starts
  that leaves the group (a daemon, a `setsid` child) keeps running after disconnect.
- **Remembered environment values are plaintext, when you ask for them.** The spawn form's recent
  commands keep only the *names* of the variables you set, unless you tick **Remember values** for
  that launch; then the values, API keys included, are saved in plaintext in `recent.json` under
  the platform's state directory (`0600` on Unix). Lists written by earlier versions kept values
  for every launch and still do until you press **Forget saved values** on the row. See
  [where it is kept](#remembered-commands).
- **ACP v1 as of `agent-client-protocol-schema` 1.11.** Messages added to the stable schema later
  are shown raw, as unrecognized traffic, rather than decoded.
- **One agent and one live session at a time**, over stdio only.
