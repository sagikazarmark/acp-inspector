# ACP Inspector

[![ci](https://img.shields.io/github/actions/workflow/status/sagikazarmark/acp-inspector/ci.yaml?style=flat-square&label=ci)](https://github.com/sagikazarmark/acp-inspector/actions/workflows/ci.yaml)

**Drive and observe an [Agent Client Protocol](https://agentclientprotocol.com) agent from your desktop.**

Point it at an agent command and it spawns the agent, speaks ACP v1 to it as a well-formed client,
and shows both conversations at once: the turn as a timeline and the wire as a trace of every
JSON-RPC frame exactly as it crossed, with the agent's stderr beside it.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="design/redesign/populated-1440-dark.png">
  <img alt="The inspector with Testy connected: the connection and session settings on the left, the turn timeline in the middle, and the JSON-RPC trace with the selected frame on the right." src="design/redesign/populated-1440-light.png">
</picture>

> [!WARNING]
> The inspector does not sandbox agents. A real agent reads and writes files and runs commands
> **itself, in its working directory, with your privileges**.
> Point it at a directory you are prepared to have changed.
>
> Read the [known limitations](docs/guide.md#known-limitations) before you start.

## Features

- 💬 **Turn timeline:** messages, tool calls, permission requests and elicitations, answered inline.
- 🔌 **Raw JSON-RPC trace:** every frame in both directions, with the agent's stderr beside it.
- 🧩 **Raw-first:** traffic it does not understand is recorded verbatim, never dropped.
- ✅ **Conformance annotations** where the captured traffic decides one of the spec's MUSTs.
- 🗂️ **Session lifecycle:** create, list, load, resume, close and delete, gated on what the agent advertised.
- 📦 **JSONL export** of the whole trace as evidence.
- 🤖 **Headless CLI** for smoke-testing an agent in CI.

## Quick start

Download the app for macOS, Linux or Windows from the
[latest release](https://github.com/sagikazarmark/acp-inspector/releases/latest). The downloads
are not signed, so the first launch needs one extra step; see
[supported platforms](docs/guide.md#supported-platforms).

Or build from source with Rust, [just](https://just.systems) and, on Linux, the WebKitGTK
libraries (see [prerequisites](docs/development.md#prerequisites), or run `devenv shell`):

```sh
just run
```

The **Connect** dialog asks for an agent. Command is an **executable only**, and each line of
Arguments is one argument, with no shell quoting or expansion. To try it against Claude Code
through its ACP adapter:

| Field             | Value                                   |
| ----------------- | --------------------------------------- |
| Command           | `npx`                                   |
| Arguments         | `@agentclientprotocol/claude-agent-acp` |
| Working directory | the project the agent should work in    |

The adapter uses the same authentication Claude Code does. Launch, and the handshake runs to an
open session; type a prompt and press Enter to watch the turn stream in.

**Next:** [point it at Testy](docs/guide.md#pointing-it-at-testy), a deterministic agent that
exercises every screen.

### Headless

Smoke-test an agent without a window, and keep the trace. The CLI is in each
[release](https://github.com/sagikazarmark/acp-inspector/releases/latest), or build it with Cargo:

```sh
cargo install --locked --git https://github.com/sagikazarmark/acp-inspector acp-inspector-cli
acp-inspector-cli run --timeout 120 --prompt 'Say hello' --export acp-trace.jsonl -- ./my-agent --acp
```

The exit code says whether the agent started, finished its turns, and kept the MUSTs the
inspector checks; see the [CLI reference](docs/cli.md).

## Documentation

- [User guide](docs/guide.md): launching agents, sessions, permissions and elicitations, MCP servers, attachments, the trace, platforms, known limitations.
- [Headless CLI](docs/cli.md): options, exit codes, CI usage.
- [Client services](docs/client-services.md): answering `fs/` and `terminal/` calls by hand or from a rules file.
- [Trace export](docs/trace-export.md): the JSONL schema.
- [Architecture](docs/architecture.md), [decisions](docs/adr/), [glossary](CONTEXT.md).
- [Development](docs/development.md): building, testing, layout, releasing.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

The disk image and the application bundle carry the third-party notices in `THIRD_PARTY_LICENSES.html`.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
