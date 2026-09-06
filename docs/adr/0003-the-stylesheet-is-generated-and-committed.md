# The stylesheet becomes Tailwind's output, committed to the tree and compiled in

The window's look is one hand-written string in the binary (`crates/app/src/style.rs`). The UI redesign
ring ([#97](https://github.com/sagikazarmark/dioxus-chat.orig/issues/97)) replaces it with
**Tailwind**, and the generated stylesheet is **committed to the tree and compiled into the binary**
exactly as the hand-written one is today. It is not resolved as an asset. `cargo run` therefore still
needs nothing but a Rust toolchain and the WebView libraries, and **Node is required to *change* the
styles and to *verify* them in CI, never to build the tool**.

This is the decision the ring had to make rather than assume, because the inspector's build being
Rust-and-nothing-else is stated in four places
([README](../../README.md), [§15 q1](../architecture.md#15-open-questions--validation-gaps), the
[`justfile`](../../justfile)'s `run` recipe, and the stylesheet module's own comment) and the ring
makes every one of those sentences, as written, wrong. All four are amended alongside this document.
The ring that lands the toolchain is the one after it; what this document fixes is which property of
the build is allowed to change and which is not.

**What is not allowed to change is that the tool builds from a checkout with a Rust toolchain.** That
is the property every one of those four sentences was protecting, and it is worth more to this tool
than to most: the inspector exists to be pointed at somebody's agent by somebody who is debugging it,
and a tool that costs an afternoon of toolchain before it shows a frame is one they will not try. A
committed artifact keeps that promise whole. Node is then a *contributor's* dependency, in the same
category as the rust-sdk checkout `just testy` builds — needed to work on the thing, not to run it.

**This is `dioxus-demo-kit`'s pattern and deliberately not `app/`'s.** Both run Tailwind v4 over Rust
sources and the difference is what happens to the output. `app/` writes it to `web/assets/main.css`,
reaches it through `asset!("/assets/main.css")` and lets `dx` resolve it — and its own comment states
the consequence, that *"the app does not build without it"* (`app/web/src/ui/mod.rs`). That is
correct for `app/`: it is a web target, `dx build` is its build, and Node is already in its critical
path. It is exactly the property the inspector must not acquire, because acquiring it means the
sentence above stops being true and no amount of documenting it makes the tool cheap to try again.
`dioxus-demo-kit` commits its generated stylesheet for the neighbouring reason — it ships CSS into
someone else's page, so its consumers must not need Node — and the shape that solves their problem
solves this one.

**A committed build artifact is only honest if something checks it**, which is why the arrangement
costs a CI step. A generated file in the tree is a claim that it is what its source generates, and
nothing but habit keeps that true: a hand-edit to the committed CSS, or a source change generated on
one machine and not another, drifts silently and surfaces later as a rendering difference nobody can
attribute to a commit. So the inspector's CI job (`.github/workflows/ci.yaml`) gains a step that
regenerates the sheet and fails on a diff. That step is what makes *committed* a fact rather than a
convention — the same
move [#72](https://github.com/sagikazarmark/dioxus-chat.orig/issues/72) made when it turned *every
colour is a token* into a test instead of a promise.

**Node and npm are not pinned**, and the consequence is recorded rather than solved. The inspector's
`package.json` gets no `engines` and no `packageManager` field, because Nix provides both:
`devenv.nix` enables `languages.javascript` with npm, and `app/package.json` and `demo/package.json`
already pin neither for the same reason. The consequence is that **the pin does not travel**.
`devenv.nix` lives at the repository root and not under `phoenix/`, and everything under `phoenix/`
is destined to split out ([§3](../architecture.md#3-where-it-lives),
[ADR 0001](0001-the-inspector-keeps-its-own-context.md)) — so the repository the inspector becomes
inherits a `package.json` with an unpinned Node and no environment that supplies one. It is written
here rather than fixed because the fix is a second devenv under `phoenix/`, maintained from now until
the split for a repository that does not exist yet, and the cost of the drift it prevents is a
Tailwind version mismatch that the CI check above catches on the first commit that hits it.

## The daisyUI verdict

> **Historical verdict, superseded only as a verdict by
> [ADR 0005](0005-daisyui-owns-ordinary-components-and-themes.md).** The comparison, measurements
> and committed-stylesheet decision remain evidence. The criterion changed: the modern design and
> roomier proportions rejected below are now desired.

**Rejected at the time, from two mocks of one screen**
([#105](https://github.com/sagikazarmark/dioxus-chat.orig/issues/105)). No component of daisyUI 5.7
was adopted by that ring.

The criterion below was fixed before either mock was built and this is the ring's report against it,
which is what the ring was asked to produce and not a decision taken away from anybody: the two
mocks were on that ring's branch under `design/`; they were throwaway and were removed after
production adopted daisyUI. What survives is the vocabulary, the tokens
and this section — so the numbers below are the record, and they are written down for that reason
rather than as colour.

**1. Does daisyUI carry [§9](../architecture.md#9-screens)'s semantics without fighting its
defaults?** Mostly yes, and it fights on two of them.

- **`Appearance` does not have to change, either way it is configured** — the ground on which the
  ring said daisyUI would lose outright, and it does not. Asked for its own themes
  (`themes: light --default, dark --prefersdark`) it generates
  `@media (prefers-color-scheme: dark) { :root:not([data-theme]) }` plus explicit `[data-theme=light]`
  and `[data-theme=dark]` blocks, which is exactly what `Appearance` means: System is the *absence*
  of the attribute. Configured the way the mock actually adopts it (`themes: false`, its slots
  pointed at this window's tokens) it declares no scheme at all and `theme.css` stays the only thing
  answering the query.
- **The weight-not-dimness rule survives, and daisyUI leans on both channels it gave up.** `.btn`
  hard-codes `font-weight: 600` — this vocabulary's reserved weight, the one that says a capability
  row is the advertised half of the answer — and `.label` draws itself as
  `color-mix(in oklab, currentcolor 60%, transparent)`, which is dimness as a way of saying *this is
  a hint*. Neither reaches a capability row, because daisyUI has no component for a row that is a
  claim rather than a control, so the rule holds; it holds by not using the parts that would break it.
- **The five colours are carried, and renamed on the way.** The mapping works — what the agent said
  → `info`, what we sent and trouble-that-is-not-an-error → `warning`, trouble → `error`, live →
  `success`, traffic nobody has a name for → `secondary` — but the markup then speaks daisyUI's
  vocabulary, and daisyUI's is a grading vocabulary. `alert-warning` on a login the agent asked for
  is the wrong word for a state that is not an error ([§7.1](../architecture.md#71-driven-client--agent)),
  and `badge-error` is one keystroke away from the severity a conformance annotation is forbidden to
  have. The fit is partial by construction: **daisyUI has a slot for 11 of this window's 27 colour
  tokens** and the other 16 stay its own, `--out` and `--warn` collapse into one slot because
  daisyUI has one amber, and five `*-content` foregrounds have to be invented for accents this
  palette only ever uses as text.
- **The fourth fact keeps the row's own colour and weight** ([§7.7](../architecture.md#77-the-fourth-ring-what-was-driven)) —
  no colour of its own, no count, no ordering — for the same reason as the weight rule: nothing
  daisyUI offers was used on those rows. The conformance annotation the no-lane-no-severity-no-count
  rule is written for ([§9](../architecture.md#9-screens)) is a Timeline entry and is not on this
  screen at all, so neither mock tested it; what the mocks show is the pull towards it, which is
  that `alert-error` and `badge-error` are what an inspector's own voice looks like in daisyUI's
  vocabulary.

**2. Does it shrink the hand-written CSS, or add a layer that then has to be overridden?** It adds a
layer, and the numbers are not close. Both mocks share an identical nine-declaration page shell;
past that, the Tailwind-alone mock writes **no CSS at all** — every control is utilities in the
markup, which in the window is a Rust component written once — and the daisyUI mock writes **49
declarations in 8 rules**, plus two lines of plugin configuration: 28 to point daisyUI's slots at
tokens that already exist, and 21 in 7 rules to take back defaults this window had already decided
differently. Five of the eight components the mock uses needed an override — `btn`, `textarea`,
`alert`, `badge` and `list-row`; `fieldset`, `fieldset-legend` and `list` did not, and they are the
three that do the least. The generated stylesheet for the same screen is **17.9 kB against
44.8 kB**.

**And the overrides have to be unlayered to win.** daisyUI declares its components in layers of its
own that the cascade puts after both `components` and `utilities`, so an override written in either
— the two places a Tailwind project would naturally put one — loses to the default it is overriding.
The consequence is worse than the ceremony: a utility in the markup cannot override daisyUI either,
so `text-row` on a `.btn` does nothing and the size has to be fixed in a rule.

**Why not partial**, which is what the ring expected. A partial verdict would have been *form
controls and chrome yes, dense rows no*, and the mock is what it looks like: the boundary lands
where daisyUI has nothing to offer anyway, and on this side of it every component still had to be
told the palette, the radii, the sizes, the weight, the motion and the layout. The two components
that would earn their keep — a real `select` and a `tab` set — are exactly the two this
specification has already decided against on other grounds: the appearance control is a *native*
select deliberately ([§9](../architecture.md#9-screens)), for the keyboard behaviour and the
platform's own list, and the Console's tabs are pinned to an underline with a count and no badge.
What is left is a button, a text field and a pill, at a density daisyUI does not aim at.

**Component by component, so the boundary is a decision rather than a habit**: `btn`, `badge`,
`alert`, `textarea`, `fieldset`, `fieldset-legend`, `list` and `list-row` — rendered in the mock and
rejected; `label` — rendered, and given back inside the ring, because it is `white-space: nowrap`
(which widens a 320px rail until the panel overflows) and draws itself at 60% of the current colour,
which is dimness, the channel [#83](https://github.com/sagikazarmark/dioxus-chat.orig/issues/83)
took away from this window. `tab`, `select`, `modal`, `dropdown`, `drawer`, `collapse` — not tried,
because the window had no surface for them that was not already specified otherwise. Nothing was
adopted by that ring, and `crates/app/style/` declared no daisyUI plugin at that point.

**At the time, daisyUI was only a mock devDependency.** ADR 0005 later made it a production
stylesheet input while retaining its development-only Node build contract.

## The palette record

> **Historical palette evidence.** Winter and Business owned active theme values when ADR 0005 was
> adopted; Inspector Light and Inspector Dark later replaced them under that same ownership
> decision. The measurements below describe the inspector palette before either adoption and remain
> available to explain the semantic and contrast contracts that survived them.
>
> **The floors themselves stopped being a record and became a computation.** Every value in the two
> Inspector themes now goes through
> `style::tests::inspector_light_and_dark_clear_the_inspector_contrast_floors`, which derives each
> meaning-bearing alias from the theme's roles exactly as `theme.css` mixes it and measures it
> against every ground it rests on. A palette that does not clear 4.5:1 for text or 3:1 for a
> boundary is a build that does not pass, so there is no ratio here for a later palette to make
> stale — which is why the native-platform palette that replaced the values below shipped without
> adding a table to this section.

The desktop crate's stylesheet module carried, in its doc comment, the exact contrast ratios of both
palettes and the argument for what dark does not clear. **That is a record, and it moves here**: the
decision above makes the stylesheet a *generated file*, and a judgment that a regenerating command
can delete was never safely stored there. The rules that still bind the source stylesheet stay with
the module.

The floor is WCAG AA: **4.5:1** for text against its own ground, **3:1** for boundaries and signals.
**Against *every* ground it rests on**, which is the form the sweep settled on: seven grounds per
scheme — `--bg`, `--panel`, `--raised`, `--inset`, `--hover`, and the two permission tints — because
a hint inside a button whose background changes under the pointer has three grounds, and a token
that clears the floor on one of them is a token that fails on another.

### What was true until the sweep

**The light palette cleared it** at its resting grounds. At its tightest: `--faint` over `--raised`
at 4.58:1, `--control` over `--raised` at 3.17:1.

**The dark palette did not, and was left alone anyway.** Those were the values the window had before
a light palette existed, and [#72](https://github.com/sagikazarmark/dioxus-chat.orig/issues/72) —
which added the light one — asked for dark byte for byte so that its rendering would not move. The
two requirements cannot both hold, and fidelity won. What fell short:

- `--faint` — 3.47:1 on `--bg`, 3.21:1 on `--panel`, 2.87:1 on `--raised`. It is the placeholder,
  the disabled prompt, the hints under the permission options, and the field annotation trailing a
  capability row.
- `--muted` on `--raised` — 4.32:1, in the chips and the status pills.
- `--accent-ink` on `--accent` — 4.26:1, which is Launch and Send.

This list stays as the account of what was true and why, rather than being edited into never having
happened. Two things it did not mention were below the floor too, and the sweep found them by
measuring every pair rather than the three that were known: `--control` in dark, which was the same
value as `--edge` and therefore drew a control's boundary at **1.11:1** against the button it
bounds; and `--allow-edge` / `--reject-edge`, at **2.09:1** and **1.40:1** on `--raised`, which is
the one signal on the screen that must never be misread.

### What is true now

**Both schemes clear the floor** ([#106](https://github.com/sagikazarmark/dioxus-chat.orig/issues/106)).
The deferral expired with the render it was owed to. The ratio quoted for each token below is its
**worst** across the seven grounds, which in both schemes is `--hover` — the ground a control takes
under the pointer, and the reason a token measured only at rest can still be wrong.

**Dark**, and what moved to get there:

| Token | Was | Is | Worst text ratio | At rest (`--panel` / `--raised`) |
| ----- | --- | -- | ---------------- | -------------------------------- |
| `--faint` | `#5b6b7f` | `#8a99ab` | **4.61:1** | 6.02 / 5.39 |
| `--muted` | `#78889c` | `#a2adbb` | **5.89:1** | 7.69 / 6.88 |
| `--peek` | `#9fb0c3` | `#b7c4d2` | **7.56:1** | 9.86 / 8.83 |
| `--ink` | `#cbd5e1` | unchanged | **9.03:1** | 11.78 / 10.54 |

`--peek` moved although it already cleared the floor, and that is the sweep's one unforced palette
change: three quiet tones held to 4.5:1 have to fit between the floor and `--ink`, and a ramp whose
steps are 4.6 → 5.9 → 7.6 → 9.0 is one a reader can still tell apart. Leaving `--peek` where it was
would have put it 1.16× from `--muted`, which is a distinction only a colour picker can see. What
the ramp still means is unchanged: `--faint` is chrome, `--muted` is a second voice, `--peek` is the
preview of something, `--ink` is what was said.

The five meaning-carrying colours **did not move**, in either scheme, and clear the floor as they
are: in dark, `--out`/`--warn` **6.89:1**, `--in` **6.24:1**, `--bad` **4.84:1**, `--live`
**6.95:1**, `--odd` **6.88:1**.

The rest of dark:

- `--accent` `#1f6feb` → `#1566e6` and `--accent-hot` `#2b7bf3` → `#1657bb`, so that `--accent-ink`
  over them is **4.73:1** and **6.19:1** — Launch and Send, at rest and under the pointer. Both had
  to darken and neither could darken far: `--accent` is also the Console tab's underline, which is a
  signal and holds **3.39:1** on `--panel` and **3.04:1** on the `--raised` a hovered tab paints.
  `--accent-edge` is unchanged and borders those buttons at **4.51:1** against
  `--panel`, which is the side that matters: what identifies an accent button as
  a control is its own fill against the ground, and the rim is drawn on top of
  that. It is 1.33:1 against the fill it rims, and that is not the measurement —
  unlike the permission options below, where the border *is* the difference and
  is measured against the button it bounds.
- `--control` `#232c38` → `#627b9d`, **3.09:1** at worst and 3.61:1 on the `--raised` a button
  fills with. It stopped being `--edge`'s twin, which is the shape light always had: a control's
  boundary is what identifies it as something to click or type in, and the line between two panels
  is not.
- `--allow-edge` `#2c5f3a` → `#3d8451` and `--reject-edge` `#5f2c2c` → `#b05454`, at **3.44:1** and
  **3.16:1** against the button they bound and **3.12:1** / **3.37:1** against the tint it takes on
  hover. Allowing and rejecting must never read alike, and a border nobody can see is not a
  difference.
- `--focus` is unchanged at **4.14:1** on `--bg`, which is the ground every field it rings is
  painted in.

**Light** moved twice, both because of `--hover` — the ground the old measurements did not include:

- `--faint` `#636c75` → `#5b646c`, **4.58:1** at worst (it was 4.06:1 on `--hover` and 4.37:1 on the
  reject tint), 5.17:1 on `--raised`.
- `--control` `#7d868f` → `#757f88`, **3.10:1** at worst (it was 2.81:1 on `--hover`), 3.50:1 on
  `--raised`.

Everything else in light was already clear and is untouched: `--ink` 12.00:1, `--peek` 6.83:1,
`--muted` 5.75:1, `--out`/`--warn` 5.68:1, `--in` 5.65:1, `--live` 5.61:1, `--odd` 5.58:1, `--bad`
5.16:1; `--accent-ink` over `--accent` 6.82:1 and over `--accent-hot` 9.20:1; `--accent` 5.18:1,
`--accent-edge` 6.35:1, `--reject-edge` 4.16:1, `--allow-edge` 3.87:1.

**And the five colours are still five.** No hue moved in this ring, which is the strongest form of
"they still mean what they meant": `--out`/`--warn` 42°/36°, `--in` 210°/213°, `--bad` 0°/3°,
`--live` 127°/138°, `--odd` 269°/261° (dark/light). Direction, trouble, liveness and traffic nobody
has a name for are four signals carried by five values at four separations no smaller than 67° of
hue, in both schemes.

### The two tokens held below the floor, on purpose

`--rule`, the hairline between rows, and `--edge`, the line between one panel and the next. In dark
they run **1.01:1–1.34:1** against the grounds and in light **1.03:1–1.92:1**, and that is the
decision rather than the oversight: nothing in this window is known only by seeing one of them, and
at 3:1 they would be a grid drawn over a dense tool. The one place `--rule` carries a mark rather
than a line is the unlit `.caps` dot, which repeats in a glyph what its own row states in words and
is hidden from the accessibility tree for exactly that reason (#83). The row where one connection's
traffic gives way to the next draws its boundary in `--edge`, and the ordinal beside it — the thing
that says *which* connection — is lifted to `--ink` on that row, so the handover is stated in text
and not only in a line.

**One use had left that list before the sweep**
([#83](https://github.com/sagikazarmark/dioxus-chat.orig/issues/83)), and a use is all that left it.
What moved is the capability rows. One the agent did not advertise takes `--muted`, because on the
session lifecycle screen what an agent left out is as much of an answer as what it claimed — those
rows are content, and the shortfall above was a list of chrome. On `--panel`, which is what the rail
paints and therefore what those rows sit on, `--muted` was 4.83:1 in dark and 7.12:1 in light — AA
in both even then, and not the 4.32:1 recorded for `--muted` over `--raised`, which is a different
ground. It is **7.69:1 in dark** now. Nothing else moved off `--faint`, the field annotation inside
those same rows included — and `--faint` clears the floor everywhere today, which changes what it
costs to use and nothing about what it means.

That cost the rows the channel which used to say which was which, which is why they say it two other
ways instead — weight, and words — a rule [§9](../architecture.md#9-screens) states and the redesign
is held to.

## Considered Options

- **Keep the hand-written stylesheet.** The status quo, and it works: every colour is a token, both
  schemes are driven from one set of declarations, and a test enforces it. Rejected because the
  discipline is bespoke — nothing else in the repository knows the vocabulary, adding a control means
  writing its states from scratch, and the repository already runs Tailwind in three places. The
  argument for this option was never that it was better; it was that the alternative cost Node, which
  is the cost this decision is about paying in the narrowest possible place.
- **`app/`'s pattern — generate the sheet and resolve it as an asset through `dx`.** Rejected above.
  It is the shape a Dioxus app normally takes, and taking it would make `dx` and Node prerequisites
  of a build, which is the one property this decision protects.
- **Generate at build time from `build.rs`.** Tempting, because it deletes the drift the CI check
  exists to catch: there is no committed artifact to disagree with its source. Rejected because it
  moves Node from *a contributor's dependency* into *the build*, which is `app/`'s outcome reached by
  a different road — every `cargo build` would need npm on the machine and network on the first run.
- **A `generate.mjs`, as `dioxus-demo-kit` has.** Rejected as machinery for a problem the inspector
  does not have. That script exists to scope CSS into a page the crate does not own — a class prefix,
  a `.ddk-root` scope, a scoped preflight, an allowlist of daisyUI modules. The inspector owns its
  whole document. The Tailwind CLI plus a script that regenerates and diffs is the whole build.
- **Pin Node and npm in `package.json`, or add a devenv under `phoenix/`.** Rejected for now, and the
  reasoning is the unpinned-toolchain paragraph above: it buys a pin the current environment already
  supplies, and pays for it every day until a split that has no date.

## Consequences

- **`cargo run -p acp-inspector` is unchanged**, and so is `just run`. A contributor who never
  installs Node can build, run and test the inspector; the four sentences that promised this are
  amended to say what Node *is* needed for rather than to stop promising it.
- **The inspector's CI job stops being Rust-only.** It gains a Node step whose only job is to
  regenerate the stylesheet and diff it, and a failure there is a stale committed artifact rather
  than a broken tool.
- **The colour-literal rule moves from the output to the input.** Generated CSS is nothing but
  literals, so the test that asserts no colour literal appears below the token blocks cannot survive
  against the generated file. The same rule, asserted the same way, over the source stylesheet's
  theme block — [#72](https://github.com/sagikazarmark/dioxus-chat.orig/issues/72)'s guarantee is
  kept by moving what it reads, not by dropping it.
- **A generated file cannot hold an argument.** The palette record above is the first thing that had
  to move for that reason; anything else the module comment carries which is a *record* rather than a
  rule belongs in this directory or in the specification for the same reason.
- **Node enters `phoenix/`, and travels with it.** The `package.json`, the source stylesheet and the
  check script split out with the inspector; the environment that provides Node does not.
