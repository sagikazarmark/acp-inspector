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
- Linux and Windows: build from source. Linux is exercised by CI and the
  native acceptance runs; Windows is only compiled in CI. Neither is packaged.

### Added

- **Connection.** Spawn an agent from a command, arguments, environment and
  working directory; whole JSON-RPC frames in both directions; the agent's
  stderr as a line-timestamped diagnostic channel; disconnect and relaunch
  without losing the trace. The last ten invocations that answered are
  remembered.
- **Typed ACP v1 client** (schema crate 1.7). Drives `initialize`,
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
- **Prompt content.** Image, audio, text and PDF attachments where the agent
  advertises them; slash commands from `available_commands_update`.
- **MCP server definitions** (stdio, HTTP, SSE) supplied on session opening.
- **Conformance annotations** where captured traffic decides a MUST (for
  example, a `session/load` that answered without replaying).
- **Trace console.** Filters, paging, a selected-frame pane with optional
  indentation that never changes the bytes, and copy of the complete original
  frame. Bounded retention with explicit, counted loss.
- **JSONL trace export** with a self-describing header
  ([schema](docs/trace-export.md)).
- **Window.** Light and dark themes following the system or chosen, a command
  palette, keyboard navigation, and a single-region layout for narrow windows.

### Known limitations

See [Known limitations](README.md#known-limitations) in the README.

[Unreleased]: https://github.com/sagikazarmark/acp-inspector/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sagikazarmark/acp-inspector/releases/tag/v0.1.0
