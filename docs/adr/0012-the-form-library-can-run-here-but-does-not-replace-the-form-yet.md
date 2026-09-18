# The form library can run here, but does not replace the form yet

> Superseded by [ADR 0013](0013-schemaform-draws-the-elicitation-form.md) after
> numeric containment landed and schemaform 0.5.0 shipped the finding fixes.

## Context

The Elicitation form is finished for ACP's flat profile. Replacing it with
schemaform would buy shared editing machinery, findings and accessibility
scaffolding, and dogfood a library used elsewhere by the author. It would not add
an Agent Capability the inspector lacks.

The first evaluation against 0.3.0 found desktop DOM operations missing, submission
gated on validity, and a Cargo feature leak. The second established that the first
two had been addressed by [the DOM bridge](https://github.com/sagikazarmark/schemaform/issues/29)
and [advisory submission](https://github.com/sagikazarmark/schemaform/issues/33).
The third concern was more serious than first reported: `serde_json/arbitrary_precision`
makes `maximum: 1e2` fail typed Elicitation decoding, and can silently erase cost
`amount: 0.10` and annotation `priority: 0.50` through default-on-error fields.
The maintainer [keeps exact numbers](https://github.com/sagikazarmark/schemaform/issues/31);
tracing-bunyan-formatter and apache-avro's experience with feature unification was
precedent for treating the leak as a boundary concern rather than an app-local flag.

[#3](https://github.com/sagikazarmark/acp-inspector/issues/3) put the canonicalizing
typed-copy seam and presence-asserting tripwire here before the form engine entered.
It is useful independently of adoption. The Trace remains Frame text; a renderer
can separately ask for literal requestedSchema tokens.

The [release-backed Linux spike write-up](https://github.com/sagikazarmark/acp-inspector/issues/2#issuecomment-5722197025)
was posted before this decision. schemaform/schemaform-dioxus 0.4.1 and the
[registry follow-up](https://github.com/sagikazarmark/dioxus-daisyui-components/issues/24)
ran Testy's callbacks to `end_turn` by keyboard, with three invalid values/presences
accepted. The form retained `1.50`; the Trace retained `1e2`. The measured price was
55 new unique packages, a clean build increasing from 104 to 156 seconds, and
16,721 stylesheet bytes. Only `link` needed adding to the daisyUI include list.
The missing-required finding stayed in the summary instead of beside its field.

The final workspace check then exposed another feature leak: jsonschema enables
`serde_json/float_roundtrip`, changing the very f64 parser #3 uses to restore stock
rounding. Two presence/value tripwires fail for `0.146675314082485333`. The isolated
arbitrary-precision core run passed; it did not test the released combined graph.
[#8](https://github.com/sagikazarmark/acp-inspector/issues/8) records the blocker.

## Decision

Keep the hand-built form for now. Desktop execution, advisory
submission, exact-value input, constant choices, formats, multi-selects and seeded
defaults are no longer reasons to reject the library on structural grounds, but
the typed numeric seam still needs combined-feature containment. The
spike is not merged, and its styling additions are not a production amendment to
ADRs 0003 or 0005.

Before adoption, resolve [combined-feature numeric containment](https://github.com/sagikazarmark/acp-inspector/issues/8)
and [field targeting and useful summary names](https://github.com/sagikazarmark/acp-inspector/issues/7),
accept the measured dependency and density costs explicitly, and finish the
platform/Appearance smoke rows the [spike](https://github.com/sagikazarmark/acp-inspector/issues/5)
leaves unverified. Keep the release pins and numeric tripwire in any future trial.
The host's independent [focus and answer-state fixes](https://github.com/sagikazarmark/acp-inspector/issues/6)
belong in their own change regardless of this choice.

## Consequences

The current form still owes [inline findings](https://github.com/sagikazarmark/acp-inspector/issues/1).
Choosing not to adopt now does not discharge that work. Nor does successful
WebKitGTK execution establish WKWebView, WebView2, screen-reader or System-theme
acceptance. The literal accessor and preprocessing remain spike code until a
consumer is chosen; the canonical typed seam already belongs to the inspector.
