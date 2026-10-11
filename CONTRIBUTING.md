# Contributing

Thanks for helping. The README's [Working on it](README.md#working-on-it)
section is the long form; this is the short one.

## Set up

- **Rust**: rustup picks up the pinned toolchain from `rust-toolchain.toml`.
- **just** runs every task (`just --list`).
- **Python 3** runs the scripted agents the test suite spawns.
- **Linux**: WebKitGTK and friends; on Ubuntu,
  `sudo apt-get install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev`.
- **Node** only to change or verify the stylesheet.

Or run `devenv shell`, which provides all of it.

## Before you open a pull request

```sh
just          # build Testy (pinned rust-sdk rev) and run the whole test suite
just check    # fmt, clippy for both renderers, number features, stylesheet check
just deny     # dependency advisories, licences and sources
```

CI (`.github/workflows/ci.yaml`) runs the same checks on macOS and Ubuntu, and
compiles on Windows.

## The stylesheet

`crates/app/style/sheet.css` is generated from `input.css` and `theme.css` by
Tailwind and daisyUI, and committed. After changing the styles, run `just css`
and commit what it wrote; `just check` fails if the two disagree.

## Testy

The integration tests drive Testy, the rust-sdk's test agent, built by
`just testy` from the rev pinned in `scripts/build-testy.sh`. To try another,
`ACP_RUST_SDK_REV=<rev> just testy`. Moving the pin is its own commit, with
whatever the new Testy needs.

## Decisions

Architectural decisions are in [`docs/adr/`](docs/adr/) and the spec is
[`docs/architecture.md`](docs/architecture.md). A change that goes against one
of them should say so and update it.
