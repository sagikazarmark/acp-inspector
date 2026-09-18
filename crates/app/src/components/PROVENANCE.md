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
