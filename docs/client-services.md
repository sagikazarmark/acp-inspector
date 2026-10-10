# Client services

An ACP agent can ask its client to do things: read or write a text file (`fs/read_text_file`,
`fs/write_text_file`) and run commands in a terminal (`terminal/create`, `terminal/output`,
`terminal/wait_for_exit`, `terminal/kill`, `terminal/release`). A conformant agent asks only for
what the client claimed in `initialize`.

The inspector never performs these services. It does not read or write your files and does not run
commands for the agent. What it can do is **claim** them, so you can watch an agent that relies on
them, and then **answer** each call by hand or from a rules file. Real mediation (reads and writes
scoped to the session's roots, captured terminals) is not built; see F16 in
[`release-plan.md`](release-plan.md).

## Choosing what to claim

The launch dialog has a collapsed **Client services** section:

- **Read text files**, **Write text files**, **Terminal**: each one claims the matching
  capability (`fs.readTextFile`, `fs.writeTextFile`, `terminal`) in `initialize`. All are off by
  default, which is the claim the inspector has always made.
- **Answer other unhandled agent requests by hand**: any other request the inspector does not
  service itself (an extension method such as `_vendor/ping`, or an `fs/*` or `terminal/*` call
  you did not claim) becomes a card you answer, instead of being answered method-not-found. This
  claims nothing.
- **Rules file**: a JSON file of canned answers, described below.
- **Protocol version**: the `protocolVersion` sent in `initialize`. The default is 1, the only
  version the inspector decodes. The capability view shows the version the agent answered with
  and flags a difference.

The choices apply to the next launch and are kept across disconnect and restart while the window
is open. They are not saved between runs of the app. The client capabilities tab shows what the
live connection actually claimed.

## Answering by hand

A claimed call, or an opted-in unhandled request, appears in the timeline as a card that blocks
like a permission request: the agent is waiting on you. The card shows the method, the request id
and the params, and a response editor:

- **Result**: a JSON result, prefilled with a valid skeleton for the method (for example
  `{"content": ""}` for `fs/read_text_file`, `{"terminalId": "term-1"}` for `terminal/create`,
  `{"exitCode": 0}` for `terminal/wait_for_exit`, and `{}` for anything else).
- **Error**: a JSON-RPC error code and message.

**Send** answers the request once. A card that nobody answers behaves like the other blocking
requests:

- cancelling the turn, or opening another session, answers it with the cancellation error
  (`-32800`) and says so;
- the agent can withdraw it with `$/cancel_request`, and the card says the agent withdrew it;
- the turn ending or the agent disconnecting leaves it unanswerable, and the card says so;
- at most 100 blocking requests of all kinds wait at once; past that a request is refused with an
  error and the refusal is recorded.

## Rules files

A rules file answers matching requests before anything is shown to you. The first rule that
matches answers; the timeline card says which rule it was (rules count from 1). The trace holds
the request and the answer as they crossed, like any other frames.

```json
{
  "description": "Canned answers for a demo workspace",
  "unmatched": "manual",
  "rules": [
    {
      "method": "fs/read_text_file",
      "path": "/work/README.md",
      "result": { "content": "# Demo\n" }
    },
    {
      "method": "fs/read_text_file",
      "path": "/work/*.rs",
      "result": { "content": "fn main() {}\n" }
    },
    {
      "method": "fs/read_text_file",
      "error": { "code": -32002, "message": "Resource not found" }
    },
    {
      "method": "fs/write_text_file",
      "result": {}
    },
    {
      "method": "terminal/create",
      "command": "cargo test",
      "result": { "terminalId": "tests" }
    },
    {
      "method": "terminal/*",
      "description": "Every other terminal call succeeds with an empty answer",
      "result": {}
    }
  ]
}
```

The top level is an object with:

| Key | Required | Meaning |
|---|---|---|
| `rules` | yes | The rules, tried in order. |
| `unmatched` | no | What happens to a request no rule matches: `"manual"` (the default) does what the launch settings would do without a rules file — a card to answer where the service was claimed or you opted in to answering unhandled requests, and method-not-found otherwise. `"method-not-found"` answers every unmatched request method-not-found, claimed or not. |
| `description` | no | Ignored; for your own notes. |

Each rule is an object with:

| Key | Required | Meaning |
|---|---|---|
| `method` | yes | The method, exactly or as a pattern (`terminal/*`). |
| `path` | no | Matches the request's `path` param, exactly or as a pattern (`/work/*.rs`). A request without a string `path` does not match. |
| `command` | no | Matches when the command line `terminal/create` asks for — `command` followed by `args` — starts with these words. `"cargo test"` matches `cargo test --all` but not `cargo build` or `cargonaut test`. |
| `result` | one of these two | The JSON-RPC result, sent exactly as written. |
| `error` | one of these two | `{"code": <integer>, "message": <string>, "data": <any, optional>}`. |
| `description` | no | Ignored; for your own notes. |

**Patterns** use `*` as the only wildcard. It stands for any run of characters, `/` included, so
`/work/*.rs` matches `/work/main.rs` and `/work/src/deep/lib.rs`. Every other character, `?`
included, stands for itself; a pattern without `*` is an exact match.

Rules apply to any request the inspector does not service itself, claimed or not, so a rules file
can also answer an extension method. A rule never fires for `session/request_permission` or
`elicitation/create`, which the inspector services itself.

The file is read when you choose it; choose it again after editing it. A file that is not valid
JSON, has unknown keys, or has a rule with both or neither of `result` and `error` is refused with
the rule's number, so a typo cannot silently turn into a rule that never matches.

## Permission policy

Separately from client services, the timeline header has a **permission policy** for the open
session: **Ask me** (the default), **Allow once automatically** or **Reject once automatically**.
It applies only to `session/request_permission` requests that arrive after it is set. An automatic
answer picks the first `allow_once` or `reject_once` option the agent offered and never an
`allow_always` or `reject_always` one; a request that offers no such option waits for you. A card
answered by the policy says so. The policy goes back to **Ask me** when the session changes or the
agent disconnects.
