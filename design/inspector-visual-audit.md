# Inspector visual audit

> **This records the fifth ring's pass and predates the drawing**
> ([ADR 0009](../docs/adr/0009-the-window-is-the-drawing.md)).
> The five problems below were found and fixed; where the regions *are* has
> since changed, so the baseline and revised captures show an arrangement the
> window no longer has. What replaces them is [`redesign/`](#redesign) below and
> the render harness that writes it (`crates/app/src/mock.rs`), which draws the
> same screens from fixtures rather than from a hand-driven agent.

Screenshots were captured from the Linux desktop WebView at the supported
1440 x 880 and 960 x 640 window sizes.

## Baseline

- [Disconnected, 1440 x 880, light](baseline/disconnected-1440-light.png)
- [Populated, 1440 x 880, light](baseline/populated-1440-light.png)
- [Populated, 960 x 640, light](baseline/populated-960-light.png)

The baseline exposed five recurring problems:

1. Most semantic aliases were mixed so strongly toward the base text color that
   primary, live, incoming, outgoing, and destructive states all read as shades
   of charcoal.
2. The toolbar, panel headers, canvas, composer, and rail had nearly identical
   grounds. Boundaries existed, but the application did not have the layered
   chrome expected of a native inspector.
3. A global `.method` rule intended for authentication also matched Session
   Settings method badges. It made each badge a full-width flex row and pushed
   setting labels and controls out of alignment.
4. Speech entries rendered kind then timestamp, while structured entries
   rendered timestamp then kind. Their content columns therefore moved between
   adjacent Timeline rows.
5. Related controls were separated by prose: `session/new` and `session/list`
   appeared in different groups, and the launch form presented four fields as
   one undifferentiated stack.

## Revised

- [Disconnected, 1440 x 880, light](after/disconnected-1440-light.png)
- [Populated, 1440 x 880, light](after/populated-1440-light.png)
- [Timeline, 1440 x 880, light](after/timeline-1440-light.png)
- [Timeline, 1440 x 880, dark](after/timeline-1440-dark.png)
- [Timeline, 960 x 640, light](after/timeline-960-light.png)
- [Timeline, 960 x 640, dark](after/timeline-960-dark.png)

The revision strengthens semantic color while retaining the existing contrast
floors, gives toolbar/header/composer chrome a shared native-style surface,
and uses a quiet blue selection ground for current Session Settings values.
Launch fields are split into Invocation and Process groups, Session actions are
adjacent, authentication styles are scoped to authentication, and every
Timeline row now shares stable kind, timestamp, and content columns.

The refreshed captures also include the right Session Settings rail, concise
Timeline states, semantic composer key hints, a complete copyable Session ID,
and raw-evidence actions that remain available beside closed disclosures.

## The appearance pass

The layout above is unchanged. The look is not, and the pass took two runs at it
because the first one answered the wrong question.

**The first run asked what a native desktop tool looks like** and got a correct,
joyless answer: system greys, flat chrome, a boundary at the text colour's
weight around everything. Every distinction on the screen was a value of grey,
including the ones that were supposed to be carrying meaning, and the result was
a window that looked like a screenshot of a preference pane. Kept from it:
surface layering, control bezels, capsule badges, the segmented control, the
focus ring, the pane-title size, and a control boundary at three fifths rather
than four fifths of the way to the text colour.

**The second run asked what makes an application look considered**, and the
answer was that colour has to be doing something:

1. **The five meaning-bearing aliases were mixed half-and-half with the text
   colour.** That is what a value has to be if it will only ever be read as
   text, and the cost was that direction, liveness and trouble all arrived as
   shades of near-black with a hint of hue. They are 78% of the way to their
   role now — the largest share the tightest of them clears 4.5:1 with, on the
   tightest ground — and boundaries, which owe 3:1, go to 72%. Both ratios are
   recomputed by
   `style::tests::inspector_light_and_dark_clear_the_inspector_contrast_floors`
   from the same roles, so a number chosen for how it looks fails the build.
2. **The accents were desaturated to be safe.** They clear the same floors at
   more than twice the chroma; the floor was only ever a constraint on
   lightness. What is left over — the hue and the chroma — is a decision, and
   was being spent on nothing.
3. **Every badge said its meaning in text colour alone**, inside a rounded
   rectangle that matched the controls around it. They are `badge-soft` now:
   one `--badge-color` per tone mixed into a tint, a hairline and the word, so
   an entry kind, a tool state and a turn state read as what they are at a
   glance and still say it in words for anyone who is not reading the hue.
4. **A diff said which side was which in ink only.** Each side now carries the
   allow or reject ground — the same two hues, mixed for the same reason — while
   keeping `del`/`ins`, their screen-reader labels and the strike-through.
5. **Nothing was ever lifted.** The Timeline's evidence boxes — a tool call, a
   permission request — are objects the Agent produced, quoted whole inside the
   row that reports them, and they now sit on the surface with a hairline and
   `--shadow-card` instead of being cut into it. They are the only thing that
   takes it; a test holds that.
6. **Corners were the platform's.** 8px on anything pressed or typed in and 12px
   on a box that holds them, which is rounder than a desktop control and is what
   the compact geometry was missing rather than more room.

Selection and hover moved onto the accent too: the current segment of a Mode
setter is the accent's colour, and the row under the pointer in a dense log is
the accent's tint rather than the next grey along.

### Three things that were broken rather than plain

Found by driving the surfaces rather than by looking at them, and all three had
been there for rings — the pass made two of them visible by putting colour where
grey had been hiding them.

1. **An expanded Frame highlighted its whole height.** `.frame-row` is a grid of
   a `<details>` and an actions column, and the actions column was stretched to
   the grid row — which, once a frame was open, is the whole of the raw JSON. So
   hovering an open frame painted the hover band, and marked a revealed frame,
   all the way down the side of the evidence. The band is the summary line now,
   the reveal mark belongs to the row, and what the disclosure opens is a block
   inset from the row rather than a slab bled to an edge it could not reach.
2. **Every launch field label overlapped its field by two pixels.** daisyUI
   pulls a `fieldset-legend` down by a quarter rem so it sits *on* the border of
   the fieldset it names; on a `span` inside a `label`, with no border to sit
   on, it just eats into the field. All four launch fields, both group headings
   and the roots field were doing it.
3. **A prompt longer than two lines ran through its own footer.** The keyboard
   hint and the Affordance were floated over the field's bottom padding on the
   premise that the padding was a reserve — and a textarea scrolls its bottom
   padding along with its content, so the third line went straight through both.
   An opaque band over them was tried and only moved the problem: then the
   *caret* was what went behind. The field is one bounded box now with the
   footer in flow inside it, which is also two fewer rules positioned out of
   flow — `nothing_taken_out_of_flow_is_positioned_against_the_window` is down
   to a single entry.

A `getBoundingClientRect` sweep for text overlapping a field, run over both
surfaces at both supported window sizes, now reports none.

**These captures were not retaken.** Every run was iterated against the
generated stylesheet in a headless browser; the images above are the previous
ring's, from the Linux desktop WebView, and are stale for everything in this
section. Retaking them needs a display and a running
`cargo run -p acp-inspector`.

## The application pass

The layout is unchanged again, and so is the palette's argument. What this ring
asked is narrower than either: **what is still being drawn by the engine rather
than by this window** — because those are the parts that go on looking like a
web page however considered everything around them is.

Six of them, and the first is the one you see from across the room:

1. **The scrollbars were the browser's.** A wide light trough and a boxy thumb,
   in a palette that does not change when the window goes dark — six of them,
   one per scroller, down the middle of a tool that had decided every other
   colour on the screen. They are a thumb and no trough now, drawn in
   `--control`: the same value every boundary a reader can press or drag
   already takes, which is also what keeps the thumb above the floor a graphical
   control owes. Both spellings, because the two engines this shell runs on read
   different ones, and the same two values in each.
2. **Selected text was the engine's blue**, drawn at full strength over evidence
   somebody was in the middle of reading. It is the accent's tint now — the
   colour everything the *reader* did is already in — and `--selection` went into
   the same grounds array as the three surfaces and the hover tint, so 18% is
   the number the floor picked rather than the one that looked right. The caret
   and the placeholder went the same way: the accent, and `--faint` at the
   strength the token means.
3. **Dragging across the window selected its labels.** A panel title, an
   uppercase group heading, a count, an entry kind, a key cap — chrome, and the
   most reliable tell that a desktop window is a page. Chrome declines selection
   now and evidence does not, and the Console's header, which had already
   declined it on its own, states the general rule instead of an instance of it.
   **No `.btn` is in the set**, and a test holds that: a button's label here is
   often the agent's — a permission option, a mode, an auth method, a remembered
   invocation — and a rule that took `.btn` would be this window deciding
   something the agent wrote cannot be copied.
4. **A scroller that ran out handed the rest of the gesture to the region
   behind it.** A wheel run off the end of the Trace carried on into the
   Timeline. Every region here is a screen rather than a panel inside one, so
   every one of them stops — `overscroll-behavior: contain`, which is the rule
   the root already stated about the window saying the same thing one level in.
5. **Two screens were a sentence in the top left of an empty canvas.** A
   Timeline with no session in it and a Trace with nothing on the wire are the
   whole of a region, and a paragraph set hard against its corner reads as a
   screen that has not finished drawing. Both are centred in the room they have
   with `text-wrap: balance`, so the two lines are even rather than a heading
   with a word and a half under it. The third empty is untouched: it is a
   paragraph in a 248px rail, where centring is a ragged column and nothing
   else.
6. **The counts were set in proportional figures**, so 9 → 10 stepped the tab
   strip they sit in. One figure width, on the badge — the timestamps and
   ordinals elsewhere are `--font-mono` and were already doing it.

### And one that was broken rather than plain

**`session/new` and `session/list` overflowed the rail by two pixels.** The pair
is a two-column grid at `1fr 1fr`, and a grid track's implicit minimum is its
content: at their natural widths the two buttons came to more than the rail has.
The rail is a scroll container in one axis, which makes it one in both, so two
pixels of overflow is a horizontal scrollbar under the whole left column —
invisible until the scrollbars stopped being the engine's and started being
drawn in a colour somebody chose. `minmax(0, 1fr)` is the same track with the
implicit minimum said out loud, and it is how every other flexible track in this
sheet is already spelled.

A `getBoundingClientRect` sweep for horizontal overflow, text over a field, and
content outside the window, run over the populated and disconnected surfaces in
both schemes at both supported window sizes, reports none.

**These captures were not retaken either.** This ring was iterated against the
generated stylesheet in a headless browser over server-rendered markup, the way
the previous one was; the images above are two rings stale. Retaking them needs
a display and a running `cargo run -p acp-inspector`.

## Five things review found and this pass fixed

Reported by looking at the window rather than by driving it, which is the half
the sweeps had been missing: three of these are a value that was right in one
place being wrong in another, and two are a box the wrong size for what goes
in it.

1. **The composer drew its focus ring twice.** The box carries the ring and the
   field inside it was supposed to give it up — and the override lost. The
   shared focus rule is an `:is()` whose most specific arm is `.recent .recall`,
   so the whole list weighs two classes and the rule weighs three; the override
   was written as an element and one class, which is two. What it painted was a
   3px band around the *textarea*, and the bottom edge of that band is the line
   that appeared above the send button every time the composer was typed in.
   Named through `.field` now, which is one class ahead of the rule it has to
   beat.
2. **And the ring it does draw is two pixels rather than three.** This is the
   one control on the window with a size of its own: the shared 3px is for a
   button, a 60px target the eye has to find on a busy rail, and the composer is
   the widest box here. The same band around 880 pixels of edge is several times
   the ink, and it read as the window shouting rather than as the keyboard
   arriving.
3. **Every notice in the Session Settings rail was invisible.** The compact
   ground and hairline a bounded notice takes were scoped to `.panel`, and the
   rail is a `<section class="settings">` — so every alert in it kept daisyUI's
   `base-200`, which is the rail's own ground, including the one at its foot
   that says the view is decoded. A grey box on a grey rail is a box with no
   edges. Both containers are named now, and the test asserts the fill differs
   from *each* of them rather than from the left rail alone.
4. **`unrecognized` wrapped inside its own capsule.** The kind column was
   5.75rem, which is ninety-two pixels for a badge that wants ninety-four — so
   the longest kind in the set, on the row §8 says a reader is looking hardest
   at, broke across two lines and made that row taller than every other row on
   the surface. The set is closed and this window wrote all eight of it, so the
   column is 7rem: sized to the longest of them once, rather than to whichever
   one is on screen.
5. **The permission request was recessed onto its own quote.** `bg-base-200` is
   the fill every evidence box in the Timeline carries, so a request was a grey
   card on the white canvas holding a grey card of the tool call it is about:
   two boxes, one value, and no boundary between them a reader could see.

### The request, redrawn

It is the one card in a turn that is *waiting on the reader*, so it is the one
that is lifted rather than quoted. Four changes and no new colours:

- **The card takes the document's surface and the quote inside it keeps the
  panel's.** `--bg` is the lighter of the two in light *and* in dark, so the
  thing being asked about is inset into the thing doing the asking in both
  schemes. The stripe goes to three pixels with it, because two was what it took
  when the fill was already different from the canvas.
- **The question and the id it will be answered under share a line**, the id at
  the far end in the label register — the same place and weight the Timeline's
  header gives a Session id. And *the turn is stopped here until you answer*
  reads at the weight of an answer: the three other states this slot carries
  report something that already happened, and this one is an instruction.
- **Four choices fit across.** The grid was `minmax(11rem, 1fr)`, wider than a
  quarter of the Timeline at the window's own size, so the commonest request in
  ACP — `allow_once`, `allow_always`, `reject_once`, `reject_always` — wrapped
  three-and-one and left a hole where the fourth button should have been. 9rem
  fits four at 1440 and drops to two at the 960 floor.
- **Two lines per choice, and the words in ink.** The id and the kind moved into
  one element under the name: they are one fact about the option rather than
  two, and stacked as siblings they made every button as tall as the tool call
  above them. The separator between them trails the line it is leaving rather
  than orphaning onto the next. And the tone left the label — a tonal
  `btn-outline` paints the whole word in its role colour, which put four
  buttons' worth of saturated green and red text on the one card that has to be
  read carefully. **Allowing and rejecting are the ground and the boundary
  now**, which is the decision the diff already made for the same two hues and
  the same reason: what a reader is choosing between is *this goes* and *this
  does not*. The tint is a ground text is read on rather than a hover state, so
  `style.rs` holds ink, muted and faint to 4.5:1 on both of them.

The sweep was re-run over the populated, disconnected and focused surfaces in
both schemes at both supported window sizes: no horizontal overflow, and nothing
laid out over a field.

## Two more that were broken rather than plain

Found by driving the states the earlier captures never showed — a lifecycle call
the agent refused, and a tool call that failed.

1. **A refused lifecycle call was the one notice on the window nobody could
   read.** daisyUI lays an alert out as a grid of an icon column and a content
   column, and the Session group's failure notice is a `<code>` and some words —
   so the method went into the *icon* column, a 40px track in a 248px rail, and
   `session/delete` broke across two lines in the middle of the word. It is the
   same fix Session Settings already needed for the same reason and the same
   sentence about it: a notice that is a sentence is laid out as one. Worst on
   the notice that matters most — a call the agent refused after advertising it
   is the finding a reader came for.
2. **A tool call's output lost its line breaks.** An `execute` tool answers with
   the output of a command and a `read` one with the lines of a file, and both
   are text whose shape *is* the content: a compiler diagnostic arrived as four
   lines of caret-and-pipe alignment and rendered as one paragraph with the
   pipes run together mid-sentence. That is the information loss the raw-first
   rule exists to prevent (§8), one layer above where the rule is stated. The
   `pre-wrap` a message the agent wrote already gets, and no further — it wraps
   rather than scrolls, so a long line stays inside the Timeline rather than
   widening it.

## Trouble, at the weight everything else is drawn at

Two states nobody had looked at, and one contradiction between the Console's own
two tabs.

**daisyUI fills an `alert-error` with the error role and writes on it in
white.** In a 248px rail that is a solid saturated rectangle, and it was the
loudest thing on the window by a distance — on a window whose entire palette
argument is that greys separate regions and hues say things, the one notice
drawn as a *block* of hue was the one saying the least with it. Both trouble
notices take a tint, a boundary in the role, and the words in the colour they
already were: `--bad-tint` and `--warn-tint`, the role at 12% on the document
surface. That is what every other statement of trouble here already looks like —
the capability row that says an agent refused, the setting whose set came back
an error, the reject side of a diff. Nothing is quieter about *what happened*:
the notice keeps `role="alert"` and its own words. What changed is that it is
drawn like the rest of the window.

The text on these two grounds is `--bad` and `--warn` themselves, which are the
tightest colours in the palette, so `style.rs` measures each on its own tint at
4.5:1 and as a boundary on every surface at 3:1. 12% is a tint rather than a
fill — enough that the notice is bounded by a ground as well as by a line, and
not so much that the rail has a coloured block in it again.

**And the Console was contradicting itself.** Its two evidence surfaces sit in
the same panel behind the same two tabs, and the Trace's per-row copy control is
quiet until the row is under the pointer or the control is focused — because a
mark on ten thousand rows is a texture rather than an affordance. The Diagnostic
channel's identical control was visible on every line, which is three copies of
the word *copy* stacked down the right edge of a three-line log. It follows the
rule its neighbour already states, hover *and* focus, so a keyboard still
reaches it.

**Left alone deliberately:** the unrecognized entry's sentence is a full line of
saturated violet, and it stays one. It is the loudest text in the Timeline and
that is the point — §9 spends exactly one colour on traffic nobody has a name
for, because that is what the reader came to find.

## The left rail, restructured

The one region nobody had looked at as a *whole*. Every pass above worked on
something that fits on a screen — a row, a card, a notice, a scrollbar — and the
Agent rail is six screens long, so what it needed was not another treatment but
an answer to why six screens of it read as chaotic.

Three answers, and the first two are the same answer twice: **the rail was
sized for the smallest supported window, and then everything in it was drawn as
though it had that width to spend.**

1. **248px was the size rather than the floor.** It is the number the *window*
   is floored at, and it had been spent at every window size — on the one rail
   whose content is the longest strings this tool ever draws: an invocation, a
   session id, a capability name beside the field it was claimed in, an agent's
   own refusal quoted whole. Measured, the content column is 224px. That is
   eight pixels short of two lifecycle buttons on one line, and about a syllable
   short of a field annotation on the line of the name it annotates. Both rails
   carry the same clamp now — 248px at 960, 288px from the default size up —
   which is what the details rail had said since it was built, and the Timeline
   is still the track that gives way last.
2. **`.caps .field` was capped at 45% of that.** A fraction of a narrow rail is
   a column too small for what goes in it, so `loadSession` came out as *load /
   Sess / ion*: three lines of one word, on every row of the session group at
   once, which is also what made those rows tall enough to read as holes in the
   list. The name's track already has the slack, so the cap was reserving room
   the name had not asked for and then breaking the annotation to fit inside
   what was left.
3. **And the row was three lines with sixteen pixels between each.** The state
   and the fourth fact were a grid row each, and daisyUI's `list-row` carries a
   1rem gap — correct for a row of a photo, a name and a control, and twice over
   the wrong thing between a capability's name and what the row says about it.
   The two facts are read together and are one line now, kept apart by a mark
   the accessibility tree does not get and by the leading spaces it already had.
   Nineteen rows across the two accounts: it is about a third off the longest
   block on the rail, and it is the change that makes the list scannable rather
   than merely shorter.

### And three that were untidy rather than broken

- **The rail had three ideas of what a section heading is.** Launch stated
  `Invocation` and `Process` in sentence case at the answer weight, Recent
  stated its own in a third register, and the Agent panel's five claim blocks
  stated theirs in the tracked uppercase micro-label. Scrolling the column
  crossed all three. They are one role now, named by one selector so that a rule
  given to one is given to the rest, and the panel titles above them are the
  register it deliberately does not take. The rule that used to run out of both
  sides of `Process` came out with it: a `<legend>` sits *on* its fieldset's
  border, and an uppercase heading dropped onto a hairline reads as a divider
  somebody landed a word on. The heading is the division.
- **What this window says about the traffic looked like the traffic.** The
  Agent rail carries a great deal of the inspector's own commentary and every
  word of it is owed — a call's consequences said once above the rows, the shape
  a run of claims was made in, why nothing here can drive one — but it was drawn
  as paragraphs in a column of paragraphs, so the Session group reached the
  reader as five statements of equal standing between the two buttons and the
  sessions they act on. A hairline down the near edge is the whole of the
  treatment: it costs a pixel, and it lets a reader who has read the sentence
  once skip the block without having to read it again to find out that they can.
  Consecutive notes give up the gap between them, so four sentences about one
  group of controls are one bar rather than four facts. Nothing was cut, capped
  or folded — the rail is still the region with no collapse, for the reason it
  always was.
- **Every control on a listed session is bounded now.** Reopen and close were
  `ghost` — a label and no boundary — beside a `delete` that carries the outline
  its tone comes with, so a row of three calls read as one control with two
  headings over it, and the control it read as was the destructive one. They are
  one group of push buttons, and the tone is the only thing that still tells them
  apart, which is the difference there actually is. `logout` and the listing's
  `Next page` took the same answer for the same reason.

**Two smaller things fell out of the reorder.** What `session/new` costs was
drawn *below* the roots control, so the consequence of pressing a button read as
a note about a field; it is under the buttons it is about. And the rule for what
goes in a launch field moved onto the line that names the field, where a
platform puts a unit or a format — which is also two lines back on the two
fields that already take the most room in the form.

A `getBoundingClientRect` sweep for horizontal overflow, text laid over a field,
and content outside the window was run over the populated and disconnected rail
in both schemes at both supported window sizes: none. The rail's scroll height
for a fully-capable agent is down about a fifth at the window's own size, and
none of it was bought by taking a fact off the screen.

**These captures were not retaken either.** This pass was iterated against the
generated stylesheet in a headless browser over server-rendered markup, the way
the previous two were; the images at the top of this file are now three rings
stale. Retaking them needs a display and a running `cargo run -p acp-inspector`.

## The right rail, in the same terms

The Session Settings rail is a quarter the length of the Agent rail and was in
much better shape, so this is a shorter list — and two of the three are the same
findings as the pass above, which is the point of writing them down together.

1. **Every row spent a line restating one fact about the inspector.** §9 asks
   that which method a control drives be readable without going to the trace,
   and it was: `session/set_config_option`, on a line of its own, in the gap
   between each setting's name and the thing that changes it — the same
   twenty-five characters under every config option an agent publishes, four
   times over on a surface of four rows. It sits at the far end of the line that
   already says what the setting is set to, which is directly above the control
   it names. That is where the other rail puts the field a claim was made in,
   for the same reason, and it costs no line at all.
2. **The space between two rows did not beat the space inside one.** A row's own
   lines are a `--spacing-tight` apart and so was its padding to the hairline, so
   an agent's description of a setting sat as close to the *next* setting's name
   as to the control it belongs to. There are a handful of these rows rather than
   a log of them, so they can afford the step that makes the grouping legible.
3. **The window's one sentence about itself was drawn as the agent's answer.**
   *Decoded view. Complete frames remain in Trace.* is the only notice on that
   rail which reports nothing an agent did — and it took the same bounded-notice
   chrome as the ones that do, immediately under them. On a session whose agent
   published nothing, that was two identical cards stacked at the top of an
   otherwise empty rail, one of them the finding and one of them a footnote,
   with nothing saying which. It is a rule and the caption register now, and
   `margin-top: auto` puts it where a footnote goes: the foot of the rail when
   the rows do not fill it, and under the last of them when they do.

The same sweep — horizontal overflow, text laid over a field, content outside
the window — was run over the populated, refused and empty surfaces in both
schemes at both supported window sizes: none.

### And three more the states nobody had rendered were hiding

The pass above looked at the rail as it usually is. These came from driving the
states it usually is not: an agent that has gone, a set in flight, and an agent
whose values are sentences rather than words.

1. **The Mode setter was the one control on the window that looked live when it
   was not.** The settings outlive the agent that published them on purpose
   (§5), so the rail goes on describing a session whose agent has left — and
   daisyUI draws an unavailable `.btn` by taking its ink down to a fifth, which
   is what every select and every Set action here does the moment that happens.
   The segments name their own colours, so they beat that rule and went on
   drawing the current mode in the accent beside three controls that had visibly
   given up. They take the ink down now, in the two roles that clear the floors
   on the grounds a segment is drawn on, and the raised face still says which
   value is current. **Not while the row is busy**: a set in flight says so in
   words under the row, and dimming the set somebody has just asked for would
   report their own ask back to them as the agent's absence.
2. **And nothing said why.** A row of controls nobody can press was the whole of
   what the surface said about a connection that had ended. The Agent rail has
   answered the same question with one sentence since the ring that built it;
   this is that sentence, in that place, and drawn only where there is a control
   it is about — a session whose agent published nothing has nothing here that a
   connection would make pressable, and the sentence would be describing an
   absence that is the agent's rather than the connection's.
3. **A value and the call that applies it are one control.** The set is a
   separate action on purpose — a select that wrote on change would send a
   `session/set_config_option` for every value the keyboard walked past on its
   way to the one somebody wanted — but two rounded boxes a gap apart say *two
   controls*, and the one thing this pair is short of is room for the value.
   `join` is how the segmented setter three lines above already says they are
   one, so the gap goes back to the side that needs it and daisyUI keeps the
   geometry (ADR 0005).

**One rough edge is left, and it is named rather than fixed.** An option whose
label is longer than the field — `Workspace write (workspace-write)` at the
rail's width — is drawn through the picker icon at the end of it rather than
ellipsised, because daisyUI opts into `appearance: base-select` where the engine
has it and the selected value then renders into a slot the field's own
`text-overflow` does not reach. It is a Chromium-family behaviour, so it is the
Windows WebView and not the WebKit ones; a `::selectedcontent` clamp was written
for it, measured against the engine, found to change nothing, and taken out
rather than committed as a rule that looks like a fix. Nothing is lost while it
stands: the value the agent stated is on the line above, in full, as the id it
stated it as.

The sweep was re-run over the populated, refused, disconnected and in-flight
surfaces in both schemes at both supported window sizes: no horizontal overflow,
nothing laid out over a field, nothing outside the window.

## The final pass

Run from `mock.rs` rather than from a WebView: `INSPECTOR_MOCK=… cargo test -p
acp-inspector mock::writes` renders every screen in both schemes from
fixtures, and the pages were screenshotted at 1440 x 880 and 960 x 640. What it
cannot show is what only an engine knows — real fonts, real scrollbars, a
`<details>` that opens — so the WebView remains where a behaviour is checked.
It is where a *layout* is checked, and none of the six findings below needed a
window to see once the screens were drawn honestly.

1. **The Timeline drew the clock over what the agent said, at one of the two
   supported window sizes.** The breakpoint that gives a narrow row's second
   line to the content is correct, was correct when it was written, and never
   applied: a media query adds no specificity, and the block sat above the
   `.entry .body` rule it overrides, so it lost the tie. The clock moved into
   column two and the content stayed in column two. `style.rs` asserted that the
   rearrangement was *written* — which it was — so the sheet's own test passed
   over a row a reader could not read. The block moved below the rules it
   overrides and the test asserts the order now, which is the assertion the
   first one was standing in for.
2. **Two controls said Stop, in the same red, with the same icon, on screen
   together.** One cancels a Turn; the other ends the Connection and the Agent
   with it. The toolbar's is **Disconnect** now — the Connection's own word, and
   the other half of the `Connect…` beside it. The composer keeps Stop, because
   it stops the thing the reader started and is watching.
3. **`logout` was a button alone in an empty row**, with the sentence saying
   what it costs marked `sr-only`. Right on a 288px rail, where the button is
   all that fits; on the centre screen it is a control with the width of the
   window to its left and nothing in it naming the call. It is a row of the same
   shape as the logins above it now, and the sentence is drawn.
4. **`6 of 13` was a count of nothing in particular** unless you were using a
   screen reader, which had the word `advertised` and the heading the sighted
   reader did not — both are `sr-only`. The word is drawn.
5. **One act had three names**: `Connect…` in the bar, `Connect to an agent` on
   the dialog, `Launch an agent` on the empty Timeline — the last of which opens
   the form rather than launching anything, as its own comment said. It is
   `Connect…`, and `Launch` is left to the control inside the form that does it.
6. **Prose in the auth block was set at about 180 characters a line.** The row
   gives its first column everything left over, which is right for a name and an
   id and wrong for two sentences. The sentences take a measure; the row does
   not change.

**And the first-run page was not the first-run page.** It was rendered from the
populated fixtures with only the toolbar and the claims told there was no agent,
so the screen it exists to show arrived as a full Timeline, a live Session's
Settings and thirty-six Frames under a bar reading *Disconnected*. Its own
comment called it the one screen nobody had looked at, and rendering it that way
is how it stayed that way. It draws what the window draws now — no entries, no
frames, no diagnostics, and Session Settings mounted on the window's own
condition, which is a live Session to have any. Findings 4 and 5 are both things
that page shows and the old one could not.

## The release pass

Every pass above asked what was *wrong* with something on the screen. This one
asked the question a reader asks in the first second, before they have read a
word: **what is this window, and where does it start.** It is the last pass
before release, and the four findings are the four answers the window was giving
badly — three of them ideas this project had already argued, written down, and
then not delivered.

1. **The thesis was invisible.** The regions are cards inset into a chrome
   ground, and every desktop tool worth copying draws that arrangement rather
   than a page divided by hairlines — it is argued in ADR 0006 and asserted by
   `every_region_is_an_inset_card_on_the_chrome_ground`. It was also, on screen,
   three and a half percent of lightness across an eight-pixel gutter: a
   near-white ground behind near-white cards, which reads as a thick hairline
   drawn in grey. The idea had been reasoned about and tested and never seen.
   The ground is four and a half percent off the document now, with the chroma
   taken up so it is a cool grey rather than a dimmer white, and the inset is
   `--spacing-gutter` — the step the panels already hold between their own edge
   and their content, so the window sets its regions into the ground on the
   measure everything inside them is lined up on.

   **The palette moved to pay for it, and the floors picked the numbers.** A
   deeper `base-300` took the accent's tightest pairing under 4.5:1, so the blue
   is three points darker and the amber and green two — the same hues at the
   same chroma, because the floor was only ever a constraint on lightness (ADR
   0004). `inspector_light_and_dark_clear_the_inspector_contrast_floors`
   recomputes every one of them from the roles, so this is the largest step this
   palette carries rather than the one that looked right.

2. **The Timeline was a spreadsheet.** A full-width hairline under every row —
   two hundred of them in a long session — separating rows that a kind capsule,
   their own leading and the ground that arrives under the pointer already tell
   apart. What a reader of a transcript is looking for is not where one row ends
   but where one *ask* ended, which is the unit the domain has (`CONTEXT.md`,
   ADR 0007), and the turn boundary was a caption in the middle of all that
   ruling. The rule is on the boundary rows and on no other now, on both of
   their edges, so a turn is a block of rows between two lines.

   **And a sentence takes a measure.** The row hands its content column
   everything between the kind and the clock, which at the window's own size is
   about nine hundred pixels — a hundred and forty characters a line, on the one
   screen where what is being read is prose somebody wrote. 80ch, on the four
   elements that hold sentences and not on the row: a diff, a permission
   request's choices and a raw frame are evidence laid out in columns, and what
   they need is the width the row has.

3. **The first screen was three empty boxes.** A Timeline empty state, a
   disabled composer under it, and 28vh of empty Console under that — a quarter
   of the window bounded and drawn to say that nothing had happened, beneath a
   screen that had already said so with the control that resolves it.

   The Console's own comment has claimed *empty, it is a strip* since the ring
   that built it, and it took the trace's height whether or not there was a
   trace; it is the height of its own sentence now, on **both** surfaces being
   empty rather than the selected one, because a panel that resized when a tab
   was clicked would move the window under the click that moved it. And the
   composer is drawn where there is a Session to prompt into: a disabled prompt
   box says *this is how you drive this window* to a reader who cannot drive it,
   and every word of the placeholder it carried was already on the empty screen
   above it, in a heading and a sentence rather than in grey text inside a box
   nobody can type in. A handshake that failed is the exception and the reason
   the condition is not `ready` — it has no Session and its own account of why,
   and this is the surface that draws it.

4. **The bar carried two verbs and one of them was always dead.** `Connect…`
   refuses while an agent is running, because the launch form does; `Disconnect`
   has nothing to end for the whole of a first run. So the most prominent chrome
   in the window permanently held a button nobody could press, and the pair read
   as two acts rather than as the one thing they are. The bar's own comment
   already said what it wanted — *Connect… and Disconnect read as one control in
   two states* — and the way to be one control in two states is to be one
   control. Neither state is a refusal, so the ARIA guard the pair needed went
   with them.

   **And Disconnect is quiet until it is reached for.** A red outline at rest
   made ending the Connection the highest-contrast object in the window: the
   rarest act on the bar, drawn as the loudest, on chrome a reader crosses to
   get to the appearance beside it. The tone is what the act costs and not how
   often it is wanted, so it is the platform push button every other ordinary
   control here is until a pointer or the keyboard arrives on it, and then it is
   `--bad`. Nothing is hidden and nothing moves: the icon, the word and the
   title say what it does at rest.

### And one that was broken rather than plain, found by breaking it

**A rule can leave this stylesheet without anything failing, and one did.**
Appending a paragraph to a comment that was already closed leaves a second `*/`
behind it, which makes the sentences between the two closes into CSS: Tailwind
reads them as a malformed selector, drops the rule that follows, and emits
everything else without complaint. What went was the whole of `.split` — the
window lost its two columns, its inset, and the ground its regions are cards on
— and all two hundred and seventy-seven tests passed, because they read
`input.css`, which still said all of it, and the browser reads `sheet.css`.

That is the one gap between the two the rest of `style.rs` cannot see across,
and every instance of it has the same shape: a `*/` where no comment is open.
`no_comment_in_the_source_is_closed_twice` is one assertion over the
comment-stripped source, and it names the line the stray close is on. This file
is more prose than declaration by volume, which is what makes the failure worth
a test rather than a habit.

**These captures were not retaken.** This pass was rendered from `mock.rs` —
`INSPECTOR_MOCK=… cargo test -p acp-inspector mock::writes` — and
screenshotted in a headless Chromium at both supported window sizes in both
schemes, the way the pass above it was. The images at the top of this file are
now four rings stale. Retaking them needs a display and a running
`cargo run -p acp-inspector`.

## Redesign

The window as ADR 0009 arranges it, rendered from the fixtures in
`crates/app/src/mock.rs` at the same two supported sizes:

```text
INSPECTOR_MOCK=target/mock cargo test -p acp-inspector mock::writes
```

- [Populated, 1440 x 880, light](redesign/populated-1440-light.png)
- [Populated, 1440 x 880, dark](redesign/populated-1440-dark.png)
- [Capabilities, 1440 x 880, dark](redesign/capabilities-1440-dark.png)
- [First run, 1440 x 880, light](redesign/first-run-1440-light.png)
- [Populated, 960 x 640, light](redesign/populated-960-light.png)
- [Populated, 960 x 640, dark](redesign/populated-960-dark.png)
- [Command palette, 1440 x 880, dark](redesign/palette-1440-dark.png)

The two narrow captures are the arrangement at the window's own minimum, which
is the one the drawing gives a window too narrow for three columns: one region
at a time, with a **Details | Session | Messages** bar at the foot to choose
between them, and a toolbar that drops the protocol facts it can no longer fit.
Nothing is lost at that width — the three regions are the same three regions —
and the chooser does not exist while all three are on screen.
