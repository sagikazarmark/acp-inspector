//! The window's copy of core's stores, and the subscriptions that keep it
//! current.

use acp_inspector_core::{
    AuthState, ConnectionStatus, Diagnostic, DrivenRecord, Inspector, Retention, SessionListing,
    SessionReports, SessionSettings, TimelineEntry, TracedFrame, Turn, TurnState, v1,
};
use dioxus::prelude::*;

use super::layout::Focus;
use super::settings::SessionSettingsWork;
use crate::console::{self, Tab};
use crate::navigation::Navigation;

/// Moves focus to the Diagnostics tab only if it was in the Trace being
/// replaced; focus elsewhere stays put.
pub(crate) const PRESERVE_CONSOLE_FOCUS: &str = r#"
const active = document.activeElement;
if (active?.id === "console-tab-trace" || active?.closest('[data-slot="trace"]')) {
  document.getElementById("console-tab-diagnostics")?.focus();
}
"#;

/// Whether the agent is waiting on the reader for anything, from core's pending
/// lists rather than the rows on screen.
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
    /// Frames the trace no longer holds.
    pub dropped: Signal<usize>,
    pub retention: Signal<Retention>,
    pub lines: Signal<Vec<Diagnostic>>,
    /// Diagnostic lines the log no longer holds; half of a line's identity.
    pub dropped_lines: Signal<usize>,
    pub entries: Signal<Vec<TimelineEntry>>,
    /// What each finished turn came to, which entries are grouped by. Not the
    /// live turn state, which the next prompt replaces.
    pub turns: Signal<Vec<Turn>>,
    pub turn: Signal<TurnState>,
    /// Whether the agent is waiting on an answer.
    pub blocked: Signal<bool>,
    pub session: Signal<Option<v1::SessionId>>,
    pub prompt_epoch: Signal<u64>,
    pub configured: Signal<SessionSettings>,
    /// What the agent has said about each session's title and activity.
    pub reports: Signal<SessionReports>,
    pub described: Signal<Option<v1::InitializeResponse>>,
    pub listing: Signal<Option<SessionListing>>,
    pub auth: Signal<AuthState>,
    /// What became of driving each claim. Its own store because the trace is a
    /// bounded ring and could not answer this on a long connection.
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
    let mut reports = use_signal(SessionReports::default);
    let mut described = use_signal(|| None::<v1::InitializeResponse>);
    let mut listing = use_signal(|| None::<SessionListing>);
    let mut auth = use_signal(AuthState::default);
    let mut driven = use_signal(DrivenRecord::default);
    let mut focus = navigation.region;
    let mut tab = navigation.console;

    // One subscription per store: wait for a change, then re-read. Nothing
    // polls, and core never pushes into the UI.
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.status_changes();
            while let Some(next) = changes.next().await {
                if next != ConnectionStatus::Connected {
                    settings_work.with_mut(SessionSettingsWork::reset);
                }
                // An agent gone without being asked puts its reason on screen.
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
                // Frames and the dropped count come from one snapshot: a frame's
                // identity on screen is made of both.
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

    // The timeline signals a revision, not a length: a merged update changes an
    // entry without adding one.
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
                // Same snapshot as the entries, so an entry and its turn reach
                // the screen together.
                turns.set(spans);
                blocked.set(waiting_on(&inspector));
            }
        }
    });

    // A turn that ends empties core's pending list, so recheck on turn changes.
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
            let mut changes = inspector.session_reports_changes();
            while let Some(next) = changes.next().await {
                reports.set(next);
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
        reports,
        described,
        listing,
        auth,
        driven,
    }
}
