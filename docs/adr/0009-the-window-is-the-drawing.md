# The window is the drawing

## Context

A drawing arrived — one HTML page, one populated screen, both schemes — and it is not a restyle of
what this window already was. It is a different arrangement of the same regions, and it answers
three measurements the previous rings took and did not resolve.

**The Console had the width and did not need it.** [ADR 0006](0006-the-window-is-drawn-as-an-application.md)
put it across the bottom because "both tabs' content unit is a long line, and what a long line needs
is width" — which is true of a *payload* and false of a *row*. What a Trace row actually carries is
an ordinal, a time, a direction, an envelope and a preview: sixty characters, in a strip fourteen
hundred wide, with the rest of the window's height paying for it. The payload needed the width, and
the payload was behind a per-row disclosure that opened *in place* — pushing every row under it down
a list the reader is watching arrive.

**The two accounts had a screen and cost the turn one.** ADR 0006 moved them out of the rail because
a capability row is four facts and 288px holds two. That is still true. What moved with them was the
centre screen: the Timeline shared a tablist with two reference tables, so the screen this tool
exists for was one of three things a switch chose between, and the rail was left holding launch,
sessions and authentication — a column that did not scroll and mostly did not change.

**And the palette was chosen to be safe.** One blue said *direction* and the same blue said *the
reader did this*, at different strengths; the greys did the separating and the hues said things,
which is right, and one of the hues was saying two things.

The drawing puts the Console beside the turn, gives the rail everything scoped to the connection,
and spends a hue nobody else in this category spends. None of that changes what a region *is*: the
Console still holds what the transport seam produced below the typed layer, the Timeline is still
the live session's stream and the requests waiting in it, and the rail holds nothing about either.

**This ADR was written once and is being written again**, because the first implementation of it was
a translation and this one is not. The first pass kept the previous vocabulary and moved the regions
into the new arrangement; what it produced was recognisably neither — the drawing's layout in the old
palette, at the old sizes, with the old rules deciding every case the drawing had an answer for. The
decision recorded below is the second one: **the drawing is the specification, the stylesheet is
rebuilt from it with nothing carried over, and where the drawing and a previous ring's rule disagree
the drawing wins and the rule is rewritten.** What is *not* on the table is behaviour: every
affordance, every protocol gate, every sentence and every accessible name is the same, and the tests
that hold them are unchanged.

## Decision

- **Three columns: the rail, the turn, the wire.** The toolbar is one row and the body is the
  other; the body is a fixed 244px rail and the two screens beside it, the turn wider than the wire
  because one is read and the other is scanned. The Console is a column rather than a row, and the
  row that gives way when a window is dragged short is no longer the one the reader is reading.
- **A frame is selected rather than expanded.** A Trace row is a button; the frame it names is read
  whole in a pane at the foot of the list, laid out and coloured by what it is made of
  (`json.rs` — a lexer, not a parser, so a frame nobody can parse is drawn in one colour and every
  byte of it). **The pane is the third exception to the Indentation switch** (`indent.rs`): the
  switch exists so a list of ten thousand frames does not lay out ten thousand documents for nobody,
  and this is one document, selected, on the surface whose whole subject it is. What leaves the
  window — a copy, an export — is the bytes that crossed, unchanged.
  Until a reader points at one, the pane reads the newest frame *they can see*: an empty pane under
  a live trace is a region asking to be told something it can already see, and one reading a frame
  the reader narrowed away would be the surface answering the chip with the row it just hid.
- **The envelope's four shapes can be hidden, and the fifth chip exists so nothing can be.** Chips
  beside the filter, all on when the surface opens, and one for the frames in *none* of the four —
  drawn only when there is such a frame, because a chip that can hide nothing is chrome and a shape
  with no chip is traffic that can be hidden with nothing to press to bring it back. Narrowing is
  presentation: both stores go on capturing and counting, an export writes everything, and the
  surface says what it is hiding whether it was typed or pressed.
- **The rail holds the Connection and everything scoped to it**, behind two tabs: the live Session
  with its Settings, and the two accounts of what was claimed. The rule for what may be added is the
  scope: nothing about the turn, which is the screen beside it, and nothing about the wire, which is
  the screen beside that. **Connecting is in the rail's own card** rather than in the toolbar: what
  is connected, what it was launched from, and the two acts on it are one tile, and the bar is left
  saying what the window is looking at.
  **The accounts come back to a rail, and ADR 0006's measurement is not disputed.** Both halves of it
  are still true — a row is four facts, a rail holds two — and neither is a problem now: the row is
  two lines by construction, and the account is *behind a tab*, so its length costs the rail nothing
  until somebody asks the question it answers. What the centre screen gets back is the whole width
  for the turn.
- **Where the turn stands is the turn's header and the sentence about it is the composer's.** A
  region's state belongs in that region's header; the sentence that explains it belongs above the
  box a reader would act in. Neither says the other's half.
- **A row leads to its frames; it does not carry them.** Every entry used to hold a `raw`
  disclosure and an *in the trace* chip, which put two controls and a chevron on every message in a
  conversation the drawing draws as text. The wire log is beside the turn now and its pane reads one
  frame whole, so the row is the control: pressing a message selects the traffic it came from, the
  same act is on a named control for the keyboard and for anything reading the document, and the
  bytes are read where the drawing reads them. The two cases where the frames *are* the rendering —
  traffic this window cannot name, a conformance annotation whose claim is about the bytes — draw
  them under the row without asking, because there a disclosure would hide the whole of what the row
  says. §8 is unchanged: every frame is still captured, counted, exported and readable as it
  crossed.
- **A row says what it is in the protocol's own words, and only where the drawing draws a label.**
  `user_message_chunk`, `agent_thought_chunk`, `session/request_permission` — the wire's names, so a
  reader holding this screen against the wire log beside it is not translating. The agent's own
  message is drawn bare, as the drawing draws it, and so is a card that names itself in its own
  head; the kind stays in the accessibility tree in every case, because the mark in the gutter is
  decoration and something has to carry the word. Nothing in the flow carries a timestamp: every
  frame in the wire log has one, and a column of stamps down a conversation is a column nobody
  reads.
- **The session is named and acted on in one place, and it is the rail.** The turn's header carried
  a switcher, a copy and a chevron between the region's name and its state — three controls in the
  one strip the drawing keeps clear — while the rail said the same thing again. The rail keeps it:
  the id whole and copyable, the way to another session, the way to the ones the agent is holding.
- **Two fields are drawn as the surface they are in.** The composer is one
  bounded box with its footer inside it and the palette's question is the head of
  the palette, so the window's focus ring around the control *inside* either one
  was a second boundary inside the first. Neither goes without an indicator — the
  composer's box takes the accent while the keyboard is in it, the rule under the
  palette's question does — and `style.rs` holds the pair: a rule that turns the
  ring off anywhere else fails the build.
- **A copy control goes where the whole of the bytes are drawn.** A wire log row is the head of a
  payload with the rest cut off, so a copy on it either hands over bytes the reader cannot see or
  hands over the ellipsis; the pane below reads the selected frame whole and is where it is taken
  from. Everywhere the bytes themselves are on screen, the control is still beside them.
- **A timeline entry has a marked gutter.** One 20px column of glyphs down the left of the flow, so a
  streamed conversation is scannable without reading every kind word. The mark is decoration and
  says so: every row still carries its kind in text, and the glyph is keyed on the tone rather than
  on the kind, because what a column of marks is for is what one row shares with another of its
  sort.
- **The composer offers the commands the agent published.** `available_commands_update` is the
  agent's own list; the newest announcement replaces the earlier one rather than merging with it,
  because a composer that accumulated them would offer a command the agent had stopped publishing.
  Choosing one *refills the box* and never sends: what a command takes after its name is the
  reader's to type.
- **The mode is drawn twice, because the drawing draws it twice.** Full-width rows in the rail, where
  a session's configuration is; and a cycling control in the composer's footer, where a reader stands
  when they want it changed. They are one setting and one call — the footer control sends
  `session/set_mode` for the next mode in the agent's own list — so the duplication costs nothing but
  pixels and buys the press where it is wanted. A previous ring would have forbidden this as one
  control said twice; the drawing's answer is better and is what is built.
- **Every command the window has is reachable by name.** A palette over the window, opened from the
  foot of the rail or with the keystroke every application with this control uses, closing on
  Escape and on the backdrop because the platform's `<dialog>` owns all of that. Every entry in it
  is a control that is also on screen — the rule `shell.rs` already states about the menu bar, for
  the same reason.
- **The accent is a lime.** Every developer tool in this category is blue, and blue was also this
  window's *direction* colour. A yellow-green is far enough from both directions, from success and
  from trouble to be nobody else's meaning, and it is the one hue on the window that is only ever
  about the reader: the value they set, the row they asked for, the control the keyboard is on, the
  session that is live, the text they have selected.
- **The direction colours swap to match the drawing.** A frame this client sent is the blue an
  outgoing arrow is drawn in and one the agent sent is amber; it was the other way round for five
  rings. Nothing is claimed by either colour that the arrow, the word on the row and the accessible
  name do not already say — which is exactly why the pair could follow the drawing rather than the
  drawing be bent to match the pair.
- **The palette is the drawing's, value for value.** Not derived from daisyUI's roles and not tuned
  to clear a floor: `theme.css` is the drawing's own list of hexes under the drawing's own names, and
  the two daisyUI themes are fed the same values so a button, a field, a badge and a dialog come out
  in it. What the previous arrangement did — derive every inspector meaning from a daisyUI role with
  a `color-mix` chosen to clear AA — produced colours nobody drew, and the whole point of a design
  pass is that somebody did.
- **Two things animate, and both are the drawing's.** A row fades up as it arrives; the dot beside a
  stream that is open blinks while it is open. This **reverses [ADR 0004](0004-the-design-vocabulary.md)'s
  rule against keyframes**, which was right about what it was refusing — a tool that animated its
  arriving evidence would be a tool that made the reader wait to read it — and wrong as an absolute:
  220ms of opacity under a row that has just appeared is how a dense list says *this one is new*, and
  a blinking dot is how a stream says it is still open without a second sentence. A reader who asked
  their operating system for less motion gets neither, in one rule, for everything.
- **Capitals are back in two registers and no others.** The tracked label, for a section's name; and
  the tracked mark, for the tiny word that names a shape — a frame's envelope, a remembered
  invocation's transport. `style.rs` holds both shut: every rule that sets `text-transform:
  uppercase` sets one pair of size and tracking, so the register cannot be half-adopted somewhere
  else at some other size.
- **daisyUI keeps the components and this sheet repaints them.** Twelve are included — alert, badge,
  button, checkbox, fieldset, input, label, modal, select, textarea, toast, toggle — and the sheet
  says what the drawing paints them in, in daisyUI's own variables where there is one. `list` is not
  included, for the reason ADR 0005's ring found: every list here is an evidence surface with a
  layout of its own and `list-row` has to be overridden in each. `.alert` is the one component drawn
  as a box of this window's own, because the drawing's notice is a hairline in the tone of what it is
  about where daisyUI's is a filled grid.
- **IBM Plex is asked for and never fetched.** It is the family the drawing was made in; a desktop
  tool that reached across the network to paint its own chrome would look different on a machine
  with no route out. The platform's own faces are what follows it in the stack, which is what this
  window used before the drawing and still uses without it.

## Consequences

- **Three values are below the WCAG AA text floor, on purpose, and are named.** The floor was a hard
  contract for five rings and is now a floor with a measured, enumerated exception list: `--dim2` in
  both schemes (3.05:1 in light, 3.66:1 in dark) and `--out` (4.40:1) and `--ok` (3.83:1) in light,
  each on the tightest of the four grounds. They are the drawing's own values, they are the register
  the drawing puts its smallest metadata and its two direction hues in, and every one of them clears
  the 3:1 a signal owes. What holds them is
  `the_palette_is_the_drawings_and_every_value_below_the_text_floor_is_named` in `style.rs`: it fails
  if one of the three drifts lower, fails if one is quietly raised without the record following, and
  fails if a fourth joins them. Nothing in this window is known by colour alone — an unadvertised
  capability row says "not advertised" in words, a frame's direction is an arrow and a word, and every
  dot repeats what its row states — which is what makes the exception a legibility cost rather than an
  information one. `--line` and `--line2` remain decoration in both schemes for the reason they always
  were: at 3:1 a hairline on every row and every panel is a grid over a dense tool.
- **The stylesheet was rebuilt rather than edited, and so were its tests.** `theme.css` and
  `input.css` were written from the drawing with nothing carried over, and `style.rs`'s contract suite
  was rewritten against them: the same *kinds* of contract — one focus ring, nothing hidden until a
  pointer arrives, nothing positioned against the window, every colour a token, dark written twice and
  the two agreeing, every list that tails laid out bottom-up — asserted over the new vocabulary. Where
  a contract only made sense in the old arrangement it is gone; where it named a rule the drawing
  reverses, the test now holds the reversal.
- **A short conversation reads from the top.** The stream is still newest-first in the document and
  laid out bottom-up, which is what keeps a turn arriving at the bottom of the screen; what changed
  is where a handful of rows sits in a tall window — at the top, where the drawing puts them and
  where a reader scrolling back to the beginning of a turn expects to find it, rather than floating
  against the composer.
- **The corners are the drawing's ladder and the controls are its heights.** The
  radii were rounded to a five-step scale in the first pass and every control
  between 24 and 32 pixels came out a pixel or two off; they are nine steps now,
  the drawing's own, each named for what it is on — a chip, a control, a button,
  a tile, a card, an entry, a panel, the composer, a dialog. The heights are the
  drawing's too and are written out: a component library's `xs` was neither the
  rail's 24px nor a region header's 26px, and three families of button were
  coming out a size small. `style.rs` reads both back.
- **Two component decisions changed with the drawing.** A mode is a full-width row however many there
  are, rather than a segmented pair that became a native select at three — a list of three that hid
  itself behind a popup would cost the reader the press this rail exists for. And an ungrouped select's
  values are chips, one per value, with the native select kept for exactly the one shape a row of chips
  cannot say: a *grouped* list, whose groups are something the agent said about its values.
- **A narrow window shows one region at a time.** Below 1080px the rail, the turn and the wire are
  three states of one body and a segmented bar at the foot chooses between them; the shell's own
  minimum is unchanged. Nothing is lost at any width — the three regions are the same three regions,
  and the chooser does not exist when all three are on screen.
- **The visual audit's screenshots are stale**, and the render harness that replaces them
  (`mock.rs`) is updated rather than the audit: it draws the populated window, the first-run window
  and the capabilities tab, in both schemes, from fixtures with no agent and no runtime under them.
- **What did not change is the whole of the point.** Every affordance, every gate, every sentence
  the previous rings argued for is still on screen and still tested: the four facts on a capability
  row, the reasons a row cannot be driven, the roots controls and their rule, the session lifecycle
  behind its advertisements, the blocking requests inline where the turn stopped, the conformance
  annotations in the flow, the raw evidence on every entry, the copy control wherever bytes are
  drawn, the tail controls, the live regions, and the ARIA-disabled-rather-than-native guard on
  every control that can be pressed while an answer is in flight. This is where the window is and
  what it is painted in, not what it says.
