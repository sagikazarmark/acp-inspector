# The inspector keeps a context of its own, and the repository gains a map to say so

The inspector's vocabulary moves out of its architecture spec into a [`CONTEXT.md`](../../CONTEXT.md)
of its own, beside the code it describes; its decisions get this directory; and a
[`CONTEXT-MAP.md`](../../../../CONTEXT-MAP.md) at the repository root names the two contexts and
points at each glossary. The host repository's own `CONTEXT.md` is **unchanged and gains nothing** —
it is the chat library's glossary and stays exactly that.

The trigger was a word that had started meaning three things at once. The session settings ring
([#84](https://github.com/sagikazarmark/dioxus-chat.orig/issues/84)) makes *Capability* the Agent's
claim about itself, this client's claim about itself, and — by the temptation the ring had to name
and refuse — what a Session carries on its setup response. *Mode* became both a first-class protocol
concept and a Config Option category in the same paragraph. Neither collision was noticed while the
terms were prose scattered through a specification, which is the argument for a glossary as such: a
document that defines terms while using them cannot see a word doing two jobs.

**Why not the host's glossary, which already exists and already defines some of these words.**
Because it defines them *differently, and correctly, for a different model*. Both contexts have a
Session, a Turn and a Config Option; the library models a conversation for an application to render,
the inspector models an agent's claims for a user to check, and the same word lands in a different
place in each. Merging them would force one model to bend — and everything under `phoenix/` is
destined to split out to its own repository
([architecture §3](../architecture.md#3-where-it-lives)), so a shared glossary would have to be torn
apart on the day of the split, along whatever seam had been blurred in the meantime. The inspector
has kept its vocabulary separate since its first specification; this only writes down where it lives.

**This is the first time the inspector's documentation touches the repository root**, which is the
part a later reader will want explained. Nine ADRs sit in the root `docs/adr/` and every one of them
is about the chat library; this one is deliberately not among them, because a decision about the
inspector's own documentation is not a decision the library made. What went to the root is one file
that belongs to neither context — a map. Contexts that do not know about each other still need
something that knows about both, or a reader arriving at the root has no way to tell which glossary
governs the code in front of them.

## Considered Options

- **Fold the inspector's terms into the root `CONTEXT.md`.** Rejected: it is the chat library's
  ubiquitous language, the two models genuinely disagree about shared words, and the inspector is
  leaving. The specification said so on the day it was written — *"its vocabulary lives here, not in
  the host repo's `CONTEXT.md`"* — and this decision keeps that promise rather than reversing it.
- **Leave the terms in the architecture specification.** The status quo, and cheapest. Rejected: it
  is what let *Capability* mean three things without anyone noticing, and a specification is read for
  its decisions — a reader looking up a word has to find the paragraph that happens to define it.
- **A glossary but no map.** Rejected: two `CONTEXT.md` files with nothing naming them is worse than
  one, because the root file now silently lies about its own scope. The map costs one file and is
  what makes the second glossary discoverable at all.
- **Move the host's glossary into a directory of its own, so the root holds only the map.** The
  tidier shape, and what the map convention suggests. Rejected as churn for its own sake: the root
  `CONTEXT.md` is referenced from crate documentation, specs and prior tickets, and rewriting all of
  it to make a diagram symmetrical changes nothing about which glossary governs what.

## Consequences

- The repository's agent documentation stops calling this a single-context repository: `AGENTS.md`
  and `docs/agents/domain.md` name both contexts and point at the map.
- The architecture specification's terminology section becomes a pointer. Its decisions stay where
  they are — the glossary holds terms only, and is not somewhere to record what was decided.
- The inspector's ADRs number from 0001 in this directory, independently of the root's. Two ADR
  0001s now exist in the repository and they are about different things; the path is what
  distinguishes them.
- The map is the file the split will delete. When `phoenix/` leaves, the inspector's `CONTEXT.md`
  and this directory travel with it unchanged, the root returns to a single context, and the map has
  no second context left to name — which is the outcome it was written for, not a defect in it.
