# Schemaform draws the Elicitation form

## Context

The [spike](../schemaform-spike.md) established desktop execution and advisory
answers, then stopped on numeric feature unification and field-finding placement.
[ADR 0012](0012-the-form-library-can-run-here-but-does-not-replace-the-form-yet.md)
recorded that stop. [#8](https://github.com/sagikazarmark/acp-inspector/issues/8)
now contains both numeric features at the typed Frame boundary, and schemaform
0.5.0 includes the upstream [#7 fixes](https://github.com/sagikazarmark/schemaform/pull/54).
The registry follows that release at `025d2228e9253032ac6b8deee4890c46e35379b5`.

## Decision

Use schemaform and schemaform-dioxus 0.5.0 with the registry's schemaform_daisyui
renderer for Elicitation forms. The reason is shared editing machinery and
field-local, navigable findings, not an additional Agent Capability. Accept is
advisory: it sends the engine's data even when constraints are broken. Raw remains
an independent object-or-nothing answer surface for type violations and forms the
engine cannot prepare. Switching tabs preserves both drafts; the visible one sends.

The host owns the protocol adaptation: literal requestedSchema extraction,
quarantining property types outside ACP's five-type profile, removing quarantined
required entries, multi-select uniqueness, Draft 2020-12 selection, default seeding
and answer mapping. It no longer closes the root to silence findings: 0.5.0 makes
the ordinary open-object finding informational. The Frame and typed numeric copy
remain separate. Unrenderable properties show their schema and point to Raw.

Use `Appearance::None`, retained host layout rules and daisyUI's ordinary component
classes. Add only `link` to ADR 0005's include list; card/join/radio are not needed
by the flat form. ADR 0003's explicit sources, committed generated stylesheet and
Node-free Rust build remain. Registry sources are installed unchanged, apart from
formatting, with provenance beside them. The host supplies the shell so the three
answers appear once and consent/completion keep their existing meaning.

## Consequences

The measured dependency and density costs in the spike are accepted. The old
hand-built controls and comparison switch are removed. Engine limits or schema
qualification failures remain local to the form: Raw, Decline and Cancel survive.
Numeric text the engine cannot parse is reported rather than put into its data;
Raw can still send that value with the wrong type. `format` chooses input widgets
but is not asserted as validation by schemaform.

The Linux release-backed callbacks run verifies all three broken-rule findings,
named summary entries, focus-stable answers, Raw switching and `end_turn`.
The [desktop matrix](../desktop-smoke-matrix.md) records the tested sizes and
Appearance combinations; macOS/Windows and screen-reader verification remain
platform acceptance work rather than inferred from WebKitGTK.
