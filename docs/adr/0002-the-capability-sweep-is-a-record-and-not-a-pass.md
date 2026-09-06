# The capability sweep is a record of what was driven, not a pass that drives

[§1.1](../architecture.md#11-what-is-deliberately-not-built) has promised since the MVP that *the
capability sweep* — **a tool-driven pass that exercises every advertised method and leaves a
filled-in report** — was wanted, and was the ring after the current one. The fourth ring
([#96](https://github.com/sagikazarmark/dioxus-chat.orig/issues/96)) does not build it. What it
builds instead is **passive**: a per-connection record of which of the agent's advertisements were
driven and what came back, filled in by the user driving the affordances that already exist, drawn
as a fourth fact on the capability panel's rows. The pass itself is not deferred again — it is
**declined**.

**The reason is a rule three rings have held, and the pass breaks it.** The inspector sends what a
user asked it to send. §7.5 makes that explicit where it is hardest — every session lifecycle
method becomes an affordance *"each gated on the agent's own advertisement and on nothing else —
including where the inspector believes the operation cannot succeed"* — and the one call this tool
makes on its own initiative, the listing refreshed after a close or a delete, is argued for in its
own paragraph precisely because it is the exception (`crates/core/src/client.rs`, `refresh_listing`). A
pass that drives every advertised method inverts that: it would prompt, set modes, close sessions
and delete them against a live agent because a button said *sweep*, and the traffic the trace holds
would be the tool's own invention rather than the user's ask. For a tool whose subject is what an
agent does, manufacturing the stimulus is the one thing that makes the record harder to read.

**The second reason is what a filled-in report becomes once it leaves the window.** §15 q7 settled
validation-versus-observation with an admission rule and a list of refusals: *"no rule engine, no
severity grading, no aggregate verdict, no pass/fail score and no configuration."* A document
enumerating every advertised method with an outcome beside it is read as a scorecard by the second
person who sees it, whatever the header says — and unlike an annotation, it would outlive the frames
that decide it. The record built instead is screen-only for that reason, which is also what the
trace export contract already requires of it: *"the export is the wire and only the wire"*
([`trace-export.md`](../trace-export.md)).

**What was learned by waiting is that the row's own reason had expired.** §1.1 deferred the sweep
because *"it has nothing to drive until the methods are reachable"* — a timing argument. Rings two
and three made them reachable, and the answer turned out not to be timing at all: the pass was
always the wrong shape, and could have been declined on the day the row was written. The row
therefore splits rather than clearing. The report half is consumed by this ring; the pass half stays
in the table with this argument in place of the old one.

## Considered Options

- **Build the pass as specified.** Rejected above. Worth recording that it is not merely unbuilt: a
  later reader who finds §1.1 promising an autopilot and this record instead should find the refusal
  here rather than infer that the ring ran out of room.
- **A guided checklist** — a list of every advertised method, each row a button the user presses,
  filling in as they go. The honest middle, and it keeps the user as the one who sends. Rejected as
  a second place to press the same buttons: every one of those affordances already exists on the
  panel or the listing row, and a checklist duplicating them would have to answer what happens when
  the two disagree.
- **Keep it deferred rather than declining it.** Rejected because it is not true. Deferring says the
  shape is right and the moment is wrong; the shape is what this ring found fault with.
- **An exportable report beside the JSONL trace.** Rejected: `trace-export.md` already fixes the
  export as the wire and nothing the typed layer made of it, and an artifact that outlives its
  frames is the verdict q7 refused.

## Consequences

- **The record can only be as complete as the affordances are.** Five of the twelve advertisements
  the panel draws — the prompt content kinds and the MCP server transports — have no affordance at
  all, and two more can be advertised without any affordance reaching them
  ([§7.5](../architecture.md#75-the-second-ring-the-session-lifecycle): resume where load is
  preferred, close and delete where the listing is not advertised). The fourth fact is drawn only
  where an affordance can reach, because *not driven* on a row this tool cannot drive is a false
  sentence about the agent. Making those reachable is ring work, not sweep work.
- **No new conformance rule comes with this.** An agent that advertises a capability and refuses it
  is a finding the user reads off the row; it is not annotated, because no MUST in ACP v1 obliges an
  agent to implement what it advertised ([§15](../architecture.md#15-open-questions--validation-gaps)
  q7 records the check).
- **The word *sweep* survives only in the declined row.** The record introduces no glossary term:
  with no surface of its own and no second party to audit, it names a property an Advertisement
  carries rather than a thing a screen is about — and *exercisable* is already spoken for in §7.5,
  where it means a button exists rather than that one was pressed.
