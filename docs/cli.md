# Headless CLI

[Documentation index](../README.md#documentation)

`acp-inspector-cli` is the same core without a window: it spawns an agent, runs `initialize` and
`session/new`, sends each prompt in order and waits for its turn to end, answers permission
requests by policy, stops the agent gracefully, writes the JSONL trace the app exports
([trace export](trace-export.md)), and exits with a code a CI job can branch on. It
has no renderer and no dependency the core does not already have, so it builds wherever the core
does.

```sh
cargo build -p acp-inspector-cli          # target/debug/acp-inspector-cli
just cli -- run --prompt 'echo hello' -- "$PWD/.testy/bin/testy"
```

```
acp-inspector-cli run [OPTIONS] -- COMMAND [ARGS...]
acp-inspector-cli inspect [--json] TRACE.jsonl
acp-inspector-cli --help | --version
```

## Options

| `run` option | What it does |
| --- | --- |
| `--cwd DIR` | The agent's working directory, and the session's `cwd` (made absolute). Default: the current directory |
| `--env KEY=VALUE` | Add a variable to the agent's environment (repeatable) |
| `--unset NAME` | Leave an inherited variable out (repeatable) |
| `--clear-env` | Start the agent with only the `--env` variables |
| `--prompt TEXT`, `--prompt-file FILE` | A prompt; repeatable, sent in order, each turn awaited. `-` reads a file from stdin. With none, the run stops after `session/new` — a "does it start" check |
| `--permission ask\|allow\|reject` | `ask` (the default) has nobody to ask, so it rejects and the summary says why. `allow` selects the first `allow_once` option and never an `allow_always`. `reject` selects the first reject option. When nothing offered fits, the turn is cancelled, which answers the request `cancelled` |
| `--timeout SECS` | A limit on the whole run, spawn to last turn |
| `--export PATH` | Write the trace to `PATH` (replaced if it exists, `0600` on Unix), or into `PATH` under a generated name when it is a directory. Written on every ending, timeouts included |
| `--json` | Print the summary as JSON (`"format": "acp-inspector-cli-run"`, version 1) instead of text |

## Output

The summary names the agent and its version, its capabilities, each turn's `stopReason`, the
agent's message and tool calls, every permission answer and why, the
[conformance annotations](architecture.md#15-open-questions--validation-gaps) — the MUSTs
the trace shows were broken — and the agent's stderr and the transport's remarks. Elicitations
are cancelled, since there is nobody to fill them in. `inspect` reads an export back and counts
its frames by direction and by method, pairing each answer with the request it answers.

## Exit codes

| Exit code | Meaning |
| --- | --- |
| 0 | Every prompt ended with a stop reason and nothing was annotated |
| 1 | The agent or the protocol failed: it did not start, `initialize`, `session/new` or a prompt failed, the export could not be written; or `inspect` was given a file it cannot read |
| 2 | The run completed, and the trace broke a MUST the inspector checks |
| 3 | `--timeout` ran out |
| 64 | The command line was wrong (including an unreadable `--prompt-file`) |
| 130 | Interrupted with Ctrl-C (the agent is still stopped and the trace exported) |

## In CI

A CI smoke test for an agent, keeping the trace as evidence either way:

```yaml
- name: Install acp-inspector-cli
  run: cargo install --locked --git https://github.com/sagikazarmark/acp-inspector acp-inspector-cli
- name: ACP smoke test
  run: |
    acp-inspector-cli run --timeout 120 --permission reject \
      --prompt 'Say hello' --export acp-trace.jsonl --json \
      -- ./target/release/my-agent --acp > acp-summary.json
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: acp-trace
    path: acp-*.json*
```
