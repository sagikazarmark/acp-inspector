# The trace export format — `acp-inspector-trace` v1

**Status:** pinned. This document is the contract
[§10](architecture.md#10-the-jsonl-trace-export) said would be pinned when the export landed,
and it answers [§15](architecture.md#15-open-questions--validation-gaps) questions 2 (the record
schema, and whether it aligns with the bridge's) and 3 (what "clear" means for an
export-anchored log, and how the log is bounded).

**It is a contract because something else reads it.** The export is the seed of the event log
ACP Wiretap and replay tooling are waiting on
([#50](https://github.com/sagikazarmark/dioxus-chat.orig/issues/50)). The moment one of them
consumes a file, the shape below stops being `acp-inspector-core`'s to change quietly — so it is
written down, versioned, and self-describing rather than grown.

Implemented in `crates/core/src/export.rs`; asserted, field by field, in `crates/core/tests/export.rs`.

## The file

[JSONL](https://jsonlines.org): UTF-8, one JSON document per line, `\n`-terminated including the
last line. The first line is the header; every line after it is one frame, in the order the
frames crossed the wire.

```jsonl
{"type":"header","format":"acp-inspector-trace","version":1,"at":1785312000123,"frames":2,"dropped":0,"warning":"ACP frames may contain prompts, file contents, and tool output; nothing is redacted"}
{"type":"frame","at":1785312000456,"connection":1,"direction":"client -> agent","encoding":"utf8","frame":"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1,\"clientCapabilities\":{}}}"}
{"type":"frame","at":1785312000512,"connection":1,"direction":"agent -> client","encoding":"utf8","frame":"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{}}}"}
```

A frame is carried as a **JSON string, verbatim** — never re-encoded as JSON, never
pretty-printed, never repaired. That is the raw-first rule
([§8](architecture.md#8-unknown-traffic-the-raw-first-rule)) reaching the file: a frame that was
not valid JSON at all is still a record, because it is exactly the frame the bug report is
about. Consumers that want the message parse the string themselves, and are the ones who get to
decide what to do when it will not parse.

### The header

| Field | Type | What it says |
|---|---|---|
| `type` | `"header"` | Which record this is. Always the first line, and only the first line. |
| `format` | `"acp-inspector-trace"` | What the file is, so a reader can refuse a file it does not know. |
| `version` | number | This schema's version — `1`. |
| `at` | number | When the export was taken (see [Timestamps](#timestamps)). |
| `frames` | number | How many frame records follow. |
| `dropped` | number | How many frames the trace captured that this export does **not** carry — cleared, or aged out of the cap. `0` means the export is the whole of what the trace saw. |
| `warning` | string | What is in the file. Nothing is redacted, and the file says so before anything else. |

### A frame

| Field | Type | What it says |
|---|---|---|
| `type` | `"frame"` | Which record this is. |
| `at` | number | When the frame crossed the trace decorator (see [Timestamps](#timestamps)). |
| `connection` | number | Which connection it crossed, counting from 1. |
| `direction` | `"client -> agent"` \| `"agent -> client"` | Which way it went. The inspector is always the client; it never observes a third party's traffic ([§13](architecture.md#13-relationship-to-the-host-repos-assets) — that is Wiretap's charter). |
| `encoding` | `"utf8"` | How `frame` carries the bytes. Always `utf8` from this producer. |
| `frame` | string | The frame, verbatim. |

**`connection` is why a trace of two agents is still readable.** One inspector window records
into one trace across successive connections (`Trace::tap` exists for exactly that), and every
agent's JSON-RPC ids start again at 1. Without this field a consumer correlating calls to
answers would fold two id spaces into one and reconstruct a conversation nobody had.

**`encoding` is always `utf8` here**, because a `Frame` is text by construction — the inspector
has nothing to base64. It is written anyway so that a reader built for the bridge's records,
which can carry either, reads these without a special case.

### Timestamps

`at` is **milliseconds since the Unix epoch**, as a JSON number, and may be negative on a
machine whose clock is set before 1970.

A number rather than a formatted date, because this file is read by tools: no timezone to agree
on, no parser to write, and the ordering is the comparison. Rendering it for a human is
presentation, and belongs where the reader's own clock is known — which is the same split core
and the desktop crate already have ([§5](architecture.md#5-the-coreui-split)).

Frame order in the file is the trace's order, and the trace's order is the wire's: the recording
and the handoff to the transport happen under one lock, so two senders cannot record in one
order and send in the other. Where two frames share a millisecond, **the file's order is
authoritative, not the timestamps**.

### Versioning

`version` changes when a consumer written for the old number would read a new file *wrongly* —
a field removed, a field's meaning or type changed, the record layout changed. It does **not**
change when a field is added beside the ones above.

So: **consumers must ignore fields they do not know**, and must not assume the key order in
which a record's fields happen to be written.

## Alignment with the bridge's `--trace-frames`

The prior art is `app/bridge/src/shell/trace.rs`, whose records are
`{connection, direction, encoding, frame}` behind a self-describing header. **The answer to
§15 q2 is: aligned, deliberately, and extended rather than reshaped.**

- Every field the two formats share means the same thing and is spelled the same way, **down to
  the direction strings** (`"client -> agent"`, `"agent -> client"`). The inspector *is* the
  client, so its two directions and the bridge's are the same two facts about the same two ends;
  spelling them differently would have bought a translation table and nothing else.
- The inspector adds `type` on every record (the bridge tags only its header) and `at` on every
  frame (the bridge records no time). A reader that dispatches on `type == "header"` reads both.
- The inspector's `connection` is an ordinal, because its connections are sequential; the
  bridge's is a string, because it multiplexes concurrent WebSocket clients. Same field, same
  role: *which pipe this crossed*.
- The header's `warning` is the bridge's, word for word. The two files carry the same risk, so
  they say the same thing about it.

What is deliberately **not** in the export: the diagnostic channel (the agent's stderr and the
transport's remarks) and anything the typed layer made of a frame. The export is the wire and
only the wire — a decoding belongs to whoever reads the file, and stderr is not a frame.

## Where an export goes

`Export::save` writes into the system temp directory as `acp-trace-<epoch-millis>.jsonl`;
`Export::save_in` takes a directory instead. Nothing is ever written over: a name already taken
— two exports inside one millisecond, which is one impatient click — gets the next name along,
not the earlier evidence gone. On Unix the file is created `0600`, because nothing in it is
redacted and a world-readable copy in a shared temp directory is not what anybody asked for by
pressing Export.

The shell shows the full path with **Copy export path** and, on desktop, **Open containing
folder**. Both report pending state and failure beside the path; opening reports successful
dispatch to the desktop, not proof that a folder window appeared. Copy or move the file to
durable storage to keep it: the system may remove temporary files. Retrieval does not rewrite
the export or change the snapshot/no-overwrite contract.

## Clear

**Clear drops every frame the trace holds, and nothing else.** It does not disconnect anything,
does not stop the recording, and does not wait for a turn to end. Nothing else clears the trace:
in particular the log is *not* cleared on disconnect, which is where this differs from the MCP
Inspector — an agent that just died is when its frames are worth the most.

Its relationship to the export — §15 q3's "what does clear mean for an export-anchored log?" —
is defined in both directions:

- **An export already taken is unaffected.** An export is a snapshot: the frames are copied out
  from under the trace's lock when the button is pressed, and written as they were. Clearing
  afterwards cannot reach a file, and cannot reach an `Export` value either.
- **An export taken afterwards contains only what followed**, and says so: the frames cleared
  are counted into the header's `dropped`, permanently. An export can therefore always be told
  apart from a whole session — which is the point of clearing before reproducing something, and
  the reason `dropped` never resets.

## Bounding

**A ring of `Trace::CAPACITY` (10 000) frames and `Trace::BYTE_CAPACITY` (64 MiB raw UTF-8)**,
oldest whole Frames dropped until both budgets fit, counted in `dropped`. §15 q3
asked for cap, ring, or unbounded; the argument:

- **A cap that stops recording is the worst of the three for an inspector.** The frames being
  looked for are the *last* thing the agent did, and a log that stops throws away exactly those.
- **Unbounded is a promise a long-running window cannot keep.** Frames are cheap but not free,
  and a chatty agent left running overnight has no natural end.
- **A ring keeps the newest and admits what it lost.** The admission is the part that makes it
  honest: `dropped` is in every export header and beside the frame count in the trace view, so a
  trace that is not the whole session never looks like one.

10 000 rather than the MCP Inspector's 1000, because a single ACP turn with streamed chunks and
tool calls can run to hundreds of frames and a debugging session is several turns; and because
the number is a policy, not part of the contract above — it can change without a `version`
bump.

The diagnostic channel is separate: 10,000 entries / 8 MiB, with its own visible missing-entry
count, and is not part of this export. Timeline retention is likewise independent; see
[architecture §10](architecture.md#10-the-jsonl-trace-export).

All desktop export entry points take one activation snapshot and write it in the background,
with pending, success and error states. A repeated activation while pending is ignored; Clear
or a new Connection cannot alter the snapshot. This changes neither the JSONL v1 schema nor
the current temporary-directory destination.
