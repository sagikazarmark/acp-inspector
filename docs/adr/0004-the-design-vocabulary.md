# The look is a named vocabulary, and each name has a rule for when it applies

> **Further amended by [ADR 0006](0006-the-window-is-drawn-as-an-application.md).** The type scale
> below has four steps rather than five: `--text-micro` and `--tracking-label` are removed with the
> role they existed for, and no rule in the sheet sets `text-transform: uppercase`. The flat-window
> rule now describes what is *inside* a region — every region is itself a bounded card inset into
> the chrome ground, at `--radius-card`, and the single-edge hairlines between them are gone.

> **Amended by [ADR 0005](0005-daisyui-owns-ordinary-components-and-themes.md).** The exact type
> sizes, spacing steps, radii, global font-weight reservation and transition durations below are the
> historical baseline applied by the fourth ring, not global constraints on current components.
> daisyUI now owns ordinary component proportions. The active constraints retained from this record
> are semantic colour aliases and contrast floors, monospace evidence, readable advertised/current
> facts, non-severity Conformance Annotations, no animation of arriving evidence, no layout-moving
> evidence transitions, and reduced-motion support.

The window is painted by Tailwind now ([ADR 0003](0003-the-stylesheet-is-generated-and-committed.md))
and looks exactly as it always did: type between 11 and 13px with 600-weight micro-labels throughout,
three border radii with no scale between them, five box-shadows, five tracking values, and not one
transition anywhere. There is no type scale, no spacing rhythm and no motion, which is to say there
is nothing for a sweep of eleven modules to apply — only a habit of choosing a plausible value per
rule.

**So the vocabulary is decided before the sweep, and it is decided as tokens.** They live in
[`crates/app/style/theme.css`](../../desktop/style/theme.css), in one `@theme static` block, and every
one of them is a name for a decision rather than a shorthand for a number. What follows is each
scale and the rule for when its steps apply; the file itself carries the same rules beside the
declarations, because a token whose rule is one directory away is a token that will be used for
whatever it looks like.

**It takes AgentUI's finish and not its proportions.** Borrowed from `app/`'s stylesheet: the
spacing rhythm, the radii, the elevation, the control shapes, the transitions, a real type scale,
and — the shape rather than the values — its semantic token indirection. Not borrowed: the reading
measure, because the inspector has no reading column, and any loosening of density, because rows
read at a glance need to be close together.

## Historical scale ownership

For the original sweep, every namespace was cleared with `initial` before this vocabulary declared into it. A step that
exists beside the scale is a step somebody can reach for without deciding anything, and `p-3` next
to `p-group` is not a smaller version of the rhythm — it is the absence of one. The same line does
something stronger for colour: **`--color-*: initial` is what makes `bg-red-500` not exist**.
[#72](https://github.com/sagikazarmark/dioxus-chat.orig/issues/72)'s guarantee is asserted over the
rules a person writes in this project, and no test here can see a class name in a Rust file — so the
only way to keep *every colour is a token* true once utilities arrive is for there to be no other
colour to write. `crates/app/src/style.rs` held that with a test. Current production retains the colour
namespace constraint but no longer clears Tailwind's ordinary type, spacing, radius, weight or
timing scales.

The block is `static` so that every token is emitted whether or not a utility mentions it: the
hand-written rules that survive until the sweep reaches them name these tokens in `var()`, which is
not a class and is not something Tailwind's scanner counts.

## The historical type scale

Five sizes, and each has one job.

| Token          | Size            | Where it applies                                                              |
| -------------- | --------------- | ----------------------------------------------------------------------------- |
| `--text-micro` | 10px            | The uppercase label a group wears, and nothing else.                          |
| `--text-label` | 11px            | Chrome beside content: a timestamp, a hint, a count, the field a claim was in. |
| `--text-row`   | 12px            | The density default — a row in a list, a control's own text.                   |
| `--text-body`  | 13px            | The window's base: sentences addressed to the reader.                          |
| `--text-title` | 15px            | The one line that says what a panel is about. At most one per panel.           |

**Three weights, and the third is reserved.** `--font-weight-answer` (600) is how a capability row
says it is the half of the answer the agent advertised, and how a Session Settings row says which
value is current ([§9](../architecture.md#9-screens)). Weight can only carry an answer while it is
not also how a label is drawn, so the uppercase micro-labels — which took 600 throughout the
hand-written sheet — take `--font-weight-label` (500) instead: at 10px with tracking, 500 is legible
without spending the channel. Everything else is `--font-weight-normal`.

**One tracking value.** `--tracking-label` (0.1em) applies to uppercase labels and to nothing else.
The hand-written sheet had five, which is five decisions where there is one.

## The historical spacing rhythm

Six steps on a 2px base, and the rule is which relationship the gap is between:

| Token               | Value | Between                                                                  |
| ------------------- | ----- | ------------------------------------------------------------------------ |
| `--spacing-row`     | 2px   | The vertical padding inside one row of a dense run.                      |
| `--spacing-tight`   | 4px   | The parts of one row — a mark, its word, its annotation.                 |
| `--spacing-snug`    | 6px   | A control and the sentence that explains it; two rows of different kinds. |
| `--spacing-group`   | 8px   | One named group and the next inside a panel.                             |
| `--spacing-gutter`  | 12px  | The panel's own inner edge: the one indent everything lines up on.       |
| `--spacing-block`   | 16px  | A panel's content and something unrelated to it; an empty state's room.  |

`--spacing-row` is the smallest step because it is the one that keeps the rows-per-screen a dev tool
exists to give. The 12px gutter keeps content clear of panel edges without making a constrained
developer tool feel padded out.

Sizes are not rhythm. A 7px dot or a 44px bar is a dimension of a thing rather than a distance
between two, and it is written as an explicit value — which a review can see — rather than by adding
a step to a scale that would then mean two things.

## The historical radius scale

One step per level of containment, so the three ad-hoc radii become a scale that relates: a card's
corner is one step above the control inside it, and the smallest painted box is one step below that.

| Token               | Value | On                                                          |
| ------------------- | ----- | ------------------------------------------------------------ |
| `--radius-mark`     | 3px   | The smallest painted box: a count, an inline `code` chip.   |
| `--radius-control`  | 6px   | Anything you press or type in.                              |
| `--radius-card`     | 9px   | A box that contains controls: a tool card, a login notice.  |
| `--radius-pill`     | full  | A shape that says *this is a state, not something to press*. |

`--radius-pill` is not a fourth step but a different statement, which is why it is allowed to sit
outside the ladder.

## Elevation

**One step, `--shadow-lift`, and it is for the one thing that overlaps content.** The window is flat
on purpose: what separates a panel from its neighbour is a ground and a hairline, and a dev tool
that shadows its own rows has less signal left for the stripes that mean something. The five
box-shadows the hand-written sheet carries are those stripes — direction and lane markers, which are
signal rather than elevation — and they stay hand-written, because their colour is what they say.

The step is mixed from `--ink` rather than declared as a literal, so one declaration is right in
both schemes and the every-colour-is-a-token rule stays true of a shadow.

## Motion

**Interactive state only**: hover, focus, press, disabled, expand, collapse. **Nothing animates
arriving content** — not a frame landing in the trace, not a chunk appending to the timeline, not a
stderr line. That rule is [§9](../architecture.md#9-screens)'s and it is repeated here because a
fade is the thing a visual pass adds back on the grounds that it looks nice; the reasons are that
rendering behaviour over a fast-appending log is unmeasured
([§15 q8](../architecture.md#15-open-questions--validation-gaps)), that an animation per arrival is
work per arrival, and that what the eye reads as the frame arriving is the transition finishing,
which is a lie about timing.

The original vocabulary used two durations, because a thing that responds to the pointer and a thing that opens are not the same
event: `--motion-quick` (90ms) is a control acknowledging you, and `--motion-reveal` (140ms) is a
disclosure that has somewhere to travel. `--ease-standard` decelerates — movement that starts at
once and settles.

Those exact durations and global defaults are no longer required; daisyUI may own ordinary control
timing. The behavioral constraints below remain current.

**Only properties that do not move the layout**: colour, background, border, outline, opacity,
transform. A height or a margin that transitions on a list which is being appended to is the
arriving-content rule broken from the other end.

And the rule that is a rule rather than a token: **a reader who asked for less motion gets none**.
`prefers-reduced-motion: reduce` switches every transition and animation off, in
[`input.css`](../../desktop/style/input.css), because none of this motion carries information and
nobody who turned it off at the operating system should have to argue with it. It is `app/`'s rule,
copied for the same reason.

## The colour names are indirected

`--color-panel: var(--panel)` and so on for all 27 tokens: the shape `app/`'s stylesheet takes,
translated rather than imported. Its names are a chat client's (`canvas`, `surface`, `measure`) and
these are this window's, argued where they are declared. The indirection is what lets `bg-panel` and
`text-muted` exist at all — a Tailwind theme variable cannot itself be conditional, and this
window's tokens switch on `prefers-color-scheme` and on `data-theme`.

`bg-bg` is the one name that reads badly, and it is kept anyway: `--bg` is the token this window has
always called the canvas, and inventing a second word for it here would mean the stylesheet and the
markup disagree about what the ground is called.

**No colour value moved in that ring.** The five colours' meanings and their hues were exactly what
[ADR 0003](0003-the-stylesheet-is-generated-and-committed.md) records, dark's known AA shortfall
included: that deferral expires with the render it was owed to, which is the sweep and not the
vocabulary. (It did. The sweep raised eight of dark's values and two of light's, and moved no hue —
ADR 0003's palette record has every ratio.)

## What this ring does not do

**No module is swept.** The rules in `input.css` are the hand-written ones, still carrying their own
values, and they will keep carrying them until the ring that rewrites them. A vocabulary that
arrives with eleven modules already rewritten is a vocabulary nobody had the chance to say *not
that* to, which is the whole reason this ring is separate
([#97](https://github.com/sagikazarmark/dioxus-chat.orig/issues/97) user story 17).

**Layout is untouched**, here as in every ring of the redesign.

## What the sweep settled

The ring after this one applied the vocabulary to every screen
([#106](https://github.com/sagikazarmark/dioxus-chat.orig/issues/106)). It added no token and moved
no step; what it did was answer four questions this document left to whoever met them first, and
they are recorded here because the next person to ask will look here rather than in a diff.

- **At the time, every uppercase label became `--text-micro`**, including the ones that were 11px: a form field's
  label, a panel header, the Console's tabs. Eight of them shared one rule after the sweep, at one
  size, one weight and one tracking, and what still differs between them is the colour. "The
  uppercase label a group wears" turned out to be one thing worn in eight places rather than two
  sizes of thing.
- **A hint beside such a label is `--text-label`** — one step *larger* than the label it annotates,
  which looks wrong written down and reads right on screen, because the label is tracked and
  capitalised and reads larger than it measures.
- **At the time, `--font-weight-answer` was spent in exactly two rules**: the advertised half of a capability row,
  and the value a session is set to. Launch and Send gave up the 600 they were carrying — a primary
  button says it is primary with the accent it is filled in — and a Session Settings row's *name*
  took `--font-weight-label`, because a name is what a row is asked, not what it answered. A test in
  `crates/app/src/style.rs` held the pair. ADR 0005 removed that global reservation while retaining the
  requirement that advertised and current facts remain readable without dimness alone.
- **`--motion-reveal` has one use**, and disclosure turned out to be the harder half of the motion
  rule rather than the easier one: what a `<details>` opens cannot be transitioned without animating
  a height, and a height that eases on a list being appended to is the arriving-content rule broken
  from the other end. So the *marker* travels — a rotation, which is a transform — and what it
  reveals appears at once.

## Consequences

- **The source stylesheet is two files**, split along the line this document is about: `theme.css`
  declares names and `input.css` declares behaviour. A sweep that has to know what the vocabulary
  *is* now reads one short file instead of scrolling past a thousand lines of rules to find it, and
  the rules are read without the palette in the way. The split also lets anything else be built
  against the same tokens rather than against a copy of them — which is what the ring's two mocks
  were, and what any later prototype will be — but it would be worth making without them.
  `crates/app/src/style.rs` reads both files as one source, because every rule it asserts is about the
  source as a whole.
- **`style.rs` is excluded from Tailwind's scan.** It is the one Rust file that carries no markup and
  talks *about* class names, and a scanner cannot tell a class somebody wrote from a class somebody
  wrote about — the prose in this document's neighbour generated a `.p-group` rule before the
  exclusion was added.
- **The retained vocabulary is enforced where it applies.** `style.rs` asserts that every colour
  name resolves to a token, that the block is `static`, that daisyUI owns ordinary components, and
  that semantic and reduced-motion rules remain. Exact component scales are no longer enforced.
- **Current component values come from daisyUI.** An inspector-specific arbitrary value still owes
  a layout, evidence, accessibility or behavior reason, but ordinary components use standard
  classes rather than a separately enforced numeric scale.
