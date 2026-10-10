# Changelog

All notable changes to the ACP Inspector are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html); before 1.0 a minor
version may change the trace export, the state files or the window.

## [Unreleased]

## [0.1.0]

The first release: a desktop tool that spawns one Agent Client Protocol agent
over stdio, drives it as a well-formed v1 client, and shows the turn and the
wire side by side.

### Platforms

- macOS 11 or later: a disk image per architecture (Apple Silicon, Intel).
- Linux and Windows: build from source, or build an archive of the binary
  with cargo-dist (`just dist-build`) on the platform it is for. Linux is
  exercised by CI and the native acceptance runs; Windows is only compiled in
  CI. The archives are not built or tested on CI.

### Added

- **Connection.** Spawn an agent from a command, arguments, environment and
  working directory, with inherited variables unset or the environment
  started empty (and a minimal-environment preset); whole JSON-RPC frames in
  both directions; the agent's stderr as a line-timestamped diagnostic
  channel; disconnect (stdin closed, then SIGTERM, then SIGKILL) and relaunch
  without losing the trace. The last ten invocations that answered are
  remembered, with their variables by name only unless values are asked for;
  each can be launched, refilled or removed from the form and the first-run
  card.
- **Typed ACP v1 client** (schema crate 1.11). Drives `initialize`,
  `authenticate`, `logout`, `session/new`, `session/load`, `session/resume`,
  `session/list`, `session/close`, `session/delete`, `session/prompt`,
  `session/cancel`, `session/set_mode` and `session/set_config_option`. Traffic
  it does not recognize is kept verbatim as a first-class entry, never dropped.
- **Turn timeline.** Message, thought and plan chunks, tool calls merged into one
  card with diffs and raw input/output, the turn state and `stopReason` read
  from the update stream.
- **Permission requests and elicitations** answered inline where the turn
  stops: permission options, schema-driven elicitation forms (schemaform 0.5)
  with a Raw tab, and URL elicitations opened in the system browser on click.
- **Capabilities.** What both parties claimed on `initialize`, every v1
  capability shown whether advertised or not, auth methods, and the session
  lifecycle capabilities with the field each was read from. Actions are gated
  on the advertisement.
- **Sessions.** Create, list, load, resume, close and delete sessions without
  relaunching; additional directories; session modes and config options.
- **Prompt content.** Image, audio, text, PDF and other binary attachments
  where the agent advertises them; resource links, which every agent accepts;
  slash commands from `available_commands_update`.
- **MCP server definitions** (stdio, HTTP, SSE) supplied on session opening.
- **Conformance annotations** where captured traffic decides a MUST (for
  example, a `session/load` that answered without replaying).
- **Trace console.** Filters by kind, direction, method and id; paging; a
  selected-frame pane with optional indentation that never changes the bytes,
  and copy of the complete original frame. Requests are paired with their
  responses, with durations, jumps between the two, and live elapsed time on
  calls still waiting, which can be cancelled with `$/cancel_request`. Frames
  that are not UTF-8 are kept and flagged; frames over 10 MiB leave a counted
  placeholder. An optional merged view interleaves the agent's stderr. Bounded
  retention with explicit, counted loss.
- **Send by hand.** Compose any request or notification, or edit and resend a
  frame the client sent; hand-sent traffic is marked and never drives the
  typed state.
- **JSONL trace export and import** with a self-describing header
  ([schema](docs/trace-export.md)), optional stderr records, Save As, and
  reopening an export read-only with its timeline rebuilt.
- **Window.** Light and dark themes following the system or chosen, IBM Plex
  bundled, Lucide icons, a command palette covering every action, keyboard
  navigation on ⌘ on macOS, one-click Restart, the toolbar in the macOS title
  bar, a resizable rail that becomes a drawer on narrower windows, and the
  timeline and trace read oldest-first. Launch failures are explained in the
  Connect dialog, and a Finder-launched app finds agents on the login shell's
  `PATH`. The window navigates only to its own pages. A background task that
  panics loses the connection with a diagnostic instead of leaving its calls
  hanging, and a state file that cannot be read is set aside and said.
- **Client services, answered by hand or by rules.** The launch dialog can claim
  `fs.readTextFile`, `fs.writeTextFile` and `terminal`; each claimed call, and
  optionally any other unhandled agent request, becomes a timeline card answered
  with a result or an error, or by the first matching rule of a JSON rules file
  ([docs](docs/client-services.md)). Nothing is read, written or run.
- **Permission policy** for the open session: ask, allow once or reject once
  automatically; never an `*_always` option.
- **Protocol-version override** for `initialize`, with the version the agent
  answered shown and a difference flagged.
- Tool-call names, session notices and context compactions (schema 1.11) are
  decoded and shown.
- **Headless CLI** (`acp-inspector-cli`, `crates/cli`). `run` spawns an
  agent (with `--cwd`, `--env`, `--unset`, `--clear-env`), drives `initialize`,
  `session/new` and each `--prompt`/`--prompt-file` in order, answers
  permission requests by `--permission ask|allow|reject` (never an
  `*_always` option when allowing), cancels elicitations, stops the agent
  gracefully, writes the app's JSONL trace with `--export`, and prints a text
  or `--json` summary with the agent, capabilities, stop reasons, tool calls,
  conformance annotations and diagnostics. Exit codes: 0 ok, 1 agent or
  protocol failure, 2 conformance annotations, 3 `--timeout`, 64 usage, 130
  Ctrl-C. `inspect` counts an exported trace's frames by direction and
  method. No renderer and no new dependency; see "Headless CLI" in the README.

### Known limitations

See [Known limitations](README.md#known-limitations) in the README.

[Unreleased]: https://github.com/sagikazarmark/acp-inspector/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sagikazarmark/acp-inspector/releases/tag/v0.1.0
