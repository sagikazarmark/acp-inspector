# Elicitation renderer registry install

Installed with Dioxus CLI 0.7.10 from registry commit
`68726edd4fa8c9ae6d78ccc2e805b8c253ede361` (`main`, after the merges of
registry PRs #39 and #40). To reinstall, from `crates/app`, run one command per
component:

```sh
for component in schemaform_daisyui button field input native_select checkbox textarea \
  schemaform_daisyui_collection; do
  dx components add "$component" \
    --git https://github.com/sagikazarmark/dioxus-daisyui-components \
    --rev 68726edd4fa8c9ae6d78ccc2e805b8c253ede361 --force
done
```

Name every component. `--force` refreshes only the components named on the
command line, so installing the renderer alone leaves its dependencies at
whatever revision is already on disk. Keep it one component per command:
naming the renderer and an add-on together makes `dx` copy the renderer from
two revisions at once (the add-on pins its own), which fails intermittently
with "Destination directory … already exists" and can leave a partial install.

`checkbox` is not a renderer dependency (booleans are a native checkbox now);
the launch form uses it directly. `textarea` is the launch form's multi-line
field (FM1). `schemaform_daisyui_collection` is the opt-in homogeneous-array
add-on, applied in `elicitation.rs` so string arrays without an enum keep the
daisyUI collection. The radio and compound-select add-ons are not installed.

`dx` rewrites `crates/app/Cargo.toml` on each install. Afterwards:
- pin `schemaform` and `schemaform-dioxus` exactly again (`=0.6.1`; `dx` writes a
  caret requirement);
- remove `features = ["router"]` from `dioxus-primitives` (see below).

The installed Rust source is unchanged except for workspace `cargo fmt` formatting.
The upstream repository is MIT OR Apache-2.0, as are this repository and schemaform.

The renderer uses released `schemaform` and `schemaform-dioxus` 0.6.1 and
`dioxus-field` 0.8.0. The host's shell, protocol preprocessing and answer handling
live outside these installed modules. The host selects `Density::Compact` alongside
`Appearance::None` and gives the renderer's own buttons the host's `btn-quiet`
look through `ButtonClasses`; its explicit CSS lays out the registry's compact
markers without modifying the installed source.

## Host dependency feature selection

The installer enables `dioxus-primitives/router`; this host deliberately disables
it in `crates/app/Cargo.toml`. None of the installed components uses routing, and
both desktop and web builds compile the complete installed source without it.
After reinstalling, retain `default-features = false` and remove `features =
["router"]` if the installer restores it.

Against `f965522`, this removes four normal-edge package/version entries on Linux:
`dioxus-router 0.7.10`, `dioxus-router-macro 0.7.10`, `base16 0.2.1`, and
`sha2 0.10.9`. The count is **414 → 410 external entries** (workspace crates
excluded; strip Cargo's `(*)` marker before deduplicating). The form integration's
increase over its original baseline is therefore **51**, down from 55. This is a
resolved-graph measurement, not a binary-size or compile-time measurement.

The remaining `palette`, `time`, and SDK timing dependencies are unconditional in
the pinned primitives crate. Palette serves its color picker; calendar/date-picker
code uses time, and compound Select uses SDK timing. Removing local unused widget
files cannot eliminate unconditional upstream dependencies. Further reduction
needs an upstream component-feature split; the installed renderer remains intact.

Reinstalling at `68726ed`, with `select/` and `radio_group/` removed, leaves the
count where `3e79e57` had it: **413 → 413 external entries** on Linux. Only
versions move (dioxus-field, schemaform, schemaform-dioxus, and jsonschema and
referencing 0.54.0 → 0.58.6 through schemaform). Deleting the unused widgets
shrinks the stylesheet, not the graph: `dioxus-primitives` is still reached
through `field`, `input`, `native_select`, `button` and `checkbox`.
