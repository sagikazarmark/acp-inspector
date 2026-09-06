# The stable method list is the gate, not the feature name

## Context

This tool depends on one protocol crate, `agent-client-protocol-schema`, with default features only.
Both manifests say why in the same words: *every `unstable_*` one is traffic this layer deliberately
does not recognize, and unrecognized traffic is displayed rather than decoded*
([§8](../architecture.md#8-unknown-traffic-the-raw-first-rule)). The rule has been applied three
times and never argued with — `plan_update` and `plan_removed` are not among the eleven decoded
`session/update` variants ([§7.2](../architecture.md#72-serviced-agent--client)), `session/fork` is
not a typed method ([§7.5](../architecture.md#75-the-second-ring-the-session-lifecycle)), and the
capability panel says out loud that `fork`, `providers` and `nes` are claims this client does not
decode ([§15](../architecture.md#15-open-questions--validation-gaps) q11).

The fifth ring ([§7.8](../architecture.md#78-the-fifth-ring-elicitation)) needs
`elicitation/create` and `elicitation/complete` decoded, and the schema crate hides both behind
`unstable_elicitation` and doc-labels every type **UNSTABLE**. Read as written, the manifest comment
forbids the ring.

Read as *argued*, it does not, and the difference is where the rule's authority comes from:

- **The coverage yardstick is the canonical schema's method list**, not the crate's feature list —
  [§7.4](../architecture.md#74-the-coverage-yardstick) says so, and it predates every use of the
  rule.
- **§7.5 sourced its `session/fork` decision from that list**, in those terms: absent from
  `schema/v1/meta.json`, present only in `schema/v1/meta.unstable.json`, therefore not a typed
  method. The cargo feature was the consequence, never the premise.
- **`elicitation/create` and `elicitation/complete` are in `schema/v1/meta.json`** — two of the
  eleven client methods it defines — and `session/fork` is not. The protocol's own canonical
  statement of what stable v1 contains includes them.
- **The crate is behind its own schema**, not ahead of it: `agent-client-protocol-schema` 1.6.0 is
  the current release and the rust-sdk's `main` still pins `=1.6.0`, with the elicitation types
  compiled out by default and documented against a specification the published protocol
  documentation now carries as v1.

So the manifest comment states a rule by naming a mechanism that used to coincide with it. The two
have come apart for one method family, and the choice is which of them this tool was actually
following.

Two alternatives were considered and rejected. **Hand-rolling the types in core** keeps the manifest
sentence literally true at the cost of roughly seven hundred lines of serde that must track a schema
this tool has a dependency on precisely so it does not have to — and it would decode elicitation *as
this repository understands it* rather than as v1, which is the thing [§11](../architecture.md#11-v2-seams-kept-open)
seam 1 exists to prevent. **Waiting for the crate to stabilize the feature** is waiting on somebody
else's packaging for a decision the canonical schema has already made, with every turn that dies at
`-32602` as the cost of the wait.

## Decision

- **The gate on what this tool decodes is the canonical schema's stable method list** — the methods
  `schema/v1/meta.json` defines — and never the name of a cargo feature. Where the two agree, which
  is everywhere else today, nothing changes.
- **`unstable_elicitation` is enabled for that one family and no other.** No `unstable_*` feature is
  turned on for convenience, for a type that happens to live behind it, or because another was.
- **The manifest comments are corrected to state the rule rather than the mechanism.** A comment that
  has to be argued past is a comment that will be argued past again by somebody with a worse reason.
- **Anything the stable list does not define stays raw-first.** `plan_update`, `plan_removed`,
  `session/fork`, `fork`, `providers`, `nes` and every unstable capability keep exactly the treatment
  §7.2, §7.5, §8 and §15 q11 give them today. This decision moves one family across a line; it does
  not move the line.
- **The crate's `UNSTABLE` doc labels are not evidence about the protocol.** They are evidence about
  the crate's release policy, which is a different question from what v1 contains, and this tool
  reads the schema for the first.

## Consequences

- **The workspace and core manifests enable one feature and say why**, in the house style, pointing
  here. `crates/core/tests/headless.rs`'s hold on the dependency graph is unchanged: the full
  `agent-client-protocol` SDK is still not in it, because the objection to the SDK was never about
  stability.
- **A schema-crate upgrade can break the build in a new way.** An unstable feature may change shape
  between releases without ceremony, and the pin (`1.6`) plus a compile failure is the whole of the
  protection. That is accepted: the alternative was maintaining the same types by hand, where the
  same change arrives as silently wrong decoding rather than as a build error.
- **The day the crate stabilizes elicitation, the only change is deleting a feature line.** Nothing
  in core, the window or the tests names the feature.
- **This ADR is the answer the next `unstable_*` request is measured against**, and the test it has
  to pass is narrow: is the method in `schema/v1/meta.json`? If it is not, §8 already decided the
  question and this document does not reopen it.
