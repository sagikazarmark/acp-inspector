# A turn is a record and not only a position

## Context

`CONTEXT.md` has always defined a **Turn** as one `session/prompt` and everything until its
outcome. The stores did not keep one. [`TurnState`](../../core/src/turn.rs) is a single value
saying where the *live* turn stands, replaced the moment the next prompt goes out, and
`TimelineEntry` carried no turn at all — so the centre screen was a flat, arrival-ordered list, and
the only turn information anywhere on the window was one status about the most recent one, in the
composer's strip.

Two things followed from that, and the second is the reason this was worth building.

**A session read as rows rather than as exchanges.** Six prompts produced forty rows with nothing
saying where the third began or how it finished. Each turn's outcome existed for as long as it was
the live one and was then overwritten; the frames were still in the Trace, but reading a session's
shape meant correlating them by hand.

**And an agent that talked outside a turn looked exactly like one that did not.** An agent that goes
on streaming into a turn it has already resolved, or starts talking before anything was prompted,
produces rows indistinguishable from the rows above them — on the one screen whose job is to show
what an agent actually did.

## Decision

- **Every timeline entry carries the turn it arrived in**, or `None` where it arrived outside one.
  Stamped when the entry is made and never restamped: a tool call that began in one turn and
  streamed updates into the next belongs to the turn that started it.
- **`None` is a fact about the traffic and not a gap in the record.** Traffic before the first
  prompt, traffic after the agent resolved the turn, and a session replayed by `session/load` all
  belong to no turn, and none of them is tidied into whichever turn it fell between.
- **The timeline keeps a record of each turn** — its ordinal from one, when the prompt went out, and
  what it came to. `TurnOutcome` is `Ended(StopReason)` or `Failed(CallError)` and has no spelling
  for `Idle` or `InFlight`, because those are not outcomes.
- **The record and the position never say the same thing about the same turn.** A turn with an
  outcome is not the live one any more and the live one has no outcome yet, which is why
  `TurnOutcome` is a second type rather than a second copy of `TurnState`. The outcome is read *from*
  the settled state, so the two cannot disagree about how a turn finished.
- **The span opens where the prompt goes out and closes after the conformance annotation is made.**
  A finding about a turn belongs inside the turn it is about, not in whatever arrives next.
- **The turns are the session's**, and a switch discards them with the entries. A new connection
  ends whatever was open and keeps the record, for the same reason the entries survive one.
- **The screen draws a quiet line under a turn that is over**, carrying the agent's own
  `stopReason` in the wording the composer already uses for the live turn — one wording, shared, so
  the record and the position cannot describe the same stop two ways. A turn that ended having said
  nothing still gets its line: a `refusal` that produced no updates is a turn that happened, and
  drawing nothing for it would be a screen where the prompt vanished.
- **A run of out-of-turn traffic is marked where the run begins**, in words. A turn's line goes
  *under* it because how a turn ended is known at its end; an out-of-turn run's marker goes *above*
  it because what those rows are is known before reading them, and a run that is still arriving has
  no end to hang a label on. One marker per run and not one per row.
- **It is not a severity and not a grade** ([§9](../architecture.md#9-screens)). A turn that ended
  and a run outside one are the same weight, because a replayed session is entirely out of turn and
  entirely correct. The out-of-turn marker takes the colour unrecognized traffic takes, and the
  words are the answer either way.
- **Grouping is read from the stamp and never inferred from the rows.** Working out turn boundaries
  from arrival order in the window would be protocol inference in a layer that is strictly
  presentational ([§5](../architecture.md#5-the-core-window-seam)), and it would be wrong on exactly
  the traffic this exists to show.

## Consequences

- **`TimelineEntry` gained a public field and core gained two public types** (`Turn`,
  `TurnOutcome`). Every construction site of an entry stamps it, which is four call sites in the
  store and no decision at any of them: the open turn is store state.
- **The window reads the record from the same snapshot as the entries**, announced by the same
  revision. An entry stamped with a turn and the turn it names have to reach a reader together, or
  the screen draws a group whose outcome it has not been told about yet.
- **The centre screen renders rows rather than blocks.** A row is a block, a turn's ending, or the
  start of an out-of-turn run; the boundaries are list items among the list items, which is what
  keeps them in the scroll, in the keyed list, and in the same order as the traffic.
- **Five headless tests drive it through the public API** (`crates/core/tests/turns.rs`), Testy for the
  conformant cases and scripted `/bin/sh` agents for the two an agent that behaves cannot produce:
  talking before any prompt, and talking after resolving the turn.
- **What this does not do** is judge any of it. Out-of-turn traffic draws no annotation and no
  warning: the specification does not forbid it, several legitimate updates arrive that way, and
  what an inspector owes the reader here is the fact rather than a verdict on it.
