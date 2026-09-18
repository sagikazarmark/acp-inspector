# Elicitation renderer registry install

Installed with Dioxus CLI 0.7.9:

```sh
dx components add schemaform_daisyui \
  --git https://github.com/sagikazarmark/dioxus-daisyui-components \
  --rev 025d2228e9253032ac6b8deee4890c46e35379b5
```

Run from `crates/app`. The installer fetched the renderer package and its seven
component dependencies (pinned by the install manifest to `3dfbfd2521e06e539767d10fbf0dfa2171d290ed`).
The installed Rust source is unchanged except for workspace `cargo fmt` formatting.
The upstream repository is MIT OR Apache-2.0, as are this repository and schemaform.

The renderer uses released `schemaform` and `schemaform-dioxus` 0.5.0.
The host's shell, protocol preprocessing and answer handling live outside these
installed modules. Reinstall deliberately with `--force` when updating the registry.

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
