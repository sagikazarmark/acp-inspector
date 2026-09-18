//! The ACP Inspector's screens: a window onto an [`Inspector`].
//!
//! **Strictly presentational** (`docs/architecture.md` §5). Everything here
//! subscribes to a store and renders it, or takes a click and calls core back.
//! There is no protocol logic, no connection lifecycle and nothing to decide: a
//! failed spawn opens the console because the *status* says the agent never
//! started, and frames appear because the *trace* grew — both of them facts
//! core's tests assert without a window (`crates/core/tests/inspector.rs`).
//!
//! **One crate, a feature per renderer**
//! (`docs/adr/0011-one-ui-crate-a-feature-per-renderer.md`). The screens are
//! the same program whichever window draws them; what differs is the shell
//! around them — the frame, the title bar, the menu strip — and that is
//! `shell.rs`, the one module that imports `dioxus::desktop`, gated on the
//! `desktop` feature along with the three places this file calls into it.
//!
//! This is also the repo's first desktop artifact (§15 q1), so what it proves
//! beyond the screens is the ground itself: a WebView shell over an async child
//! process, built and run by plain `cargo run`.

#[cfg(not(any(feature = "desktop", feature = "web")))]
compile_error!(
    "acp-inspector draws in a renderer, and a build has to pick one: `--features desktop` or `--features web`"
);

mod agent;
mod appearance;
#[allow(dead_code, unused_imports)] // Unmodified registry install includes its complete public API.
mod components;
mod composer;
mod connect;
mod console;
mod copy;
mod disclosure;
mod elicitation;
mod elicitation_schema;
mod elicitation_words;
mod indent;
mod json;
// The window's own mark, drawn into its icon: reached only through the shell,
// so it goes where the shell goes. The bundle's icon includes the same file by
// path and does not pass through here (`examples/bundle-icon.rs`).
#[cfg(feature = "desktop")]
mod mark;
#[cfg(test)]
mod mock;
mod palette;
mod permission;
mod prompt_image;
mod rail;
mod session;
mod session_settings;
#[cfg(feature = "desktop")]
mod shell;
mod spawn;
mod style;
mod tail;
mod time;
mod timeline;
mod toast;
mod trace;
mod update;

use std::collections::HashSet;
use std::path::PathBuf;

use acp_inspector_core::{
    AgentCommand, Appearance, AuthState, CallError, ConnectionStatus, Cursor, Diagnostic,
    DrivenRecord, Indentation, Inspector, RecentCommands, Roots, SessionListing, SessionSettings,
    Settings, TimelineEntry, TracedFrame, Turn, TurnState, v1,
};
use dioxus::prelude::*;

use agent::{AuthenticationCall, Failed, Opening};
use appearance::AppearanceControl;
use console::{Console, Tab};
use indent::IndentationControl;
use rail::Rail;
use timeline::Timeline;

/// Which of the two screens the window is spending its width on.
///
/// **Two states, as drawn**: both side by side, or the wire alone — the second
/// for reading a frame that is mostly payload. It is a segmented control
/// because the two are exclusive and one of them is always true, which is the
/// one thing a toggle cannot say.
///
/// Window-session state, like the Console's tab and the rail's: what somebody
/// was reading yesterday is not what this window should open onto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spine {
    /// Both, which is what this tool is for.
    #[default]
    Split,
    /// The wire, whole.
    Wire,
}

impl Spine {
    const ALL: [Self; 2] = [Self::Split, Self::Wire];

    fn label(self) -> &'static str {
        match self {
            Self::Split => "Split",
            Self::Wire => "JSON-RPC full",
        }
    }

    fn says(self) -> &'static str {
        match self {
            Self::Split => "Show the timeline and the wire side by side",
            Self::Wire => "Give the wire the whole width",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Split => "split spine-split",
            Self::Wire => "split spine-wire",
        }
    }

    /// The other one, for the menu item that was a collapse — and so only where
    /// there is a menu bar to put the item in.
    #[cfg(feature = "desktop")]
    fn other(self) -> Self {
        match self {
            Self::Split => Self::Wire,
            Self::Wire => Self::Split,
        }
    }
}

/// Which region a window too narrow for three columns is showing.
///
/// The drawing shows one at a time below 1080px and puts a bar at the foot to
/// choose between them. Nothing above that width reads this; the stylesheet
/// decides where the threshold is, and this decides which one wins when it is
/// crossed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    /// The turn, which is what the window is for.
    #[default]
    Turn,
    /// The wire.
    Wire,
    /// The rail: the connection, the session, the claims.
    Rail,
}

impl Focus {
    const ALL: [Self; 3] = [Self::Rail, Self::Turn, Self::Wire];

    fn label(self) -> &'static str {
        match self {
            Self::Rail => "Details",
            Self::Turn => "Session",
            Self::Wire => "Messages",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Rail => "body on-rail",
            Self::Turn => "body on-turn",
            Self::Wire => "body on-wire",
        }
    }
}

/// Keep an automatic Console surface change from orphaning focus in Trace. A
/// reader elsewhere in the window stays where they are; one already using the
/// Console moves to the tab that names the surface replacing Trace.
const PRESERVE_CONSOLE_FOCUS: &str = r#"
const active = document.activeElement;
if (active?.id === "console-tab-trace" || active?.closest('[data-slot="trace"]')) {
  document.getElementById("console-tab-diagnostics")?.focus();
}
"#;

#[derive(Clone, Default)]
struct AuthenticationWork {
    connection: u64,
    pending: Vec<AuthenticationCall>,
}

impl AuthenticationWork {
    fn reset(&mut self) {
        self.connection += 1;
        self.pending.clear();
    }

    fn begin(&mut self, call: AuthenticationCall) -> u64 {
        self.pending.push(call);
        self.connection
    }

    fn finish(&mut self, connection: u64, call: &AuthenticationCall) {
        if self.connection == connection
            && let Some(index) = self.pending.iter().position(|pending| pending == call)
        {
            self.pending.remove(index);
        }
    }
}

#[derive(Clone, Default)]
struct SessionSettingsWork {
    generation: u64,
    modes: usize,
    option: Option<v1::SessionConfigId>,
    options: usize,
}

impl SessionSettingsWork {
    fn reset(&mut self) {
        self.generation += 1;
        self.modes = 0;
        self.option = None;
        self.options = 0;
    }

    fn begin_mode(&mut self) -> u64 {
        self.modes += 1;
        self.generation
    }

    fn finish_mode(&mut self, generation: u64) {
        if self.generation == generation {
            self.modes = self.modes.saturating_sub(1);
        }
    }

    fn begin_option(&mut self, option: v1::SessionConfigId) -> u64 {
        self.option = Some(option);
        self.options += 1;
        self.generation
    }

    fn finish_option(&mut self, generation: u64) {
        if self.generation != generation {
            return;
        }
        self.options = self.options.saturating_sub(1);
        if self.options == 0 {
            self.option = None;
        }
    }
}
use toast::Toast;

fn main() {
    // Which window draws the screens is the build's to say, not this file's:
    // the desktop shell is a frame, a title bar, a menu and an icon around them
    // (`shell.rs`), and any other renderer gets the screens and nothing around.
    #[cfg(feature = "desktop")]
    shell::launch(App);
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// Whether the agent is waiting on the reader for anything (§7.2, §7.8).
///
/// One question over both lists, because what the screen says about it is one
/// sentence: which kind of request stopped the tool is not a distinction the
/// reader's next action turns on. Core's lists rather than a scan of the rows,
/// which is the rule the flag already followed for one of them.
fn waiting_on(inspector: &Inspector) -> bool {
    !inspector.pending_permissions().is_empty() || !inspector.pending_elicitations().is_empty()
}

/// The window: a spawn form, the turn timeline, and the Console — the trace and
/// the agent's stderr as tabs — under both.
#[component]
fn App() -> Element {
    // One inspector for the window's lifetime. One connection at a time is
    // core's rule (`CONTEXT.md`, *Connection*), so the window has nothing to
    // arbitrate.
    let inspector = use_hook(Inspector::new);

    // What the spawn form remembers (§9). Read once, at the window's start,
    // because that is what "across restarts" means: nothing else writes the
    // file, so the list only ever changes here — after a launch that worked.
    let recent = use_hook(RecentCommands::load);
    let mut remembered = use_signal({
        let recent = recent.clone();
        move || recent.entries()
    });
    // Why the list will not survive this window, if it will not. Empty on every
    // machine with a home directory, which is why it is a note under the form
    // rather than a screen.
    let mut unsaved = use_signal(|| None::<String>);

    // How the window should look (§9). Read once at the start, for the reason
    // the recent commands are: nothing else writes the file, so the preference
    // only ever changes here — when somebody picks from the control in the bar.
    let settings = use_hook(Settings::load);
    let mut appearance = use_signal({
        let settings = settings.clone();
        move || settings.appearance()
    });
    // Painting is an effect rather than something the click does, so the stored
    // choice and a chosen one take exactly the same path: the window is painted
    // once at the start from whatever the file said, and again whenever the
    // signal moves. Saving is the click's, because the first paint is the file
    // being honoured and not a change to write back.
    let initial_appearance = appearance();
    use_effect(move || appearance::paint(initial_appearance));
    // And how a payload is drawn (§8). Put where every surface that draws bytes
    // can find it rather than threaded to each of them, which is the rule the
    // copy control already follows for the same three surfaces (`indent.rs`).
    let mut indentation = indent::provide(settings.indentation());
    // One listener for every tailing list in the window, installed once. Which
    // lists exist changes as tabs are selected and surfaces empty, so it
    // captures rather than being attached per element (`tail.rs`).
    use_effect(|| {
        document::eval(tail::WATCH);
    });
    // And the one keystroke that is not a menu accelerator, because it has to
    // work wherever the reader's focus is (`palette.rs`).
    use_effect(|| {
        document::eval(&palette::watch());
    });
    let choosing = settings.clone();
    let switching = settings.clone();

    let mut status = use_signal(|| ConnectionStatus::Disconnected);
    let mut frames = use_signal(Vec::<TracedFrame>::new);
    // What the trace no longer holds, and where the last export went. Both are
    // the trace's own answers (§10): how many frames a trace keeps and where an
    // export is written are core's decisions, and this window is where they are
    // read out.
    let mut dropped = use_signal(|| 0usize);
    // The failure is a string because a rendered failure is a sentence: an
    // `io::Error` is neither cloneable nor comparable, and what the panel does
    // with one is print it.
    let mut exported = use_signal(|| None::<Result<PathBuf, String>>);
    let mut lines = use_signal(Vec::<Diagnostic>::new);
    // What the console's log no longer holds. Zero for as long as it stays
    // unbounded — it is carried because it is half of what a line's identity is
    // made of, and a console that stopped tracking it would break silently on
    // the day the log is bounded rather than loudly now.
    let mut dropped_lines = use_signal(|| 0usize);
    let mut entries = use_signal(Vec::<TimelineEntry>::new);
    // And what each turn of this session came to, which the entries are grouped
    // by (§9). Its own signal beside them rather than a second reading of the
    // turn *state*: that one is where the live turn stands and is replaced by
    // the next prompt, and this is the record of the ones that are over.
    let mut turns = use_signal(Vec::<Turn>::new);
    let mut turn = use_signal(TurnState::default);
    // Whether the agent is waiting on an answer (§7.2). Core's list of pending
    // requests, asked as one question — never worked out from the rows on
    // screen, because core is where a request stops being answerable and a
    // window recomputing it would drift the moment it got one case wrong.
    let mut blocked = use_signal(|| false);
    let mut session = use_signal(|| None::<v1::SessionId>);
    let mut prompt_epoch = use_signal(|| 0_u64);
    // What that session is configured as (§7.6). Its own store rather than
    // something read off the session: the settings are filled from the setup
    // response and kept current by what the agent announces afterwards, and both
    // of those are core's to decide.
    let mut configured = use_signal(SessionSettings::default);
    // The protocol stores say what the Agent answered; these counts say only
    // that this presentation is still waiting for those answers.
    let mut settings_work = use_signal(SessionSettingsWork::default);
    // The agent's own account of itself, which the capability display is a
    // rendering of — and the two things that hang off it: what it answered when
    // asked for its sessions, and where authentication stands (§9).
    let mut described = use_signal(|| None::<v1::InitializeResponse>);
    let mut listing = use_signal(|| None::<SessionListing>);
    // Presentation state for listing requests, scoped to the Connection that
    // started them. Multiple asks remain possible because the Affordance is
    // gated only by the Advertisement; the count keeps the notice visible until
    // every ask on this Connection has answered.
    let mut listing_work = use_signal(|| (0u64, 0usize));
    let mut auth = use_signal(AuthState::default);
    // Authentication has no protocol-level loading state. This is only the
    // Connection-scoped presentation fact; core's AuthState remains the one
    // account of whether the Agent accepted or refused the request.
    let mut auth_work = use_signal(AuthenticationWork::default);
    // And what became of driving what it claimed (§7.7): the fourth fact each
    // capability row carries. Its own store beside the claim rather than
    // something read off the trace — the trace is a bounded ring, and a record
    // derived from one would report *not driven* about an agent that was driven
    // on exactly the long connection where somebody wants to read it.
    let mut driven = use_signal(DrivenRecord::default);
    // Why the last launch never got as far as a session, if it did not. Core
    // answers the handshake once; the stores carry everything else.
    let mut problem = use_signal(|| None::<CallError>);
    // The invocation the live agent was launched from, kept for the one thing
    // that needs it after the launch: a session created on demand is opened in
    // the same working directory the handshake's was (§7.5), and that is the
    // form's answer rather than this window's. The launched command and not the
    // form's current contents — the fields are editable while an agent runs,
    // and the agent that is running is the one being asked.
    let mut launched = use_signal(|| None::<AgentCommand>);
    // The launch handshake is longer than the Connection transition: core says
    // Connected as soon as the transport exists, while initialize and
    // session/new may still be in flight. Keep that interval explicit in the
    // launch control rather than calling it Running before the launch returns.
    let mut launching = use_signal(|| false);
    // And the invocation the form folds *around*: the launched command, once
    // the agent it started has described itself. Not `Connected` on its own —
    // that status is set optimistically, before the child is known to exist, so
    // a typo would fold the form and re-open it a frame later. This is the same
    // mark the launch handler already uses to decide an invocation is worth
    // remembering.
    let live = use_memo(move || {
        if status() == ConnectionStatus::Connected && described.read().is_some() {
            launched()
        } else {
            None
        }
    });
    // Which lifecycle call last failed, and what it said — `session/new`, a
    // listed session reopened, closed or deleted. Its own signal rather than
    // `problem`: that one is the handshake's, and a refusal here leaves whatever
    // was live still live. One signal for all four because it is one sentence in
    // one place: the affordances sit together and the latest answer is the one
    // worth reading — and it carries the method, because which of the four
    // refused is the finding (§7.5).
    let mut failed = use_signal(|| None::<Failed>);
    // Both screens from the start: the Console's promise is that the wire and
    // the agent's diagnostics sit *beside* the turn instead of in a separate
    // terminal (§9), and a live view behind a click is a click away from being
    // the terminal it replaced.
    let mut spine = use_signal(Spine::default);
    // And which of the Console's two surfaces is on screen. The window's rather
    // than the panel's, because the one thing that changes it without a click is
    // a connection status, and statuses arrive here. Not persisted, and the
    // default is stated by the type (`console::Tab`).
    let mut tab = use_signal(Tab::default);
    // Which lookup the rail is on. The window's, like the two above and for the
    // same reason: it is never written to `settings.json`, because what somebody
    // was reading yesterday is not what this window opens onto.
    let mut rail_tab = use_signal(rail::Tab::default);
    // And which region a window too narrow for all three is showing. Read only
    // by the stylesheet's narrow rules and by the bar at the foot that sets it.
    let mut focus = use_signal(Focus::default);
    // What each screen has been asked to show of the other's (§9: the turn and
    // the wire, at once). Both are the window's rather than either screen's,
    // because pointing at the other screen is not a thing a screen can do to
    // itself — and reaching the trace also opens the panel it is in, which is
    // this window's state and nobody else's.
    let mut revealed = use_signal(Vec::<u64>::new);
    let mut sought = use_signal(|| None::<u64>);
    // The one thing that shuts the Sessions dialog which is not the reader: a
    // session that opened. That is what every control in it which opens one was
    // pressed for, and the screen it opened onto is behind the dialog. A session
    // *ending* leaves it open on purpose — closing or deleting one is managing
    // the list, and a window that walked away in the middle would be answering a
    // click nobody made. `peek` rather than a read, because what this compares
    // against is the last render's value and subscribing to it would make the
    // effect its own cause.
    let mut opened_before = use_signal(|| None::<v1::SessionId>);
    use_effect(move || {
        let live = session();
        if live.is_some() && *opened_before.peek() != live {
            session::close();
        }
        opened_before.set(live);
    });

    // And the frame the operating system draws, which is the one piece of this
    // window's chrome the window does not paint. A desktop application names
    // what it is looking at in its title bar — the task switcher, the dock and
    // the window list all read it — and a tool that said only its own name
    // there was a tool whose four open windows were four identical entries.
    // Only where there is a title bar: another renderer's is not the shell's
    // to set from here.
    #[cfg(feature = "desktop")]
    use_effect(move || {
        let titled = match subject(described.read().as_ref(), launched.read().as_ref()) {
            Some(subject) => format!("{subject} — ACP Inspector"),
            None => "ACP Inspector".to_owned(),
        };
        shell::set_title(&titled);
    });

    // And the one thing the window says about *itself* rather than about the
    // agent: that a copy took (`toast.rs`). Provided here because the control
    // it answers for is on three screens, and takes itself away on a clock this
    // call starts — the window's, so no row can carry it off.
    let announcer = toast::provide();

    // One subscription per store, all the same shape: wait for the store to
    // change, then re-read it. Nothing here polls, and nothing in core pushes
    // into the UI — which is what keeps core headless and this crate
    // replaceable by a web surface later (§1).
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.status_changes();
            while let Some(next) = changes.next().await {
                if next != ConnectionStatus::Connected {
                    settings_work.with_mut(SessionSettingsWork::reset);
                }
                // The MCP Inspector flow worth copying verbatim (§9), carried
                // onto tabs: an Agent that is gone without being asked puts its
                // reason on screen. Focus elsewhere stays put; focus in the
                // Trace that is about to be replaced moves to the Diagnostics
                // tab instead of falling back to the document.
                if console::surfaces_diagnostics(next) {
                    let _ = document::eval(PRESERVE_CONSOLE_FOCUS);
                    focus.set(Focus::Wire);
                    tab.set(Tab::Diagnostics);
                }
                status.set(next);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let trace = watched.trace().clone();
        async move {
            let mut changes = trace.changes();
            while changes.next().await.is_some() {
                // The two together, from one moment: a trace at its cap drops
                // one every time it records one, and clearing moves both numbers
                // at once. Read a moment apart they would describe different
                // moments — and a frame's identity on screen is made of both
                // (`Trace::snapshot`).
                let (recorded, gone) = trace.snapshot();
                frames.set(recorded);
                dropped.set(gone);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let diagnostics = watched.diagnostics().clone();
        async move {
            let mut changes = diagnostics.changes();
            while changes.next().await.is_some() {
                let (said, gone) = diagnostics.snapshot();
                lines.set(said);
                dropped_lines.set(gone);
            }
        }
    });

    // The timeline signals a *revision* rather than a length (§11 seam 3): a
    // tool call that finished changed an entry without adding one, and a
    // subscriber watching a count would never hear about it.
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        let timeline = inspector.timeline().clone();
        async move {
            let mut changes = timeline.changes();
            while changes.next().await.is_some() {
                entries.set(timeline.entries());
                // From the same snapshot and the same announcement as the
                // entries: an entry stamped with a turn and the turn it names
                // have to reach the screen together, or it draws a group whose
                // outcome it has not been told about yet.
                turns.set(timeline.turns());
                blocked.set(waiting_on(&inspector));
            }
        }
    });

    // The turn moving is the other thing that changes who is waiting on whom: a
    // turn that ends is a turn whose blocking requests nobody will ever answer,
    // and core says so by emptying the list. Both subscriptions read the same
    // list rather than each keeping a count of their own.
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.turn_changes();
            while let Some(next) = changes.next().await {
                turn.set(next);
                blocked.set(waiting_on(&inspector));
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.session_changes();
            while let Some(next) = changes.next().await {
                settings_work.with_mut(SessionSettingsWork::reset);
                prompt_epoch += 1;
                session.set(next);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.session_settings_changes();
            while let Some(next) = changes.next().await {
                configured.set(next);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.agent_changes();
            while let Some(next) = changes.next().await {
                described.set(next);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.listing_changes();
            while let Some(next) = changes.next().await {
                listing.set(next);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.auth_changes();
            while let Some(next) = changes.next().await {
                auth.set(next);
            }
        }
    });

    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.driven_changes();
            while let Some(next) = changes.next().await {
                driven.set(next);
            }
        }
    });

    // Which captured frames the typed layer made an entry of — the answer a
    // trace row needs to offer the way back to the turn. A memo off the
    // timeline rather than a store of its own: it is a question about entries
    // this window already has, and the timeline announcing a revision is
    // exactly when the answer changes (§11 seam 3).
    let decoded = use_memo(move || {
        entries()
            .iter()
            .flat_map(|entry| entry.frames.iter().filter_map(|recorded| recorded.captured))
            .collect::<HashSet<u64>>()
    });

    // One core-owned answer drives both the listing claim and its Affordance in
    // the Agent panel. Reading the description subscribes this memo to the next
    // agent even though the protocol interpretation remains core's (§5).
    let gated = inspector.clone();
    let lists_sessions = use_memo(move || described.read().is_some() && gated.lists_sessions());

    // And which way into a session it already has, if any — the same question
    // asked of the same store, with the preference between the two calls core's
    // as well (§7.5): a window that picked between `session/load` and
    // `session/resume` would be a window deciding protocol.
    let restoring = inspector.clone();
    let restores = use_memo(move || {
        described
            .read()
            .is_some()
            .then(|| restoring.advertised_restore())
            .flatten()
    });

    // And the two ways out of one, each its own claim in its own field (§7.5).
    // Same shape as the listing gate for the same reason: reading the signal is
    // the subscription and the condition at once.
    let closing = inspector.clone();
    let closes = use_memo(move || described.read().is_some() && closing.closes_sessions());
    let deleting = inspector.clone();
    let deletes = use_memo(move || described.read().is_some() && deleting.deletes_sessions());

    // And whether a session can be opened with roots the user supplies (§7.7),
    // which is the same question again about the one advertisement here that
    // gates a field on a request rather than a method. It gates the control and
    // nothing else: a reopen hands the agent back the roots it reported whatever
    // this says, which is core's rule and not a thing this window decides.
    let rooting = inspector.clone();
    let opens_with_roots =
        use_memo(move || described.read().is_some() && rooting.opens_with_roots());

    // And the way out of a login, claimed in the same shape under `auth` rather
    // than under `sessionCapabilities` (§7.7). Gated on the advertisement and on
    // nothing else — never on where authentication stands, because a logout with
    // nobody logged in is the corner worth driving.
    let advertising_logout = inspector.clone();
    let logs_out = use_memo(move || described.read().is_some() && advertising_logout.logs_out());

    let starting = inspector.clone();
    let recording = recent.clone();
    let stopping_rail = inspector.clone();
    let stopping_palette = inspector.clone();
    let exporting_palette = inspector.clone();
    let clearing_palette = inspector.clone();
    let choosing_palette = settings.clone();
    let switching_palette = settings.clone();
    let prompting = inspector.clone();
    let cancelling = inspector.clone();
    let configuring = inspector.clone();
    let cycling = inspector.clone();
    let selecting = inspector.clone();
    let listing_agent = inspector.clone();
    let opening = inspector.clone();
    let opening_from_timeline = inspector.clone();
    let restoring_session = inspector.clone();
    let closing_session = inspector.clone();
    let deleting_session = inspector.clone();
    let authenticating = inspector.clone();
    let logging_out = inspector.clone();
    let exporting = inspector.clone();
    let clearing = inspector.clone();

    // The menu bar, answered. Every item is a control that is also on screen —
    // the strip is a second way to the window's affordances and never the only
    // one — so each arm here does exactly what the control does and decides
    // nothing of its own. The two that need an agent check for one, because a
    // menu item cannot be drawn behind an advertisement the way a button can.
    // A block rather than a hook of its own, so the four clones it needs go
    // with it where there is no menu bar to answer.
    #[cfg(feature = "desktop")]
    {
        let menu_export = inspector.clone();
        let menu_clear = inspector.clone();
        let menu_stop = inspector.clone();
        let menu_open = inspector.clone();
        shell::use_menu(move |command| match command {
            shell::command::EXPORT_TRACE => {
                exported.set(Some(
                    menu_export
                        .trace()
                        .export()
                        .save()
                        .map_err(|error| error.to_string()),
                ));
                focus.set(Focus::Wire);
                tab.set(Tab::Trace);
            }
            shell::command::CLEAR_TRACE => menu_clear.trace().clear(),
            shell::command::STOP_AGENT => menu_stop.disconnect(),
            shell::command::NEW_SESSION => {
                let Some(command) = launched() else {
                    return;
                };
                let inspector = menu_open.clone();
                failed.set(None);
                spawn(async move {
                    let outcome = inspector.open_session(&command, None).await;
                    failed.set(
                        outcome
                            .err()
                            .map(Failed::of(v1::AGENT_METHOD_NAMES.session_new)),
                    );
                });
            }
            shell::command::SHOW_TRACE => {
                focus.set(Focus::Wire);
                tab.set(Tab::Trace);
            }
            shell::command::SHOW_DIAGNOSTICS => {
                focus.set(Focus::Wire);
                tab.set(Tab::Diagnostics);
            }
            // The item that was a collapse, answering the control that replaced it.
            shell::command::TOGGLE_CONSOLE => spine.set(spine().other()),
            _ => {}
        });
    }

    // Everything about the agent that two surfaces draw: the Agent screen,
    // which says what it claimed, and the Sessions dialog, which acts on what
    // it has. One value built once — two literals would be two places for the
    // same fact to be wrong.
    let claims = agent::Claims {
        agent: described(),
        // Not a signal and not a store: the claim is fixed for
        // every connection and there is nothing that could change
        // it (§7.6), so the screen reads the value core sends.
        client: acp_inspector_core::client_capabilities(),
        client_info: acp_inspector_core::client_info(),
        driven: driven(),
        auth: auth(),
        lists_sessions: lists_sessions(),

        session: session(),
        auth_pending: auth_work().pending,
        listing: listing(),
        listing_pending: listing_work().1 > 0,
        restores: restores(),
        closes: closes(),
        deletes: deletes(),
        opens_with_roots: opens_with_roots(),
        connected: status() == ConnectionStatus::Connected,
        failed: failed(),
        logs_out: logs_out(),
        // Neither answer is waited for here either: both land in a
        // store on their way past, and this window renders stores.
        on_authenticate: EventHandler::new(move |method: v1::AuthMethodId| {
            let inspector = authenticating.clone();
            let call = AuthenticationCall::Authenticate(method.clone());
            let connection = auth_work.with_mut(|work| work.begin(call.clone()));
            spawn(async move {
                let _ = inspector.authenticate(&method).await;
                auth_work.with_mut(|work| work.finish(connection, &call));
            });
        }),
        // And the way back out of one. Nothing is waited for here
        // either, and nothing else is sent: the live session is left
        // strictly alone, because what the agent does to a session it
        // has logged out of is the finding (§7.7). What came of the
        // call is on the `logout` row above, where the record draws
        // it.
        on_logout: EventHandler::new(move |()| {
            let inspector = logging_out.clone();
            let call = AuthenticationCall::Logout;
            let connection = auth_work.with_mut(|work| work.begin(call.clone()));
            spawn(async move {
                let _ = inspector.logout().await;
                auth_work.with_mut(|work| work.finish(connection, &call));
            });
        }),
        // A second session on the agent that is already running.
        // Everything the switch costs — the turn cancelled, the
        // requests answered `cancelled`, the timeline discarded and
        // rebuilt — is core's, in the one call, because a sequence
        // in an event handler is a sequence each surface would have
        // to get right again (§5). What comes back is why there is
        // no new session, if there is none.
        // The roots ride with it, because they are part of the ask
        // rather than a setting: what the control beside the button
        // holds is what this session is opened with, and `None` is
        // an agent that advertised no control at all (§7.7).
        on_new_session: EventHandler::new(move |roots: Option<Roots>| {
            let inspector = opening.clone();
            // The invocation, not a directory: which command is
            // running is this window's to remember, and what
            // session a command opens is core's to decide (§7.1).
            // There is no agent here that was not launched from the
            // form, and the button is only drawn once one has
            // described itself, so the empty case is a shape rather
            // than a path.
            let Some(command) = launched() else {
                return;
            };
            failed.set(None);
            spawn(async move {
                let outcome = inspector.open_session(&command, roots).await;
                failed.set(
                    outcome
                        .err()
                        .map(Failed::of(v1::AGENT_METHOD_NAMES.session_new)),
                );
            });
        }),
        on_list: EventHandler::new(move |from: Option<Cursor>| {
            let inspector = listing_agent.clone();
            let connection = listing_work().0;
            listing_work.with_mut(|(_, pending)| *pending += 1);
            spawn(async move {
                let _ = inspector.list_sessions(from).await;
                listing_work.with_mut(|(current, pending)| {
                    if *current == connection {
                        *pending = pending.saturating_sub(1);
                    }
                });
            });
        }),
        // A session the agent listed, opened. Which call that takes
        // is core's answer read back out of the same memo the rows
        // were drawn from, so the button and the call cannot say
        // different things; the switch it costs is core's too, in
        // the one call (§5). What comes back is why there is no new
        // live session, if there is none.
        on_open: EventHandler::new(move |opening: Opening| {
            let inspector = restoring_session.clone();
            let Some(how) = restores() else {
                return;
            };
            failed.set(None);
            spawn(async move {
                let outcome = inspector
                    .restore_session(&opening.session, how, opening.roots)
                    .await;
                failed.set(outcome.err().map(Failed::of(how.method())));
            });
        }),
        // The two ways a session ends, and the same shape as every
        // other affordance here: one call into core, nothing waited
        // for, and what comes back is why it did not happen. The
        // listing is asked for again by core rather than from here,
        // because a call and the question it makes worth asking are
        // one sequence and sequences are core's (§5).
        on_close: EventHandler::new(move |session: v1::SessionId| {
            let inspector = closing_session.clone();
            failed.set(None);
            spawn(async move {
                let outcome = inspector.close_session(&session).await;
                failed.set(
                    outcome
                        .err()
                        .map(Failed::of(v1::AGENT_METHOD_NAMES.session_close)),
                );
            });
        }),
        on_delete: EventHandler::new(move |session: v1::SessionId| {
            let inspector = deleting_session.clone();
            failed.set(None);
            spawn(async move {
                let outcome = inspector.delete_session(&session).await;
                failed.set(
                    outcome
                        .err()
                        .map(Failed::of(v1::AGENT_METHOD_NAMES.session_delete)),
                );
            });
        }),
    };

    rsx! {
        style { {style::SHEET} }

        TopBar {
            status: status(),
            appearance: appearance(),
            indentation: indentation(),
            subject: subject(described.read().as_ref(), launched.read().as_ref()),
            protocol: described.read().as_ref().map(|agent| agent.protocol_version.to_string()),
            session: session(),
            spine: spine(),
            on_spine: move |chosen| spine.set(chosen),
            // The choice applies now and is written now. A write that fails
            // is not reported: an appearance is one click to redo, and the
            // window is already in it either way.
            on_choose: move |chosen: Appearance| {
                appearance::paint(chosen);
                appearance.set(chosen);
                let _ = choosing.set_appearance(chosen);
            },
            // The same shape, and silent about a failed write for the same
            // reason: the screens redraw from the signal, and the file is
            // where the next launch reads it from.
            on_indent: move |chosen: Indentation| {
                indentation.set(chosen);
                let _ = switching.set_indentation(chosen);
            },
        }

        // Where a session is opened, switched or ended, over the screen it
        // changes: the Timeline's own header is what opens it (`session.rs`).
        session::Sessions { claims: claims.clone() }

        // Where an agent is started, over the window rather than beside it: a
        // task done once per connection does not earn a column of every screen
        // (`connect.rs`).
        connect::Connect {
            running: status() == ConnectionStatus::Connected,
            launching: launching(),
            recent: remembered(),
            unsaved: unsaved(),
                    // Spawn, `initialize`, `session/new` — one call, because the
                    // order of the three is core's business and not a window's
                    // (§5). What comes back is the first thing that went wrong, if
                    // anything did; everything else is already in a store.
                    on_launch: move |agent: AgentCommand| {
                        let inspector = starting.clone();
                        let recent = recording.clone();
                        launching.set(true);
                        problem.set(None);
                        unsaved.set(None);
                        failed.set(None);
                        listing_work.with_mut(|(connection, pending)| {
                            *connection += 1;
                            *pending = 0;
                        });
                        auth_work.with_mut(AuthenticationWork::reset);
                        launched.set(Some(agent.clone()));
                        spawn(async move {
                            let failure = inspector.start(&agent).await.err();
                            launching.set(false);
                            // Remembered because an agent answered for itself —
                            // the command exists, it ran, and it speaks ACP.
                            // Read from the store rather than from the call
                            // succeeding, because the handshake can fail after
                            // that and still leave an invocation worth having
                            // back: an agent that will not open a session until
                            // somebody logs in (§7.1) is exactly the case, since
                            // the login happens in another terminal and coming
                            // back to four fields typed again is what this is
                            // here to stop. A command that never started never
                            // said anything — that is a typo, and it is still in
                            // the fields.
                            if inspector.agent().is_some() {
                                unsaved.set(recent.record(&agent).err().map(|error| error.to_string()));
                                remembered.set(recent.entries());
                                // The one thing that shuts the dialog which is
                                // not the reader: an agent answered, so the
                                // task it was open for is done.
                                connect::close();
                            }
                            problem.set(failure);
                        });
            },
        }

        // Everything a command in the palette can ask for is a control that is
        // also on screen (`palette.rs`), and every one of them is built from a
        // handler this window already has.
        palette::Palette { commands: commands(PaletteWiring {
            connected: status() == ConnectionStatus::Connected,
            described: described.read().is_some(),
            traced: !frames().is_empty(),
            spine: spine(),
            on_spine: EventHandler::new(move |chosen: Spine| spine.set(chosen)),
            on_tab: EventHandler::new(move |chosen: Tab| {
                focus.set(Focus::Wire);
                tab.set(chosen);
            }),
            on_rail: EventHandler::new(move |chosen: rail::Tab| rail_tab.set(chosen)),
            on_new_session: claims.on_new_session,
            on_stop: EventHandler::new(move |()| stopping_palette.disconnect()),
            on_export: EventHandler::new(move |()| {
                exported.set(Some(
                    exporting_palette.trace().export().save().map_err(|error| error.to_string()),
                ));
                focus.set(Focus::Wire);
                tab.set(Tab::Trace);
            }),
            on_clear: EventHandler::new(move |()| clearing_palette.trace().clear()),
            on_indent: EventHandler::new(move |()| {
                let chosen = Indentation::of(!indentation().indented());
                indentation.set(chosen);
                let _ = switching_palette.set_indentation(chosen);
            }),
            on_appearance: EventHandler::new(move |chosen: Appearance| {
                appearance::paint(chosen);
                appearance.set(chosen);
                let _ = choosing_palette.set_appearance(chosen);
            }),
        }) }

        div { class: focus().class(),
            Rail {
                status: status(),
                launched: launched(),
                session: session(),
                claims: claims.clone(),
                settings: configured(),
                settings_epoch: settings_work().generation,
                changing_mode: settings_work().modes > 0,
                setting_option: settings_work().option,
                commands: timeline::commands(&entries()).len(),
                tab: rail_tab(),
                on_tab: move |chosen| rail_tab.set(chosen),
                on_stop: move |()| stopping_rail.disconnect(),
                // Nothing is waited for here: what the Agent answered — an
                // acknowledgement it never restated, or a refusal — is written
                // into the Session Settings store by the task reading the
                // Connection, and this window renders stores (§7.6).
                on_set_mode: move |mode: v1::SessionModeId| {
                    let inspector = configuring.clone();
                    let generation = settings_work.with_mut(SessionSettingsWork::begin_mode);
                    spawn(async move {
                        let _ = inspector.set_mode(&mode).await;
                        settings_work.with_mut(|work| work.finish_mode(generation));
                    });
                },
                // This setter answers with the complete replacement option set,
                // which the Connection reader has put in the store before this
                // call returns (§7.6).
                on_set_config_option: move |choice: session_settings::Choice| {
                    let inspector = selecting.clone();
                    let generation = settings_work
                        .with_mut(|work| work.begin_option(choice.option.clone()));
                    spawn(async move {
                        let _ = inspector
                            .set_config_option(&choice.option, &choice.value)
                            .await;
                        settings_work.with_mut(|work| work.finish_option(generation));
                    });
                },
            }

            div { class: spine().class(),
            if spine() == Spine::Split {
            Timeline {
                prompt_epoch: prompt_epoch(),
                image_advertised: described().is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.image),
                entries,
                turn: turn(),
                session: session(),
                connected: status() == ConnectionStatus::Connected,
                held_frames: dropped() as u64..(dropped() + frames().len()) as u64,
                blocked: blocked(),
                problem: problem(),
                // The same call the Agent screen makes, offered on the screen
                // that says there is no session — and only while there is an
                // agent that has described itself, which is what `live` means
                // and what that screen's own button is drawn behind. No roots ride
                // with it: the control that supplies them is on the rail, and
                // for a session that does not exist yet *none were asked for*
                // is the true answer rather than *an empty control was*.
                on_new_session: live().map(|command| {
                    let inspector = opening_from_timeline.clone();
                    EventHandler::new(move |()| {
                        let inspector = inspector.clone();
                        let command = command.clone();
                        failed.set(None);
                        spawn(async move {
                            let outcome = inspector.open_session(&command, None).await;
                            failed
                                .set(
                                    outcome.err().map(Failed::of(v1::AGENT_METHOD_NAMES.session_new)),
                                );
                        });
                    })
                }),
                // Both of these answer with what the agent said, and neither
                // answer is waited for: the turn's state is the stream's to
                // tell, and it has already told the store by the time a call
                // returns (§11 seam 2).
                mode: cycle(&configured(), status() == ConnectionStatus::Connected),
                on_set_mode: move |mode: v1::SessionModeId| {
                    let inspector = cycling.clone();
                    let generation = settings_work.with_mut(SessionSettingsWork::begin_mode);
                    spawn(async move {
                        let _ = inspector.set_mode(&mode).await;
                        settings_work.with_mut(|work| work.finish_mode(generation));
                    });
                },
                on_prompt: move |content: Vec<v1::ContentBlock>| {
                    prompting.submit_prompt(content).map(|_| ())
                },
                on_stop: move |()| {
                    let inspector = cancelling.clone();
                    spawn(async move {
                        let _ = inspector.cancel().await;
                    });
                },
                // The request answers itself: it arrived carrying its own
                // resolver (§5), so what a click needs is the request and the
                // option, and nothing here has to find the connection it came
                // from. Not waited for, like the rest: what the panel then
                // shows is what the store says became of it.
                on_answer: move |answer: permission::Answer| {
                    spawn(async move {
                        let _ = answer.request.select(&answer.option).await;
                    });
                },
                // The same shape for the other blocking request (§7.8), and the
                // same reason it needs nothing else: three answers rather than
                // an option id, each of them a claim about what the reader did,
                // and the request knows how to send whichever it was.
                on_elicit: move |answer: elicitation::Answer| {
                    spawn(async move {
                        let _ = match answer.reply {
                            elicitation::Reply::Accept(content) => {
                                answer.request.accept(content).await
                            }
                            elicitation::Reply::Decline => answer.request.decline().await,
                            elicitation::Reply::Cancel => answer.request.cancel().await,
                        };
                    });
                },
                // A row the reader asked for from the Console, and an entry's
                // frames on their way there. Reaching the trace opens the
                // Console and selects the tab it is on: an affordance that
                // scrolled a panel nobody can see would be a control that does
                // nothing, and this is the same escalation a failed spawn makes
                // for the same reason (§9).
                turns: turns(),
                // Whether there is an agent that could open a session, which is
                // what the header's session control is drawn behind. The two
                // accounts it used to draw are the rail's now.
                described: described.read().is_some(),
                sought: sought(),
                on_reveal: move |frames: Vec<u64>| {
                    focus.set(Focus::Wire);
                    tab.set(Tab::Trace);
                    revealed.set(frames.clone());
                    trace::focus_frames(frames);
                    // What the Console last sent here is answered now, so a
                    // second click on the same row is a second reveal rather
                    // than a no-op against a signal that never changed.
                    sought.set(None);
                },
            }
            }

            // The wire and the agent's diagnostics, beside the turn: two tabs,
            // one at a time (§9, ADR 0009).
            Console {
                frames,
                dropped_frames: dropped(),
                saved: exported(),
                // Both are one call into core and nothing else. Neither is
                // spawned: an export is a snapshot taken and written now, and a
                // button whose file appears a moment later is a button that
                // cannot say where it went.
                on_export: move |()| {
                    exported
                        .set(Some(exporting.trace().export().save().map_err(|error| error.to_string())));
                },
                on_clear: move |()| clearing.trace().clear(),
                lines,
                dropped_lines: dropped_lines(),
                tab: tab(),
                on_select: move |chosen| tab.set(chosen),
                revealed,
                decoded,
                // A frame on its way back to the entry it became. It does not
                // touch the Console: what the reader is looking at here is what
                // they clicked in, and a panel that shut itself to show them the
                // answer would have taken away the question.
                on_seek: move |ordinal: u64| {
                    sought.set(Some(ordinal));
                    revealed.set(Vec::new());
                    timeline::focus_entry(ordinal);
                },
            }
            }
        }

        // The way between the three regions on a window too narrow for them,
        // drawn by the stylesheet only below the width where they stop fitting.
        div { class: "mobile-bar seg", role: "group", aria_label: "Which region is shown",
            for chosen in Focus::ALL {
                button {
                    key: "{chosen.label()}",
                    class: "seg-item",
                    "data-slot": "focus",
                    r#type: "button",
                    aria_pressed: chosen == focus(),
                    onclick: move |_| focus.set(chosen),
                    "{chosen.label()}"
                }
            }
        }

        ConsoleAnnouncement { status: status() }

        // Over all three regions and belonging to none of them, which is what
        // it is for: the copy control sits on every surface that draws bytes,
        // and what it did is the same sentence wherever it was pressed. Last in
        // the document because that is where a thing that overlaps everything
        // else belongs.
        Toast { said: announcer.said() }
    }
}

/// What the palette needs of the window: the state that decides which commands
/// there are, and the handlers they call.
///
/// One value rather than eleven arguments, and it is a *wiring* rather than a
/// state: nothing here is stored, and every field is either something the window
/// already renders from or a handler a control already calls.
struct PaletteWiring {
    connected: bool,
    described: bool,
    traced: bool,
    spine: Spine,
    on_spine: EventHandler<Spine>,
    on_tab: EventHandler<Tab>,
    on_rail: EventHandler<rail::Tab>,
    on_new_session: EventHandler<Option<Roots>>,
    on_stop: EventHandler<()>,
    on_export: EventHandler<()>,
    on_clear: EventHandler<()>,
    on_indent: EventHandler<()>,
    on_appearance: EventHandler<Appearance>,
}

/// Every command the palette offers (`palette.rs`).
///
/// **Gated exactly as the controls are.** A command that needs an agent is not
/// offered without one, for the reason a button that cannot be pressed is not
/// drawn: a list of eighteen verbs, four of which do nothing tonight, is a list
/// that has to be learnt rather than read. What is *never* gated on state the
/// protocol leaves to the agent — a session that cannot be opened, a setting
/// that will be refused — stays exactly where §7.3's rule puts it: on the
/// affordance, offered, and answered by the agent.
fn commands(wiring: PaletteWiring) -> Vec<palette::Command> {
    let PaletteWiring {
        connected,
        described,
        traced,
        spine,
        on_spine,
        on_tab,
        on_rail,
        on_new_session,
        on_stop,
        on_export,
        on_clear,
        on_indent,
        on_appearance,
    } = wiring;
    let mut commands = vec![palette::Command::new(
        "Connection",
        "Connect…",
        "Start an agent, or run one you have run before",
        EventHandler::new(move |()| connect::open()),
    )];

    if connected {
        commands.push(palette::Command::new(
            "Connection",
            "Disconnect",
            "End the connection and stop the agent",
            on_stop,
        ));
    }

    if described {
        commands.push(palette::Command::new(
            "Session",
            v1::AGENT_METHOD_NAMES.session_new,
            agent::SESSION_NEW_COSTS,
            EventHandler::new(move |()| on_new_session.call(None)),
        ));
        commands.push(palette::Command::new(
            "Session",
            "Sessions…",
            "Open, switch or end a session",
            EventHandler::new(move |()| session::open()),
        ));
    }

    commands.push(palette::Command::new(
        "Trace",
        "Export…",
        "Write every frame to a JSONL file, nothing redacted",
        on_export,
    ));
    if traced {
        commands.push(palette::Command::new(
            "Trace",
            "Clear",
            "Drop the frames captured so far; exports already written are untouched",
            on_clear,
        ));
    }

    for (chosen, says) in [
        (Tab::Trace, "Show every frame that crossed the wire"),
        (
            Tab::Diagnostics,
            "Show everything the transport said that was not a frame",
        ),
    ] {
        commands.push(palette::Command::new(
            "View",
            match chosen {
                Tab::Trace => "Trace",
                Tab::Diagnostics => "Diagnostics",
            },
            says,
            EventHandler::new(move |()| on_tab.call(chosen)),
        ));
    }
    for chosen in Spine::ALL {
        if chosen != spine {
            commands.push(palette::Command::new(
                "View",
                chosen.label(),
                chosen.says(),
                EventHandler::new(move |()| on_spine.call(chosen)),
            ));
        }
    }
    for (chosen, label, says) in [
        (
            rail::Tab::Session,
            "Session settings",
            "What the live session is configured as",
        ),
        (
            rail::Tab::Capabilities,
            "Capabilities",
            "What the agent claimed, and what this client claimed",
        ),
    ] {
        commands.push(palette::Command::new(
            "View",
            label,
            says,
            EventHandler::new(move |()| on_rail.call(chosen)),
        ));
    }

    commands.push(palette::Command::new(
        "Window",
        "Indent JSON",
        "Draw payloads indented. What crossed the wire is unchanged",
        on_indent,
    ));
    for chosen in Appearance::ALL {
        commands.push(palette::Command::new(
            "Window",
            chosen.label(),
            "How this window is painted",
            EventHandler::new(move |()| on_appearance.call(chosen)),
        ));
    }

    commands
}

/// Announces the one automatic surface change in the window without making
/// either evidence list live or taking focus from what the reader was doing.
/// The region is persistent so a status transition changes text inside it.
#[component]
fn ConsoleAnnouncement(status: ConnectionStatus) -> Element {
    let said = console::diagnostics_announcement(status);

    rsx! {
        div {
            class: "sr-only",
            "data-slot": "console-announcement",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            if let Some(said) = said {
                "{said}"
            }
        }
    }
}

/// What the window is looking at, and how to say it in one line — or `None`,
/// where it is not looking at anything.
///
/// The Agent's own account of itself where it gave one, and the invocation it
/// was started from where it did not: an agent that sent no `agentInfo` is a
/// fact about that agent, and the command is still what the reader typed.
///
/// **Nothing launched is `None` and not this tool's own name.** The name is the
/// window's, and the window now says it in its own corner (`TopBar`): a
/// connection chip reading *ACP Inspector* beside a title reading *ACP
/// Inspector* was the tool introducing itself twice and the agent nowhere. The
/// title bar still falls back to it, because a task switcher entry has to say
/// something.
fn subject(
    described: Option<&v1::InitializeResponse>,
    launched: Option<&AgentCommand>,
) -> Option<String> {
    if let Some(info) = described.and_then(|agent| agent.agent_info.as_ref()) {
        let name = info.title.clone().unwrap_or_else(|| info.name.to_string());
        return Some(format!("{name} {}", info.version));
    }

    launched
        .map(|command| {
            let command = command.command.trim();
            command
                .rsplit_once('/')
                .map(|(_, program)| program)
                .unwrap_or(command)
                .to_owned()
        })
        .filter(|program| !program.is_empty())
}

/// The bar: the tool's own mark, what it is connected to, the two facts a
/// reader checks without looking away, and the switches that are about the
/// whole window.
///
/// **Connecting is not here.** The drawing puts what a connection *is* and what
/// can be done to it in one card at the top of the rail, which is where a
/// desktop tool puts a connection it holds one of; the bar names it and says
/// whether it is up.
#[component]
fn TopBar(
    status: ConnectionStatus,
    appearance: Appearance,
    /// Whether payloads are drawn indented (§8). In the bar beside the
    /// appearance, because it is the same kind of preference: one switch, about
    /// how the whole window reads.
    indentation: Indentation,
    /// What the window is looking at: the running Agent, or nothing at all
    /// before one has been launched. The tool's own name is the mark beside it
    /// and is never the subject.
    subject: Option<String>,
    /// What version of the protocol the agent answered `initialize` with, where
    /// one has. The agent's own number and not this client's claim.
    protocol: Option<String>,
    /// The live Session, named where the window's other fact is.
    session: Option<v1::SessionId>,
    spine: Spine,
    on_spine: EventHandler<Spine>,
    on_choose: EventHandler<Appearance>,
    on_indent: EventHandler<Indentation>,
) -> Element {
    let (tone, said) = connection_state(status);

    rsx! {
        header { class: "bar",
            div { class: "bar-brand",
                span { class: "mark", aria_hidden: "true", "A" }
                h1 { "ACP Inspector" }
                span { class: "version", "v{env!(\"CARGO_PKG_VERSION\")}" }
            }
            // What is connected, as one chip: the state as a dot, the agent as
            // the name it gave, and the transport it was reached over.
            div { class: "bar-conn", "data-slot": "connection-chip",
                span { class: "dot dot-{tone}", aria_hidden: "true" }
                span { class: "sr-only", "Connection: {said}." }
                span { class: "conn-name",
                    match &subject {
                        Some(subject) => rsx! { "{subject}" },
                        None => rsx! { span { class: "cleared", "no agent" } },
                    }
                }
                span { class: "conn-meta", "stdio" }
            }

            span { class: "grow" }

            // The two protocol facts, in the register the wire is read in.
            div { class: "bar-facts",
                if let Some(protocol) = protocol {
                    span { class: "bar-protocol", "protocol v{protocol}" }
                }
                if let Some(session) = session {
                    span { class: "bar-rule", aria_hidden: "true", "|" }
                    span { class: "bar-session", "{session}" }
                }
            }

            div { class: "seg", role: "group", aria_label: "Which screens are shown",
                for chosen in Spine::ALL {
                    button {
                        key: "{chosen.label()}",
                        class: "seg-item",
                        "data-slot": "spine",
                        r#type: "button",
                        aria_pressed: chosen == spine,
                        title: chosen.says(),
                        onclick: move |_| on_spine.call(chosen),
                        "{chosen.label()}"
                    }
                }
            }

            IndentationControl { chosen: indentation, on_choose: on_indent }
            AppearanceControl { chosen: appearance, on_choose }
        }
    }
}

/// The mode the composer's own control cycles through (`composer::Cycle`).
///
/// **Read from the Session Settings store and nowhere else** (§7.6): what is
/// shown is the mode the agent last stated, and what the control asks for is
/// the next mode *the agent published*. An agent that published none gets no
/// control, which is the same gate the rail's rows are drawn behind.
fn cycle(settings: &SessionSettings, connected: bool) -> Option<composer::Cycle> {
    let modes = settings.modes.as_ref()?;
    let offered = &modes.available_modes;
    let at = offered
        .iter()
        .position(|mode| mode.id == modes.current_mode_id);
    let next = at
        .and_then(|at| offered.get((at + 1) % offered.len().max(1)))
        .or_else(|| offered.first())
        .map(|mode| mode.id.clone())
        // One published mode is a control with nowhere to go, and the row on
        // the rail is where a reader sets it to itself if they want to watch
        // the agent answer that.
        .filter(|_| offered.len() > 1);

    Some(composer::Cycle {
        id: modes.current_mode_id.to_string(),
        available: connected && next.is_some(),
        next,
    })
}

/// The Connection, in the tone the window draws it and the word core gives it.
///
/// One table, because three surfaces say it: the chip in the bar, the card in
/// the rail, and the sentence a screen reader hears.
fn connection_state(status: ConnectionStatus) -> (&'static str, &'static str) {
    match status {
        ConnectionStatus::Connected => ("live", "connected"),
        ConnectionStatus::FailedToStart => ("gone", "failed to start"),
        ConnectionStatus::Lost => ("gone", "connection lost"),
        ConnectionStatus::Disconnected => ("idle", "disconnected"),
        // `ConnectionStatus` is non-exhaustive. A state this window has not
        // heard of must not be drawn as nothing, and must not be drawn as one
        // of the states it *has* heard of either.
        _ => ("idle", "unrecognized state"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console_announcement(status: ConnectionStatus) -> String {
        let mut dom =
            VirtualDom::new_with_props(ConsoleAnnouncement, ConsoleAnnouncementProps { status });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn automatic_diagnostics_navigation_is_announced_without_taking_focus() {
        let quiet = console_announcement(ConnectionStatus::Disconnected);
        assert!(
            quiet.contains(r#"data-slot="console-announcement""#)
                && quiet.contains(r#"role="status""#)
                && quiet.contains(r#"aria-live="polite""#)
                && quiet.contains(r#"aria-atomic="true""#)
                && !quiet.contains("Diagnostics opened"),
            "the persistent region starts empty: {quiet}"
        );

        for (status, words) in [
            (
                ConnectionStatus::FailedToStart,
                "Failed to start. Diagnostics opened.",
            ),
            (
                ConnectionStatus::Lost,
                "Connection lost. Diagnostics opened.",
            ),
        ] {
            let html = console_announcement(status);
            assert!(
                html.contains(words),
                "the automatic surface change is explicit: {html}"
            );
        }

        assert!(
            PRESERVE_CONSOLE_FOCUS.contains("console-tab-trace")
                && PRESERVE_CONSOLE_FOCUS.contains(r#"closest('[data-slot="trace"]')"#)
                && PRESERVE_CONSOLE_FOCUS.contains("console-tab-diagnostics")
                && PRESERVE_CONSOLE_FOCUS.contains(".focus()"),
            "only focus already inside the replaced Console surface is transferred: {PRESERVE_CONSOLE_FOCUS}"
        );
    }

    #[test]
    fn the_bar_names_what_the_window_is_looking_at() {
        let command = AgentCommand {
            command: "/opt/agents/bin/testy".to_owned(),
            args: String::new(),
            env: String::new(),
            cwd: String::new(),
        };

        // Nothing launched: the tool's own name, which is the only subject
        // there is.
        assert_eq!(subject(None, None), None);

        // Launched, and the agent has not described itself yet — or described
        // itself without an `agentInfo`, which is a fact about that agent. The
        // command is what the reader typed and is still the subject.
        assert_eq!(subject(None, Some(&command)).as_deref(), Some("testy"));

        // And once it has said who it is, it is who it says it is.
        let described = v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1)
            .agent_info(v1::Implementation::new("testy", "1.2.3").title("Testy"));
        assert_eq!(
            subject(Some(&described), Some(&command)).as_deref(),
            Some("Testy 1.2.3")
        );

        // The title is optional in the protocol, and the name is what is left.
        let unnamed = v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1)
            .agent_info(v1::Implementation::new("testy", "1.2.3"));
        assert_eq!(
            subject(Some(&unnamed), None).as_deref(),
            Some("testy 1.2.3")
        );
    }

    #[test]
    fn the_bar_names_the_tool_once_and_the_agent_beside_it() {
        #[component]
        fn Host(status: ConnectionStatus, subject: Option<String>) -> Element {
            rsx! {
                TopBar {
                    status,
                    appearance: Appearance::System,
                    indentation: Indentation::Wire,
                    subject,
                    protocol: Some("1".to_owned()),
                    session: None,
                    spine: Spine::Split,
                    on_spine: move |_| {},
                    on_choose: move |_| {},
                    on_indent: move |_| {},
                }
            }
        }

        fn bar(status: ConnectionStatus, subject: Option<&str>) -> String {
            let mut dom = VirtualDom::new_with_props(
                Host,
                HostProps {
                    status,
                    subject: subject.map(str::to_owned),
                },
            );
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        }

        let html = bar(ConnectionStatus::Disconnected, None);

        // **The tool names itself once, in its own corner**: the mark and the
        // title are the window's identity, and everything to the right of them
        // is about the agent.
        assert!(
            html.contains(r#"class="bar""#)
                && html.contains(r#"class="mark""#)
                && html.contains("<h1>ACP Inspector</h1>")
                && html.matches("ACP Inspector").count() == 1,
            "the window says what it is once: {html}"
        );
        // The chip carries the connection: a dot, the state in words for a
        // reader who cannot see it, and the transport it was reached over.
        assert!(
            html.contains(r#"data-slot="connection-chip""#)
                && html.contains(r#"class="dot dot-idle""#)
                && html.contains("Connection: disconnected.")
                && html.contains("stdio"),
            "the chip says what is connected and how: {html}"
        );
        assert!(
            html.contains("no agent"),
            "with nothing launched it says so rather than naming this tool: {html}"
        );
        let running = bar(ConnectionStatus::Connected, Some("Testy 1.2.3"));
        assert!(
            running.contains(r#"class="dot dot-live""#)
                && running.contains("Testy 1.2.3")
                && running.contains("Connection: connected."),
            "and with an agent it is the agent's own name: {running}"
        );

        // The two protocol facts, and neither drawn when there is nothing to
        // say.
        assert!(
            html.contains("protocol v1") && !html.contains(r#"class="bar-session""#),
            "the bar names a session only where there is one: {html}"
        );

        // Which screens are on, as a segmented control: two exclusive states,
        // one of which is always true.
        assert!(
            html.contains(r#"data-slot="spine""#)
                && html.contains(">Split</button>")
                && html.contains(">JSON-RPC full</button>")
                && html.contains(r#"aria-pressed=true"#),
            "the spine says which screens the window is spending its width on: {html}"
        );

        // And the two preferences beside it, each a group of exclusive answers
        // with the whole name on every one of them.
        assert!(
            html.contains(r#"data-slot="appearance""#)
                && html.contains(r#"aria-label="System appearance""#)
                && html.contains(r#"data-slot="indentation""#)
                && html.contains(r#"aria-label="wire payloads""#),
            "appearance and indentation are named segmented groups: {html}"
        );

        // **Connecting is not in the bar.** What a connection is and what can
        // be done to it is one card at the top of the rail (ADR 0009).
        assert!(
            !html.contains("Connect…") && !html.contains("Disconnect"),
            "the bar names the connection and does not act on it: {html}"
        );
    }
}
