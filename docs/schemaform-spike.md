# #5: schemaform in the Linux window

> Historical 0.4.1 experiment. The independent numeric fix and upstream 0.5.0
> findings fix subsequently cleared its blockers. Production adoption is recorded
> in [ADR 0013](adr/0013-schemaform-draws-the-elicitation-form.md); the measurements
> and failures below describe the original spike, not the current build.

2026-09-17. Throwaway branch `spike/5-schemaform`, baseline `286859a`.
This is the write-up for [#2](https://github.com/sagikazarmark/acp-inspector/issues/2),
not an adoption. The branch must not be merged.

> **Final workspace check changed the conclusion:** the combined release graph
> enables `serde_json/float_roundtrip`, defeating #3's stock-rounding guarantee
> for two tripwire cases. See §4 and [#8](https://github.com/sagikazarmark/acp-inspector/issues/8).
> The earlier isolated core checks passed; the full workspace suite does not.

## Reproduction and limits

- `schemaform` and `schemaform-dioxus` **0.4.1**, released crates, exact pins.
- `schemaform_daisyui` installed by `dx components add` from registry revision
  `1a61f48cd9402345feabc058898fe0915ec1ac10`; see the installed provenance file.
- Dioxus 0.7.10, Rust 1.97.1, WebKitGTK 2.52.4, Linux x86_64, Xvfb and Openbox,
  Mesa 26.2.2 software rendering. No display was initially available. The first
  launch aborted its WebView with `EGL_BAD_PARAMETER`; explicitly supplying Mesa's
  EGL vendor JSON and DRI directory made it render.
- Testy revision `726c5030bfaa88cfdac2fb1f71a63abb331ce586`. This ten-field schema
  has **no defaults**. Seeding was therefore exercised with the literal fixture
  and the engine test, not claimed from an unmodified Testy run.
- Keyboard input used `xdotool` Tab/Enter/Space/arrows and text entry. WebKit's
  inspection socket read DOM, focus, findings and values; it did not click or fill
  controls. Each inspection timed out at four seconds; each keyboard step at 25.
  Text entry needed a 100ms inter-key delay: at 5ms the baseline lost characters.
- Both Light and Dark were inspected at a normal 1440-wide window and a 960 x 640
  window. The host's fractional scale produced CSS widths **1384** and **924**;
  `scrollWidth == clientWidth` at both. This is a focused comparison, **not** a
  claim that the entire smoke matrix passed at exact CSS 1440 x 880 / 960 x 640.
  System tracking, screen readers, macOS and Windows were not exercised.

Run normally with `cargo run -p acp-inspector`. `INSPECTOR_FORM=hand-built` selects
the old form for comparison. Both build the same dependency graph; dependency and
clean-build comparisons below instead use the original commit.

## 1. Reading

**The callbacks scenario reached `end_turn`, keyboard only, through the registry
form. Accept sent three broken rules and retained focus. Not every finding was
beside its field.** The inputs were age `999`, confidence `2`, confirmed checked,
name absent, priority selected and one tag checked. Testy reported
`elicitation/form_session_accept: ok accept content_fields=5`, and finally
`elicitations: completed` and `stopReason: end_turn`. Later requests were also
answered by keyboard; one run accepted the request whose message asks for Cancel,
which Testy permits. The message is not an enforced answer.

| Smoke row / field | Baseline | Registry form |
|---|---|---|
| age | native number; stated 0–120; 999 sent | text/decimal edit; 999 sent; maximum finding beside field |
| available_at | datetime-local | datetime-local; keyboard reachable |
| birthday | date | date; keyboard reachable |
| confidence | native number; stated 0–1; 2 sent | text/decimal edit; 2 sent; maximum finding beside field |
| confirmed | native checkbox | native checkbox; toggle creates absent value |
| email | email | email; same input type |
| homepage | url | url; same input type |
| name | required text, no inline finding | required text; absent-value finding **only in summary** |
| priority | low, normal, high | high, low, normal: engine sorts choices |
| tags | rust, acp, testy | acp, rust, testy; native checkboxes; first toggle works |
| Raw | arbitrary object or absent | same; `{"confidence":"old"}` survived Form → Raw, while the form kept `1.50` |
| Accept | sends invalid content; no inline findings | advisory send; two local maximum findings plus three summary findings |
| answered state / focus | stale waiting panel; clicks reveal Trace | explicit state props refresh the panel; click boundary retains focus on Accept |
| URL / completion | existing panel | same panel; consent remains separate from completion |
| Trace | captured Frame text | Frame text retains literal schema tokens; combined-feature typed tripwire fails (see §4) |

The registry summary repeats “Value does not satisfy maximum.” twice and says
“Value does not satisfy required.” without naming the missing field. Its focus
links help navigation, but the prose is insufficient to identify each rule at a
glance. The absent `name` has no local required finding. This is
[#7](https://github.com/sagikazarmark/acp-inspector/issues/7), an adoption condition.
`format` chooses the widget but is not validated by this engine.

Presence controls add a Set/Remove stop to many fields. The form is substantially
taller than the old one. At the narrow size the Session region must be selected
in the footer before fields can be reached; its own scroller keeps them reachable.

Two existing host bugs surfaced: control clicks bubbled to the Timeline's
Frame-navigation handler, and identity-equal request handles hid answer-state
changes from the Panel. The candidate fixes are in this branch and filed separately
as [#6](https://github.com/sagikazarmark/acp-inspector/issues/6). These are not
benefits supplied by schemaform.

## 2. Styling

`Appearance::None` removes the renderer package's layout utilities, **not** those
of its composed registry Field parts. The live ten-field form emitted `grid`,
`min-w-0`, `gap-2`, `flex`, `items-center`; findings add `whitespace-normal`.
Hidden labels can additionally emit `sr-only`. All are ordinary Tailwind sources.

Reached component classes: input, checkbox, select, button, fieldset and label;
findings add `alert alert-soft alert-warning` and **link**. `fieldset-legend` is
reached by the multi-select even though this is flat ACP data. For a flat form
without any array it is not reached. **card and join** belong to homogeneous-array
items and are not reached here. **radio** needs an authored radio widget and is
not reached. The host supplies a small shell so there is one Accept next to
Decline/Cancel, rather than a second Submit button inside the form.

ADR 0003's `source(none)` and committed-sheet rule survive unchanged: the existing
explicit `@source "../src"` sees the installed package. That scanner also sees
unused variants and Default-appearance utility literals; runtime None does not
remove them from generated CSS. ADR 0005's include list gains **link only**.
The host adds flat grid spacing, full-width inputs, checkbox-row spacing, summary
icon sizing and field-finding color. Ordinary component presentation remains daisyUI's.

| Sheet | Bytes, unminified committed output |
|---|---:|
| Baseline | 117,334 |
| Registry scanned, existing include list | 133,108 |
| Link included and host layout rules | 134,055 |
| Total growth | **16,721 (14.25%)** |

Re-skinning the renderer is not smaller as a source change: this flat form needs
one new include-list entry and a handful of host layout rules. Forking all widget
classes would add maintenance to save unused scanner output. A carefully bounded
utility source list could reduce the sheet, but was not implemented or measured;
there is no measured re-skin byte saving to claim. Widening to card/join/radio is
unnecessary for this ACP profile.

## 3. Weight

Clean dev builds, downloaded dependencies, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_DEV_INCREMENTAL=false`, `cargo build -p acp-inspector --timings`:

| Measurement | Baseline | Spike |
|---|---:|---:|
| Clean build, one sample | 104 seconds | 156 seconds |
| Unique normal-edge package/version entries, including workspace crates | 361 | 416 |
| New unique packages | — | **55** |

These are one-run wall times, not a benchmark distribution. The baseline target
was cleaned after disk exhaustion; the spike used a new empty target directory.
Both ran in this machine with the same debug/incremental settings. Cargo's tree
`(*)` repeat marker must be stripped **before** deduplication; counting printed
lines as packages would incorrectly report 463 and 533.

schemaform directly adds `jsonptr`, `jsonschema`, `referencing`, a second
`num-bigint` (0.5.1), `serde_path_to_error`, and `sha2` 0.11. The validation graph
adds regex/fancy-regex, fraction, URI/email validation, UUID SIMD and exact-number
helpers. serde and serde_json were already here. The Dioxus adapter reuses the
window's Dioxus crates. The registry additionally brings `dioxus-field`, the git
`dioxus-primitives` package, router, SDK time, palette and their helpers. This
weight includes the registry's complete installed choice-widget implementations,
even though the ACP form uses native selects.

## 4. Fidelity

`scripts/spike-testy-literals.py` proxies Testy and substitutes only confidence's
schema tokens: default **1.50**, maximum **1e2**. On Linux the form input's value
was exactly `1.50`, and Raw showed `"confidence": 1.50`. The captured Frame still
contained `1e2`. Bounds are findings, not a visible literal constraint caption:
the registry form does **not** display `1e2` beside the input. The Trace is where
that literal remains readable.

`ElicitationRequest::raw_form()` extracts a RawValue directly from Frame text;
typed v1 still reads 1.5 and 100.0. The integration test asserts the raw schema,
typed values and retained Frame together. All five existing numeric tripwire
tests pass with **isolated** `serde_json/arbitrary_precision`; all 21 core
Elicitation tests also pass under that feature. That isolated run was insufficient.

The final `cargo test --locked` workspace run activates a second unified feature:
`schemaform -> jsonschema/jsonschema-value -> serde_json/float_roundtrip`.
The existing seam calls `serde_json::from_str::<f64>`, which is itself changed by
that feature. Both cost and Elicitation-bound tests fail for
`0.146675314082485333`: actual `0.14667531408248533`, required stock result
`0.14667531408248535`. This is a real change to what the typed layer reads, not a
test expectation to update. The literal examples 1.50 and 1e2 still work, but
**#3 does not yet contain the full released graph**. Filed as
[#8](https://github.com/sagikazarmark/acp-inspector/issues/8); the tripwire stays red
in this throwaway branch. Exact-value input is not permission to bypass #3.

## 5. What the seam costs

`elicitation_schema.rs` has **52 production lines**, including comments/blanks,
before its test module. It quarantines unknown property types, drops their required
entries, marks ACP multi-select arrays unique, closes the root, and selects Draft
2020-12. Default seeding and advisory answer mapping live at the renderer boundary.
Raw is still the escape hatch if compilation fails or a value has the wrong type.

Quarantine and multi-select uniqueness describe this protocol profile and are
reasonable host rules. Closing the root solely to silence a generic-engine advisory
is the least attractive rule: it changes the engine's interpreted schema while
the Agent never said that. It applies only to the form copy; Raw and the Trace are
not rewritten. The preprocessing is small; the adapter lifecycle, error reporting,
answer mapping and retained Raw surface are the larger integration cost.

## Conclusion

The engine can run here against a release and send the demonstrated advisory answers.
It cannot yet enter production: the combined feature graph changes typed rounding.
The flat form gains findings and edit handling, but pays in dependencies, build
time, height, extra keyboard stops and generic finding text. Keep the hand-built
form pending numeric containment, the field-targeting/summary follow-up and a deliberate adoption
decision. Preserve the independent host fixes through their own ticket. This branch
is evidence for that decision and remains unmerged.

## Final verification and review

`cargo test --locked --no-fail-fast --quiet`: **576 passed, 2 failed**, including
the doc test. Both failures are the existing numeric tripwires described in §4;
all targets were allowed to finish. Desktop and web `cargo clippy --all-targets
-- -D warnings`, formatting, stylesheet regeneration check and `git diff --check`
passed. Targeted Elicitation tests were run throughout development.

Two-axis review against `286859a` found local failure handling that could remove
Raw through the root error boundary and recoverable edit feedback that could
permanently remove Accept from keyboard navigation. Both were corrected: form
preparation failures render locally, operation feedback does not gate advisory
submission, and the Raw shape guard is also checked by the click handler. A
262,145-character default regression test failed before the local-fallback fix
and passes afterwards. The shared advisory-content mapping removes duplication
between Enter submission and the host's Accept button.
