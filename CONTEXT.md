# ACP Inspector

Ubiquitous language for a desktop tool that drives one ACP agent and shows what crossed the wire.
Its subject is agent behaviour, so its words are about *claims and traffic* — who said what, and
whether anyone can check it. Terms are the inspector's own: this context borrowed nothing from the
glossary of the repository it began in and lent nothing to it, and it kept its own on the way out
([ADR 0001](docs/adr/0001-the-inspector-keeps-its-own-context.md)).

The decisions these terms are used to state live in [`docs/architecture.md`](docs/architecture.md).
This file holds terms and nothing else.

## Language

### The subject

**Agent**:
The stdio subprocess the inspector spawns and speaks ACP to. The user's, named by command line —
never this tool.
_Avoid_: Server, backend, assistant

**Connection**:
One spawned Agent plus the framed message pipe to it: outgoing frames, incoming frames, and a
diagnostic side-channel. One at a time.
_Avoid_: Client, transport, link

**Frame**:
One JSON-RPC message as it crossed the wire, direction- and timestamp-tagged. The unit of the
Trace.
_Avoid_: Packet, message (bare), event

**Trace**:
Every Frame on every Connection in the order it crossed, captured below the typed layer. It
outlives any one Agent.
_Avoid_: Log, history, wire panel

**Diagnostic channel**:
Everything the transport said about one Connection that was not a Frame, line-timestamped: for
stdio the Agent's own stderr, and beside it the transport's own remarks — that it could not start
the Agent, that the Agent exited, that the pipe broke, that a Frame or a stderr line was too long
to keep. One channel and one order, not two, so the Agent's account and the transport's are read
against each other.
_Avoid_: Stderr log, console (the Console is a panel it appears in, and holds the Trace too)

**Console**:
The tabbed panel holding the Trace and the Diagnostic channel, showing one at a time. What is in it
is what the transport seam produced about one Connection, below the typed layer: the Frames as they
crossed, everything else the transport said, and where it says the record has a hole. Nothing
decoded is in it, which is why the Timeline is not — a Frame's row may name what the *envelope*
says it is, which is JSON-RPC's own vocabulary and not v1's, and never what it means.
_Avoid_: Wire panel, drawer, dock, bottom panel

### The conversation

**Session**:
What the Agent holds a run of Turns in, named by an id the Agent minted. The inspector holds one
**live Session** at a time; opening another is a switch, and having none is a state the screens say
out loud.
_Avoid_: Thread, chat, conversation

**Session setup response**:
The answer to whichever call opened the live Session — `session/new`, `session/load` or
`session/resume`. Where a Session's own data rides, as opposed to what the Connection's
`initialize` said. Not a protocol term; the protocol names the three responses separately and has
no word for what they have in common.
_Avoid_: Session response, new-session response

**Turn**:
One `session/prompt` and everything until its outcome. What ends a Turn is read from the update
stream, not from the call returning.
_Avoid_: Exchange, round, request

**Timeline**:
The centre screen: the composer, the rendered `session/update` stream for the live Session, the
inline Blocking Requests, and the Conformance Annotations among them. It is the live Session's
stream *and* whatever the Connection is waiting on an answer to — which is why it has something to
draw when there is no live Session and something still wants answering.
_Avoid_: Transcript (the host context's word for an ordered message list; the Timeline carries more
than messages), feed, chat log

**Unrecognized entry**:
A Frame or `session/update` variant the typed layer cannot decode, rendered first-class from its raw
JSON. Not an error and never dropped.
_Avoid_: Unknown event, unsupported message, parse failure

**Conformance annotation**:
The inspector's own statement that a specification **MUST** was broken, made only where captured
Frames decide it, and rendered beside the traffic it is about. It is a report of a rule, never a
grade, a score or a verdict.
_Avoid_: Violation, warning, error, lint, check

### What is asked of the reader

**Blocking request**:
A call from the Agent that stops until somebody answers it — asking permission for a tool call, or
an Elicitation — as opposed to the traffic that only reports. What makes it one is that the Timeline
is where it is *answered* and not only where it is shown, so a Connection with no live Session still
has somewhere to put one. Every answer is a claim about what the reader did, which is why this tool
makes one only where it drew something for them to do.
_Avoid_: Prompt, dialog, callback, interrupt

**Elicitation**:
The Agent asking for input the Session's traffic cannot carry: a form, built from a schema the Agent
sent, or a URL it wants opened somewhere this tool cannot see into. Answered *accept*, *decline* or
*cancel*. Tied either to a Session — optionally to one tool call in it — or to a single request
outside any Session, and the two are not variants of one shape but two different things it can be
about. Distinct from the host context's term of the same name, which knows only the form.
_Avoid_: Form request, input request, prompt

### What is claimed

**Advertisement**:
Whatever an Agent or this client said about itself that an Affordance is gated on. It is not one
mechanism: a Capability is an Advertisement, and so is the presence of a Session's settings data on
a Session setup response — which is why the word is deliberately wider than *capability*.
_Avoid_: Support, feature flag

**Capability**:
Never used unqualified in this context, because three different claims wear the word: what the Agent
says about itself, what this client says about itself, and — wrongly — what a Session carries on its
setup response. The first two are **Agent Capability** and **Client Capability**. The third is not a
capability at all and is named as what it is, an Advertisement; **Session Capability** is the term
that would otherwise be mistaken for it.

**Agent Capability**:
What the Agent claimed about *itself* in its `initialize` result — per-Connection, and true of every
Session on it.
_Avoid_: Capability (bare), agent feature

**Client Capability**:
What the inspector claimed about *itself* in its `initialize` request — the services it will perform
and the data shapes it will render. What it does not claim, a conformant Agent must not send it.
_Avoid_: Capability (bare), our capabilities

**Session Capability**:
An Agent Capability claimed by the presence of an object under `sessionCapabilities` — named for the
Session methods it is about, not for a Session that carries it. No Session ever carries one. What a
Session *does* carry, on its setup response, is its Session Settings, and those are an Advertisement
rather than a Capability.
_Avoid_: Capability (bare), per-session capability

**Affordance**:
A control the inspector offers for driving the Agent. Gated on the Advertisement and on nothing else
— never on what the inspector believes the Agent's state makes sensible.
_Avoid_: Feature, action, button

### Session Settings

**Session Settings**:
The Modes and the Config Options of the live Session, together: what the Session is configured as
and what it could be configured as. The protocol has no word for the pair — this one is the
inspector's.
_Avoid_: Settings (bare), preferences, configuration, session config

**Mode**:
A named way the Agent operates in a Session, one of a set the Agent published with the one currently
active named among them. Changed with `session/set_mode`. *Mode* is also a Config Option **category**
and a Config Option id Agents really use — a Mode and a Config Option that happens to be about modes
are two different things, and an Agent publishing both has published two.
_Avoid_: Profile, persona, state

**Config Option**:
A named selector the Agent published for a Session — its id, its label, its kind, its current value,
and the values on offer. Changed with `session/set_config_option`. Distinct from the host context's
term of the same name, which is that domain model's word for the same protocol field.
_Avoid_: Setting, preference, parameter

### Working vocabulary

**Ring**:
One round of work around the whole tool, sized to a single implementation effort and specified
before it starts. A scope word, not a release: nothing here is versioned or shipped by it.
_Avoid_: Milestone, phase, sprint, release
