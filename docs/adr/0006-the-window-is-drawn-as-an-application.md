# The window is drawn as an application rather than as a page of panels

## Context

[ADR 0005](0005-daisyui-owns-ordinary-components-and-themes.md) gave the window a component library,
a palette whose greys separate and whose hues say things, and a corner worth having. What it did not
change is the shape the regions are arranged in, which is the one ADR 0004 inherited from the
hand-written sheet: full-bleed areas divided by one-pixel rules, a toolbar separated from the
document by another, and a Console separated from both by a third.

That arrangement reads as a page divided into areas. Every desktop tool worth copying draws the
other one, and the difference is not decoration: a region bounded on four sides and set into a
ground is an *object*, and a region whose edge is a shared hairline is a *fold*. The same pass
found four more places where the window was drawn as a document rather than as a program — a strip
of chrome under a strip of chrome, a heading register out of a settings pane, a row that spent a
seventh of the window on metadata before the first character of content, and a menu bar naming the
renderer's verbs instead of this tool's.

None of this is a new palette or a new component system. What follows is where the window is
arranged, what a register is for, and which of ADR 0004's rules the arrangement retires.

## Decision

- **Every region is an inset card on a chrome ground.** The Timeline, Session
  Settings and the Console take the document surface, a hairline on all four sides and
  `--radius-card`; the ground behind them is the chrome surface the toolbar is already drawn in,
  showing through an 8px gutter. The single-edge borders come out — a region bounded and *also*
  separated by a line is the two arrangements at once — and so does the line under the toolbar,
  which now shares one continuous surface with the ground behind the cards. It costs 32px of width:
  the centre screen's floor at the 960px minimum window is 432px rather than 464px.
- **One strip of chrome per panel.** The Trace's Export and Clear move onto the Console's own bar
  beside the tabs, as icon controls with their sentences as tooltips and their names in the
  accessibility tree, and only on the surface they act on. The Trace's body draws no header at all.
- **Nothing is set in tracked capitals.** The 10–11px uppercase label at `0.1em` is the house style
  of a decade of settings panes; eight of them down one rail were the loudest thing on the window.
  A section heading is found by weight and colour at the size the content already is — 12px, the
  answer weight, the ink — over a field label at the same size in the ordinary weight and the muted
  tone. `--text-micro` and `--tracking-label` are removed from the vocabulary rather than left
  unused, because a step nobody has a use for is a step the next person reaches for.
- **A Timeline row leads with what the agent said.** The timestamp moves from the second of three
  fixed columns to the row's right edge: content starts 112px in rather than 196px in, and nothing
  is hidden or deferred to hover. Below 1100px of window the kind and the clock share the first line
  and the content takes the whole of the second, because at the minimum window three columns left
  quoted evidence six characters wide.
- **Every row a pointer can be over says so**, in `--hover`, transitioning its ground and nothing
  that lays out.
- **An empty screen offers the one thing that resolves it**: a session that can be opened, or the
  launch fields opened. The state that is waiting on the agent offers nothing.
- **The window's chrome is this tool's.** A menu bar naming Trace, Agent and View — every item a
  control that also exists on screen — beside the platform's own Edit and Window. An icon drawn from
  the window's palette rather than the renderer's logo. And the toolbar names the *subject*, which
  is the running Agent, because the title bar directly above it is already drawing the program's
  name.
- **The Console can be typed into.** A filter narrows whichever surface is showing, by
  case-insensitive substring over the row as it is drawn. It is presentation and nothing else: both
  stores go on capturing, the tab counts go on counting, an export writes everything, and a narrowed
  surface says what it is hiding so that a hidden row and a missing one never look alike. A reveal
  arriving from the Timeline clears it, because a row this panel has been told to show must not be
  answered with nothing.
- **A Trace row may name what the envelope says it is.** `Frame::summary` reads `method`, `id`,
  `result` and `error` — JSON-RPC's own four fields — and the row leads with one word and the id.
  This is not the typed layer: it never looks inside `params`, never names a v1 type, and a frame in
  none of the four shapes is labelled with nothing and drawn exactly as it was before. The bytes
  stay on the row, verbatim and unreformatted, behind the summary rather than replaced by it.
- **A row's own controls are quiet until they are wanted.** The raw disclosure
  and the way back to the wire were drawn on every Timeline entry at all times —
  two faint controls per row, which is what made a screen of the agent's words
  read as a screen of chrome. They appear on hover and on focus, the rule the
  Console's rows already state about their copy control, and an open disclosure
  stays lit whatever the pointer is doing. **The space stays reserved**: these
  lists are appended to while they are read, so a row that grew under the cursor
  would move the rows below it.
- **A count of zero is not drawn.** The Timeline's heading and both Console tabs
  said `0` on an empty surface, which is the empty state's sentence repeated in
  a shape that means *there are some*. The two counts that are an agent's own
  answer — the Session Settings it published, the sessions it listed — keep
  theirs at zero, because there a nought is what the agent said.
- **Each tailing list offers the way back to its newest row**, once it is not at
  it. The lists still never move on their own: no auto-scroll, no jump on
  arrival, no anchoring. Where the list *is* is the engine's to know, so a
  delegated scroll listener marks it and the stylesheet decides what the mark
  looks like — no signal, no re-render. The control is positioned against the box
  that holds the list rather than being a row of it: as a sticky list item it
  took the list's row styling, took the auto margin that makes the newest row
  hold the free space, and contributed to the scroller's own overflow — so there
  was room to scroll past the newest row, which is exactly when the control is
  drawn. `position: absolute` names two rules now, each asserted with the
  ancestor it is laid out against, which is what #74 asks for.

- **The centre says what the agent claimed.** The two accounts were most of the
  rail — measured on a populated window, 1430px of 1994px in a 568px viewport, against four hundred
  pixels for everything a reader presses. Collapsing them behind a summary bought the height and
  none of the width, which is the other half of the problem: a capability row is four facts and
  288px of rail holds three, so every row was two lines and the block was a column of them. The
  accounts move to the centre screen behind a switch — **Timeline | Agent | Client**, in the tablist
  the Console already uses for surfaces behind one panel — and each gets a screen, where a row is
  one line. The rail was left with launch, sessions and authentication, and stopped scrolling —
  which is the state the bullet below this one found it in.
  **A screen each rather than two columns of one.** They shared a screen first, which is what §9's
  *beside* asked for while both were in a rail; on the centre it meant the agent's account — the one
  that changes, the one a reader came for — spent half its width on a block that is identical on
  every launch of every agent. They are still drawn identically and still never merged into one list
  of "capabilities"; what the split costs is that §7.6's causal reading, a boolean config option
  whose shape answers this client's claim, is a tab away rather than a glance away. That is the
  price of an Agent screen that is about the agent.
  **And no disclosure.** Each account was folded behind its summary while it lived in the rail,
  because the rail could not hold the rows; on a screen with room to draw them a fold is a chevron
  and a click in front of the thing the screen is for. The line the fold opened stays, because the
  count on it is worth reading before the rows are: how many claims were advertised, and how many
  were refused when something drove them — an agent that advertised a method and refused it is the
  finding this exists for (§7.7). A count is not a grade: it says how many rows say *advertised*,
  orders nothing and judges nothing.
  **The move is a move and never an abridgement**: every row carries all four of §7.7's facts, in
  words, in the accessibility tree, exactly as it did in the rail.
  **And the screens are drawn for the room they now have.** The identity was three rows of a
  two-column list — `title`, `name`, `version` — which is what a 288px rail does to three short
  facts; it is the screen's heading now, a name with the two protocol strings under it. Every run of
  claims was its own grid, so each little table aligned with itself and with nothing else; the
  tracks are stated once for the account, so the four facts line up down the whole of it. The shape
  a run was made in was a paragraph between the rows and is an annotation over them. And the
  advertisement's own title is gone: the tab names the account and the heading names who it belongs
  to, so a third saying of it in four inches was the rail's block title kept after the block stopped
  being one.
  **The composer is drawn on the turn's view alone, and only where there is a turn to have.** It
  stayed under all three views at first, on the reasoning that a Stop which moved was a turn nobody
  could cancel; what that cost was a hundred and fifty pixels of a reference screen for a control
  one tab away. The Connection's own Stop is in the toolbar and never moves. And with no Session
  open it is not drawn at all: a disabled prompt box is a control that says *this is how you drive
  this window* to a reader who cannot drive it, and every word of the placeholder it carried is
  already on the empty screen above it — in a heading and a sentence, beside the control that
  resolves the state. The exception is a handshake that failed, which has no Session and an account
  of why, and this is the surface that draws it.

- **And then the rail goes, because what was left of it was not a rail.** Moving the accounts out
  left a permanent fifth of the window's width holding three unrelated one-off tasks — a launch form
  written once per connection, a session listing read once, a login used once — of which, measured
  on a populated window, forty per cent was empty. Three rounds of restyling made it a tidier column
  of the same thing. What each of the three needed was different:
  **Launching is a dialog.** A native `<dialog>` shown with `showModal`: the platform traps focus,
  dims the window, closes on Escape and gives focus back to whatever opened it — four behaviours a
  hand-rolled overlay owes and gets subtly wrong. Whether it is showing lives on the element and in
  no Rust signal, because Escape and the backdrop close it without telling anybody and a window that
  believed otherwise would have a Connect button that works once. The fold comes out with the column
  that made it necessary, and so does the line naming the running agent — that line existed because
  the fields might be hiding it, and the toolbar has said the subject since this ADR's first pass.
  **Sessions and authentication are groups of the Agent screen**, which is the agent they are about:
  its identity, then what a reader presses, then what it advertised. Nothing about them is
  abridged — §7.7's four facts, the six lifecycle capabilities drawn whether or not they were
  advertised, `logout` beside the auth methods rather than among the session controls.
  **What is left is a window without a left column**: a toolbar, one centre screen with its tabs,
  Session Settings beside it while a session is live, and the Console. The centre screen gains the
  rail's whole width at every window size, which is the width the four-fact rows were always short
  of. What this costs is that launching is now two clicks from cold rather than one — the price of
  not spending a fifth of every window on a form nobody looks at twice.

- **And the three that were left share nothing, so they stop sharing a screen.** Dissolving the rail
  put launch in a dialog and swept sessions, authentication and the capability listing onto the Agent
  screen together. Measured there, the screen was 1126px wide and used 640px of it — 43% empty, on
  the screen the accounts had moved to *for the width* — the table began 393px down so two of its
  thirteen rows were visible, and `Sessions` was a heading twice, three hundred pixels apart, once as
  an action group and once as a capability run. They are three kinds of thing:
  **A session listing is a set of objects, and it belongs to the screen that draws one.** It is a
  dialog off the Timeline's header, where the live session is already named: opening, switching and
  ending happen where the effect is visible. A dialog and not the sketched dropdown, because a listed
  session carries an id, a title, a working directory, a roots control, three calls and a page
  cursor — a menu holding that is a panel wearing a menu's clothes, and one that dropped it would be
  abridging what §7.5 gates a claim at a time. The five paragraphs of consequence above the list
  become the controls' own descriptions, by the pattern `session/new` already used: tooltip for a
  pointer, `aria-describedby` for the tree, and the accessibility tree loses nothing.
  **A login is a state.** *Login required*, *Authenticated* or *Login refused* on the identity line,
  which is where an application with an account puts it; the methods and the two calls stay under it,
  the `logout` sentence moves onto the button, and the block draws nothing at all where the agent
  advertised no method — which is most agents, and for which the heading used to introduce one
  sentence saying so. Nothing is said where nothing has happened: `Unasked` is not *logged out*, and
  a line reporting it would be this window inventing a state the protocol has not got.
  **A capability listing is a reference table**, so it gets the screen. The four facts are four
  columns whose slack falls after the last of them rather than between the first two — which is what
  the 40rem cap was really answering — the group headings stay on screen while their rows scroll, and
  the refusal §7.7 had to give a line of its own, because the widest thing it had was a 280px rail,
  fits on the row. A chip row narrows the table to what was advertised, what was not, or what was
  refused, with a chip drawn only where it would leave rows. The narrowing is presentation and
  nothing else: both counts on the summary line go on counting the whole account, because they are
  the agent's answer rather than the filter's, and a narrowed table says what it is hiding — the
  rule the Console's filter already states, now on its second surface.

- **And a moved surface is redrawn, not re-hung.** Each of the three arrived in its new place as the
  markup it had in the rail, which is a different defect from being in the wrong place and had to be
  fixed separately.
  **The sessions dialog is three parts in the order a reader needs them** — the session they are in,
  the sessions they could be in, the way to a new one — rather than two buttons, five paragraphs and
  one undifferentiated list with the live session marked by a word halfway down its fourth row. The
  live session is drawn whether or not `session/list` has ever been pressed, because its id is not
  the listing's to give and the calls that end one carry nothing else; while a listing is in flight
  it keeps that id and loses its description, which *was* the answer being replaced. A row leads with
  the call, with the `additionalDirectories` control under it rather than in front of it, and the two
  calls that end a session are quiet until the row is hovered or focused — this ADR's own rule about
  row controls, with the space reserved so nothing moves under the pointer.
  **The hairline that marked the inspector's own commentary comes out.** It was a rail treatment: a
  pixel down the near edge, no room to spare, and a way for a reader to skip a sentence they had
  already read. On the claims screen that rule lands between every group heading and its rows — where
  a table wants nothing — and quotes an annotation about those rows as though it came from somewhere
  else. The register is the size and the tone, which is what carried it before the bar was added.

- **Hover-reveal is for a control that repeats something already on the row**, and the sessions
  dialog is where that was learned by getting it wrong. The rule above was written for a Timeline
  entry's raw disclosure and its way back to the wire — both of which say again, quietly, what the
  row has already said. Applied to `session/close` and `session/delete` it hid the *only* way to do a
  thing: what is not drawn cannot be looked for, a pointer is not the only way to arrive, and a list
  whose controls appear under the cursor has to be swept to be read. What made three calls at equal
  weight read as a wall was the equal weight, so the way in is the row's own size and the two ways
  out are a step down from it. `style.rs` fails on a rule that hides one.
- **The platform is given two more of the dialog's behaviours.** A click on the backdrop closes a
  dialog everywhere else, and `showModal` does not do it — so the backdrop is a
  `<form method="dialog">` whose submit button is the click, which is the platform's own way out
  rather than a listener to attach, remove and reason about. And the first focus goes to the dialog's
  box rather than to the first focusable control in it, which was Close: the least likely thing a
  reader came for and the one whose accidental Enter undoes the opening. The launch dialog keeps its
  first field focused, because typing is what it is opened for.
- **Outline icons are drawn as outlines.** The crate hands its SVGs over with `fill="currentColor"`
  on the element, so every closed path in one was painted solid *and* stroked. On a chevron, whose
  path is open, that is invisible; on a refresh arrow, a trash can or a stop it is the difference
  between the icon and a blob of ink. Every icon in the window changed with one declaration.
- **One connection at a time is a rule the toolbar keeps.** Connect stayed live while an agent ran,
  so the window offered to start a second one and would have opened onto a Launch that refuses.
  Stopping is the deliberate act, and it is the one on offer while an agent is there.
- **But a connection that is *replaced* takes its session's view with it.** Keeping the record and
  keeping the view are two different questions, and they had two different answers by accident: a
  `session/new` on the same agent cleared the timeline, and a relaunch did not — so the larger break
  of the two was the one that left a dead agent's rows above a live agent's, undifferentiated, under
  a composer sending to the live one. The timeline is the view of the one live session, which is what
  its own store says it is; it empties where that session is left behind, and only where there was
  one, so an agent that talked before any session existed keeps its rows. The record is the trace,
  which is untouched and marks the seam between connections.
- **A connection that ended keeps its record.** Nothing is cleared when an agent goes — the timeline,
  the trace, the diagnostics, the settings it published and the sessions it reported are what this
  tool was run to produce, and an agent that went away mid-session is the finding rather than a mess
  to tidy. What the window owes is to say which state it is in wherever there are controls it is
  about, which Session Settings and the login already did and the sessions dialog now does.

- **And a sweep for what the rail left behind.** Moving three surfaces off a column leaves rules and
  wrappers that describe a shape nothing has any more, and they do not announce themselves. The sweep
  found: the identity's three-row table, whose rules outlived the heading that replaced it; an
  `.identity-body` contract guarding a class no markup emits; two dialog boxes declared twice, ten
  identical lines each, so the only difference between them was the one number nobody could see was
  the difference; four selectors declared twice hundreds of lines apart, which is the hazard
  `style.rs` reads by — it takes the first rule with a matching selector; an auth method drawn as a
  one-column grid with its call *under* its name, which is a 288px column's only option and on a
  screen the width of the window is a form where an account belongs; that same row wearing daisyUI's
  `list-row`, whose own grid shrink-wrapped it to a quarter of the width it had; and `copy` as the
  last text control among icons — kept as a word because a clipboard *character* is not in every
  platform font, which stopped being the alternative when this window took an icon crate. The
  vocabulary went too: contracts named for a rail that is not there.

## What this does not change

- The palette, the themes, the contrast floors and the rule that meaning is never carried by colour
  alone. No colour value moved.
- The motion rules. Arriving Frames, Timeline entries and diagnostic lines still do not animate; the
  two transitions added are grounds under a pointer, which lay out nothing, and a reader who asked
  for less motion still gets none.
- Raw-first. Every surface that drew bytes still draws them.
- Conformance Annotations remain evidence rather than severity.
- The generated stylesheet is still committed and compiled in, and a valid checkout still builds and
  runs without Node.

## Consequences

- **ADR 0004's flat-window rule is narrowed rather than dropped.** *Panels and rows are flat* is
  still true of everything inside a region; what changed is that a region is now a bounded object on
  a ground rather than an area between hairlines. `--shadow-lift` still belongs to the one thing that
  overlaps content and `--shadow-card` to the Timeline's evidence boxes; no region takes either.
- **ADR 0004's type scale loses a step.** Five sizes become four, and the rule that went with the
  fifth — *the uppercase label a group wears* — is retired outright. `style.rs` asserts that no rule
  in the sheet sets `text-transform: uppercase` and that neither retired token is declared, so the
  register cannot come back one rule at a time.
- **The Console's "nothing decoded" line needed a word.** [`CONTEXT.md`](../../CONTEXT.md) and
  [§9](../architecture.md#9-screens) now say what was always meant: nothing decoded *as v1*. The
  envelope is the transport's own vocabulary, which is what the Console is for.
- **The window has a menu, so it has ids to keep in step.** `shell.rs` declares the commands and
  `main.rs` answers them; a test reads the command module out of the source and fails when the
  window does not answer one, because a menu item that does nothing is worse than one that is not
  there. What a click *does* is not testable without a window, and is not tested.
- **The launch form's props lost half of themselves, and the sheet lost the fold.** `SpawnForm` no
  longer takes what is running or whether the fields are open, because a dialog is neither folded nor
  beside the agent it started; `.launch-disclosure` comes out of the sheet and out of the
  `::details-content` list this ADR added, and the running-invocation rules go with it. `connect.rs`
  is the one place that opens or shuts the element, in two lines of script, and it interpolates
  nothing an agent said.
- **There is a way to look at the window without running it.** `crates/app/src/mock.rs` renders the
  populated screens from fixtures, with the committed stylesheet, into an HTML file per scheme —
  which is what `design/inspector-visual-audit.md` needed a person with a WebView to produce, and
  why that record went stale. It renders on every `cargo test` and writes only when
  `INSPECTOR_MOCK` names a directory. It is not the engine: fonts, scrollbars and native disclosure
  behaviour are still only visible in the real shell.
