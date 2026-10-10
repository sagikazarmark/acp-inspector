//! The window's copy of core's stores, and the subscriptions that keep it
//! current.

use acp_inspector_core::{
    AuthState, ConnectionStatus, Diagnostic, DrivenRecord, Inspector, Retention, SessionListing,
    SessionSettings, TimelineEntry, TracedFrame, Turn, TurnState, v1,
};
use dioxus::prelude::*;

use super::layout::Focus;
use super::settings::SessionSettingsWork;
use crate::console::{self, Tab};
use crate::navigation::Navigation;

/// Keep an automatic Console surface change from orphaning focus in Trace. A
/// reader elsewhere in the window stays where they are; one already using the
/// Console moves to the tab that names the surface replacing Trace.
pub(crate) const PRESERVE_CONSOLE_FOCUS: &str = r#"
const active = document.activeElement;
if (active?.id === "console-tab-trace" || active?.closest('[data-slot="trace"]')) {
  document.getElementById("console-tab-diagnostics")?.focus();
}
"#;

/// Whether the agent is waiting on the reader for anything (§7.2, §7.8).
///
/// One question over both lists, because what the screen says about it is one
/// sentence: which kind of request stopped the tool is not a distinction the
/// reader's next action turns on. Core's lists rather than a scan of the rows,
/// which is the rule the flag already followed for one of them.
fn waiting_on(inspector: &Inspector) -> bool {
    !inspector.pending_permissions().is_empty()
        || !inspector.pending_elicitations().is_empty()
        || !inspector.pending_services().is_empty()
}

/// What the window renders from: one signal per store core keeps.
#[derive(Clone, Copy)]
pub(crate) struct Stores {
    pub status: Signal<ConnectionStatus>,
    pub frames: Signal<Vec<TracedFrame>>,
    // What the trace no longer holds, and where the last export went. Both are
    // the trace's own answers (§10): how many frames a trace keeps and where an
    // export is written are core's decisions, and this window is where they are
    // read out.
    pub dropped: Signal<usize>,
    pub retention: Signal<Retention>,
    pub lines: Signal<Vec<Diagnostic>>,
    // What the Console's bounded Diagnostic channel no longer holds;
    // it is half of what a line's identity is
    // made of, and a console that stopped tracking it would break silently on
    // the day the log is bounded rather than loudly now.
    pub dropped_lines: Signal<usize>,
    pub entries: Signal<Vec<TimelineEntry>>,
    // And what each turn of this session came to, which the entries are grouped
    // by (§9). Its own signal beside them rather than a second reading of the
    // turn *state*: that one is where the live turn stands and is replaced by
    // the next prompt, and this is the record of the ones that are over.
    pub turns: Signal<Vec<Turn>>,
    pub turn: Signal<TurnState>,
    // Whether the agent is waiting on an answer (§7.2). Core's list of pending
    // requests, asked as one question — never worked out from the rows on
    // screen, because core is where a request stops being answerable and a
    // window recomputing it would drift the moment it got one case wrong.
    pub blocked: Signal<bool>,
    pub session: Signal<Option<v1::SessionId>>,
    pub prompt_epoch: Signal<u64>,
    // What that session is configured as (§7.6). Its own store rather than
    // something read off the session: the settings are filled from the setup
    // response and kept current by what the agent announces afterwards, and both
    // of those are core's to decide.
    pub configured: Signal<SessionSettings>,
    // The agent's own account of itself, which the capability display is a
    // rendering of — and the two things that hang off it: what it answered when
    // asked for its sessions, and where authentication stands (§9).
    pub described: Signal<Option<v1::InitializeResponse>>,
    pub listing: Signal<Option<SessionListing>>,
    pub auth: Signal<AuthState>,
    // And what became of driving what it claimed (§7.7): the fourth fact each
    // capability row carries. Its own store beside the claim rather than
    // something read off the trace — the trace is a bounded ring, and a record
    // derived from one would report *not driven* about an agent that was driven
    // on exactly the long connection where somebody wants to read it.
    pub driven: Signal<DrivenRecord>,
}

impl Stores {
    /// Whether there is a live connection.
    pub fn connected(&self) -> bool {
        (self.status)() == ConnectionStatus::Connected
    }
}

/// Subscribes to every store.
pub(crate) fn use_stores(
    inspector: &Inspector,
    mut settings_work: Signal<SessionSettingsWork>,
    navigation: Navigation,
) -> Stores {
    let mut status = use_signal(|| ConnectionStatus::Disconnected);
    let mut frames = use_signal(Vec::<TracedFrame>::new);
    let mut dropped = use_signal(|| 0usize);
    let mut retention = use_signal(Retention::default);
    let mut lines = use_signal(Vec::<Diagnostic>::new);
    let mut dropped_lines = use_signal(|| 0usize);
    let mut entries = use_signal(Vec::<TimelineEntry>::new);
    let mut turns = use_signal(Vec::<Turn>::new);
    let mut turn = use_signal(TurnState::default);
    let mut blocked = use_signal(|| false);
    let mut session = use_signal(|| None::<v1::SessionId>);
    let mut prompt_epoch = use_signal(|| 0_u64);
    let mut configured = use_signal(SessionSettings::default);
    let mut described = use_signal(|| None::<v1::InitializeResponse>);
    let mut listing = use_signal(|| None::<SessionListing>);
    let mut auth = use_signal(AuthState::default);
    let mut driven = use_signal(DrivenRecord::default);
    let mut focus = navigation.region;
    let mut tab = navigation.console;

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
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
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
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
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
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let (held, spans, holes) = timeline.snapshot();
                entries.set(held);
                retention.set(holes);
                // From the same snapshot and the same announcement as the
                // entries: an entry stamped with a turn and the turn it names
                // have to reach the screen together, or it draws a group whose
                // outcome it has not been told about yet.
                turns.set(spans);
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

    Stores {
        status,
        frames,
        dropped,
        retention,
        lines,
        dropped_lines,
        entries,
        turns,
        turn,
        blocked,
        session,
        prompt_epoch,
        configured,
        described,
        listing,
        auth,
        driven,
    }
}
