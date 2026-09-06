# daisyUI owns ordinary components and themes

> **Amended by [ADR 0006](0006-the-window-is-drawn-as-an-application.md)** on layout and register:
> the regions are inset cards on the chrome ground rather than full-bleed areas divided by
> hairlines, and the uppercase label register is retired. The palette, the themes, the contrast
> floors, the depth decisions and the component ownership below are unchanged, and no colour value
> moved.

## Context

[ADR 0003](0003-the-stylesheet-is-generated-and-committed.md) rejected daisyUI after comparing two
mocks under a criterion that prioritized the inspector's exact compact proportions and the smallest
generated stylesheet. Its comparison, measurements, palette history and committed-stylesheet
decision remain valid records of that choice.

The criterion has changed. A modern daisyUI design and roomier ordinary controls are now desired.
Every desktop surface has adopted daisyUI, so retaining hand-rolled control chrome or globally
transforming standard classes back into the old design would preserve the rejected criterion under
a component-library name.

## Decision

- Production uses daisyUI's unprefixed standard component classes first. A daisyUI component or
  variant that satisfies the required semantics is preferred to a hand-rolled equivalent.
- Inspector Light (`inspector-light`) owns the explicit light theme and Inspector Dark
  (`inspector-dark`) owns the explicit dark theme. System is the absence of `data-theme`, using
  daisyUI's preferred-dark selection. The persisted and user-facing choices remain System, Light
  and Dark.
- **Greys separate regions and hues say things, and neither does the other's job.** The themes are
  three quiet steps of one cool near-neutral — a document surface, the chrome surface the rails,
  toolbar, composer and Console are drawn in, and the step that bounds and insets them — over
  saturated accents. A screen whose greys were also carrying meaning is a screen with nothing left
  to carry it with, which is what a fourth grey or a desaturated accent costs.
- **The meaning-bearing aliases are mixed three quarters of the way to their role**, not half.
  Half was what a value has to be to be read only as text, and it delivered direction, liveness and
  trouble as shades of the text colour. 78% is the largest share the tightest of the five clears
  4.5:1 with on the tightest ground; boundaries owe 3:1 and go to 72%. Both numbers are recomputed
  by `style.rs` from the same roles, so raising either past what a hue can carry fails the build.
- **Panels are flat, pressable controls are not, and one thing is a card.** `--depth: 1` spends
  daisyUI's step on a control's top highlight and the shade directly under it; `--shadow-control`
  gives the same to the near-white controls daisyUI cannot, because it tints its shadow from a
  fill. `--shadow-card` is the one step above that, and it belongs to the Timeline's evidence
  boxes alone — a tool call and a permission request, which are objects the Agent produced quoted
  whole inside the row reporting them, and which appear a few times per screen rather than once per
  row. No panel and no row is lifted; overlapping content remains `--shadow-lift`'s.
- Radii are 8px on anything pressed or typed in and 12px on a box that holds them, which is rounder
  than the platform's own controls and is the cheapest thing on screen that reads as considered.
- daisyUI owns ordinary component palette, spacing, radius, elevation, font weight and transition
  timing. The retained Inspector type roles set ordinary content at approximately 13px, dense
  evidence at 12px, metadata at 11px and short category labels at 10px; ordinary compact controls
  remain near that scale rather than introducing a second component system.
- The compact geometry is a 40px toolbar, a 248px Agent rail, approximately 28px ordinary and
  toolbar controls, 12px panel gutters and 4-8px local gaps. Content may expand a row when the Agent
  provides text that must remain readable.
- Heroicons Outline is the one icon family. It is used sparsely for the agreed toolbar, composer
  and disclosure controls at 15px; icon-only controls use an approximately 28px square target plus
  a tooltip, accessible name and shared visible focus treatment.
- One keyboard focus treatment, and it is the platform's ring rather than a hairline held away from
  the control: three pixels of the accent at partial alpha, offset by one. Alpha is what lets the
  same ring sit on an accent-filled button and on the chrome behind a field.
- Where a component has a shape or a variant that already means the thing, it takes it. A badge is
  a state and never something to press, so every badge is a capsule and every badge that carries a
  meaning is `badge-soft` — the tint, the hairline and the word all mixed from one
  `--badge-color`, so no single channel is the only one saying it. A small closed set of values is
  a segmented control — one sunken track with the current value raised out of it — rather than a
  row of buttons distinguished by tint. The two sides of a diff carry the allow and reject grounds,
  which are the same two hues for the same reason.
- Custom CSS remains for layout, inspector meaning, evidence presentation, accessibility, ordering
  and behavior. It may narrow a component for one of those contracts, but it does not recreate a
  second general-purpose control system.
- daisyUI remains a development dependency because Node generates the production stylesheet. The
  generated stylesheet remains committed and compiled into the binary; a valid checkout still
  builds and runs without Node.
- The operating system continues to own the real window frame, scrollbars and system UI font
  stack. The Inspector does not draw or skin substitutes for them.

## Retained inspector contracts

- Meaning-bearing inspector aliases derive from theme roles and satisfy the documented contrast
  floors. Meaning is not conveyed by color alone.
- Raw JSON, Frames, identifiers and diagnostic lines remain monospace, and raw-first evidence stays
  available where the architecture requires it.
- Agent Capability and current-value facts remain readable in words; dimness is not their only
  distinction.
- Conformance Annotations remain evidence, not severity: no severity lane, grade or count.
- Arriving Frames, Timeline entries and diagnostic lines do not animate. Layout-moving evidence
  transitions remain forbidden, and readers requesting reduced motion receive none.

## Supersession

This ADR supersedes only ADR 0003's daisyUI rejection. It does not supersede that ADR's comparison,
measurements, palette history or committed generated-stylesheet decision. It amends ADR 0004 by
relaxing the old exact density, spacing, radius, global font-weight reservation and transition
timing while retaining the compact ranges and the semantic, contrast, evidence and motion
constraints above.
