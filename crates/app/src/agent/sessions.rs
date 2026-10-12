//! The Sessions dialog: the live session, the listing, and the ways in and out.
//! See `docs/architecture.md` §7.5.

use acp_inspector_core::{
    Cursor, Restore, Roots, SessionListing, SessionReports, Stated, StatedBy, v1,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdPlus, LdRefreshCw},
};

use super::roots::{Reopen, roots_field, roots_rule};
use super::{Failed, Opening};

/// The session lifecycle affordances, and whatever the agent last answered into
/// them: the current session, the other listed sessions, and a new session at
/// the foot (with its roots, where `additionalDirectories` was advertised).
///
/// The live session is shown whether or not it was listed; the calls that end
/// it need only its id.
pub(crate) fn sessions(
    listing: Listing<'_>,
    connected: bool,
    failed: Option<&Failed>,
    creating: Signal<String>,
    on_new_session: EventHandler<Option<Roots>>,
) -> Element {
    let roots = listing.roots;
    let on_list = listing.on_list;
    // The listed sessions, split into the live one and the others.
    let answered = listing
        .answered
        .map(|answered| answered.sessions.as_slice());
    // While a listing is in flight the live row keeps its id but not the
    // description, which came from the page being replaced.
    let current = (!listing.pending)
        .then(|| {
            answered.and_then(|sessions| {
                sessions
                    .iter()
                    .find(|session| Some(&session.session_id) == listing.live)
            })
        })
        .flatten();
    // Each session once: pages are appended as they come, so one that moved
    // between pages while they were fetched is listed twice. The rows are
    // keyed by id, and every row's actions address the session by it anyway.
    let mut seen = std::collections::HashSet::new();
    let others: Vec<&v1::SessionInfo> = answered
        .unwrap_or_default()
        .iter()
        .filter(|session| Some(&session.session_id) != listing.live)
        .filter(|session| seen.insert(&session.session_id))
        .collect();

    rsx! {
        // The dialog's body holds this and the MCP editor, so both get its
        // padding and scroll together.
        div { class: "sessions-content", "data-slot": "sessions",
            // The listing outlives the agent on purpose (§5); say why its
            // controls are unavailable.
            if !connected {
                p {
                    class: "hint availability",
                    id: UNAVAILABLE,
                    "data-slot": "availability",
                    role: "status",
                    "Disconnected. These are the sessions the last agent reported; start an agent to act on them."
                }
            }

            // From the listing where there is one, else from the live id alone.
            if let Some(live) = listing.live {
                section { class: "rail-group", "data-slot": "session-current",
                    div { class: "rail-head",
                        h3 { class: "heading", "Current session" }
                    }
                    ul { class: "session-list",
                        {session_row(current, live, &listing, connected)}
                    }
                }
            }

            // Gated on the `session/list` advertisement (§7.5).
            if listing.advertised {
                section { class: "rail-group session-listing",
                    header { class: "rail-head",
                        h3 { class: "heading", "Other sessions" }
                        if !others.is_empty() {
                            span { class: "rail-count", "{others.len()}" }
                        }
                        button {
                            class: "btn btn-xs btn-quiet",
                            r#type: "button",
                            title: "session/list",
                            aria_disabled: (!connected).then_some("true"),
                            aria_describedby: (!connected).then_some(UNAVAILABLE),
                            onclick: move |_| if connected { on_list.call(None) },
                            span { class: "disclosure-icon", aria_hidden: "true",
                                Icon { class: "icon", width: 16, height: 16, icon: LdRefreshCw }
                            }
                            "List sessions"
                        }
                    }
                    {listed(&listing, &others, connected)}
                }
            }

            // The failure names its method: any of the four calls here may fail
            // on its own.
            if let Some(failed) = failed {
                p { class: "alert alert-error", "data-slot": "session-failure", role: "alert",
                    code { "{failed.method}" }
                    " failed: {failed.error}"
                }
            }

            footer { class: "rail-group",
                div { class: "session-actions",
                    button {
                        class: "btn btn-sm btn-filled",
                        r#type: "button",
                        aria_disabled: (!connected).then_some("true"),
                        // The label is the verb; the wire name and cost are the tooltip.
                        title: "{v1::AGENT_METHOD_NAMES.session_new}: {SESSION_NEW_COSTS}",
                        aria_describedby: if connected { "session-new-costs".to_owned() } else { format!("session-new-costs {UNAVAILABLE}") },
                        // `None` where no roots control was advertised: none were asked for.
                        onclick: move |_| if connected { on_new_session.call(roots.then(|| Roots::supplied(creating()))) },
                        span { class: "disclosure-icon", aria_hidden: "true",
                            Icon { class: "icon", width: 16, height: 16, icon: LdPlus }
                        }
                        "New session"
                    }

                    if roots {
                        details { class: "disclose bare roots-new",
                            summary {
                                aria_label: "Edit additionalDirectories for a new session",
                                title: "Edit additionalDirectories for a new session",
                                {crate::disclosure::chevron()}
                                code { "additionalDirectories" }
                            }
                            div { class: "disclose-body",
                                {roots_rule()}
                                {roots_field(
                                    creating,
                                    false,
                                    "additionalDirectories for a new session".to_owned(),
                                )}
                            }
                        }
                    }
                }

                // The tooltip's sentence, in the document for `aria-describedby`.
                p { class: "sr-only", id: "session-new-costs", {SESSION_NEW_COSTS} }
            }
        }
    }
}

/// One session, as a row of either list, with every advertised affordance.
///
/// `info` is `None` for a live session the agent has not listed: no reopen
/// (which needs the agent's description), only close and delete.
fn session_row(
    info: Option<&v1::SessionInfo>,
    id: &v1::SessionId,
    sessions: &Listing<'_>,
    connected: bool,
) -> Element {
    let Listing {
        live,
        restores,
        closes,
        deletes,
        roots,
        on_open,
        on_close,
        on_delete,
        ..
    } = *sessions;
    let current = live == Some(id);
    let title = sessions.reports.title(id, info);
    let updated = sessions.reports.updated_at(id, info);
    // The listing's title, where an update outranked it with another one.
    let relisted = info.map(|info| info.title.clone()).filter(|listed| {
        title
            .as_ref()
            .is_some_and(|title| title.by == StatedBy::Update && title.value != *listed)
    });

    rsx! {
        li {
            key: "{id}",
            class: if current { "session-row live" } else { "session-row" },
            div { class: "session-head",
                span {
                    class: if current { "dot dot-live" } else { "dot dot-idle" },
                    aria_hidden: "true",
                }
                span { class: "session-title",
                    title: title.as_ref().map(|title| stated_by(title.by)),
                    match title.as_ref().and_then(|title| title.value.as_ref()) {
                        Some(title) => rsx! { "{title}" },
                        None => rsx! { span { class: "cleared", "untitled session" } },
                    }
                }
                // The dot is decoration; say which row this is in words.
                span { class: "sr-only",
                    if current { "Current session. " } else { "Available session. " }
                }
                span { class: "session-meta",
                    if let Some(Stated { value: Some(when), by }) = updated.as_ref() {
                        span { title: stated_by(*by), "updated {when}" }
                    } else if info.is_none() && !sessions.pending {
                        "not in a listing"
                    }
                    // Where an update renamed a session the listing still
                    // names otherwise, both claims are shown.
                    if let Some(listed) = relisted {
                        span { class: "session-relisted", "data-slot": "relisted",
                            title: "{v1::AGENT_METHOD_NAMES.session_list}",
                            match listed {
                                Some(listed) => rsx! { "listed as “{listed}”" },
                                None => rsx! { "listed untitled" },
                            }
                        }
                    }
                }
            }
            div { class: "session-line",
                code { class: "session-id", "{id}" }
                if let Some(info) = info {
                    span { class: "session-id", title: "{info.cwd.display()}", "{info.cwd.display()}" }
                }
                div { class: "session-actions",
                if let (Some(info), Some(how)) = (info, restores) {
                    Reopen {
                        session: info.clone(),
                        how,
                        roots,
                        connected,
                        on_open,
                    }
                }
                // Close and delete are each gated on their own claim and never
                // hidden. Unavailable controls stay focusable and say why through
                // the availability line.
                if closes {
                    button {
                        class: "btn btn-xs btn-quiet session-end",
                        r#type: "button",
                        title: "{v1::AGENT_METHOD_NAMES.session_close}",
                        aria_label: "Close session {id}",
                        aria_describedby: described_unless(connected, ENDING),
                        aria_disabled: (!connected).then_some("true"),
                        onclick: {
                            let id = id.clone();
                            move |_| if connected { on_close.call(id.clone()) }
                        },
                        "Close"
                    }
                }
                if deletes {
                    DeleteSession { id: id.clone(), connected, on_delete }
                }
                }
            }
        }
    }
}

/// Where a session's title or activity was stated, for the tooltip on it.
pub(crate) fn stated_by(by: StatedBy) -> String {
    match by {
        StatedBy::Update => "From session_info_update".to_owned(),
        StatedBy::Listing => format!("From {}", v1::AGENT_METHOD_NAMES.session_list),
    }
}

/// The id of the line that says why the dialog's controls are unavailable.
const UNAVAILABLE: &str = "sessions-unavailable";

/// What a session control is described by: its consequence, and while there is
/// no agent, why it cannot be used.
pub(super) fn described_unless(connected: bool, consequence: &str) -> String {
    if connected {
        consequence.to_owned()
    } else {
        format!("{consequence} {UNAVAILABLE}")
    }
}

/// How long an armed delete waits for its confirmation before it disarms.
const CONFIRM_FOR: std::time::Duration = std::time::Duration::from_secs(6);

/// `session/delete`, confirmed in two steps because it cannot be undone.
///
/// The first press arms it, the second sends; Cancel, Escape, or
/// [`CONFIRM_FOR`] of nothing disarm it. The label changes on the same button,
/// so focus stays put.
#[component]
fn DeleteSession(
    id: v1::SessionId,
    connected: bool,
    on_delete: EventHandler<v1::SessionId>,
) -> Element {
    let mut armed = use_signal(|| false);
    // Which arming the timer belongs to, so an old timer cannot disarm a
    // newer press.
    let mut arming = use_signal(|| 0_u64);
    let mut arm = move || {
        armed.set(true);
        let this = arming() + 1;
        arming.set(this);
        spawn(async move {
            tokio::time::sleep(CONFIRM_FOR).await;
            if arming() == this {
                armed.set(false);
            }
        });
    };
    let deleting = id.clone();

    rsx! {
        span { class: "session-delete", "data-slot": "session-delete", "data-armed": "{armed()}",
            onkeydown: move |event| {
                if event.key() == Key::Escape && armed() {
                    event.stop_propagation();
                    armed.set(false);
                }
            },
            button {
                class: if armed() { "btn btn-xs btn-filled btn-bad session-end confirm" } else { "btn btn-xs btn-quiet btn-bad session-end" },
                r#type: "button",
                title: "{v1::AGENT_METHOD_NAMES.session_delete}",
                aria_label: if armed() { format!("Confirm delete session {id}") } else { format!("Delete session {id}") },
                aria_describedby: described_unless(connected, ENDING),
                aria_disabled: (!connected).then_some("true"),
                onclick: move |_| {
                    if !connected {
                        return;
                    }
                    if armed() {
                        armed.set(false);
                        on_delete.call(deleting.clone());
                    } else {
                        arm();
                    }
                },
                if armed() { "Confirm delete" } else { "Delete" }
            }
            if armed() {
                button {
                    class: "btn btn-xs btn-quiet",
                    r#type: "button",
                    "data-slot": "cancel-delete",
                    aria_label: "Keep session {id}",
                    onclick: move |_| armed.set(false),
                    "Cancel"
                }
            }
        }
    }
}

/// What the listing is drawn from: what the agent advertised, what it last
/// answered, and the handlers for its rows.
#[derive(Clone, Copy)]
pub(crate) struct Listing<'a> {
    /// Whether the agent advertised `session/list` (core's answer).
    pub(super) advertised: bool,
    /// What it answered the last time it was asked, if it has been.
    pub(super) answered: Option<&'a SessionListing>,
    /// What it said about each session in `session_info_update`, which
    /// outranks the listing's description.
    pub(super) reports: &'a SessionReports,
    /// The live session.
    pub(super) live: Option<&'a v1::SessionId>,
    /// Whether the answer, or its next page, is currently in flight.
    pub(super) pending: bool,
    /// How a listed session is opened, or `None` where the agent advertised no
    /// way to.
    pub(super) restores: Option<Restore>,
    /// Whether `session/close` and `session/delete` were advertised: two
    /// separate claims (§7.5).
    pub(super) closes: bool,
    pub(super) deletes: bool,
    /// Whether `additionalDirectories` was advertised, gating every roots
    /// control (§7.7).
    pub(super) roots: bool,
    pub(super) on_list: EventHandler<Option<Cursor>>,
    pub(super) on_open: EventHandler<Opening>,
    pub(super) on_close: EventHandler<v1::SessionId>,
    pub(super) on_delete: EventHandler<v1::SessionId>,
}

/// The listed sessions other than the live one, or the state standing in for
/// them.
///
/// One reopen per row: load and resume differ only in replay, which the call
/// carries ([`Restore`]) and [`opening`] states (§7.5).
fn listed(sessions: &Listing<'_>, others: &[&v1::SessionInfo], connected: bool) -> Element {
    let Listing {
        answered,
        pending,
        restores,
        closes,
        deletes,
        on_list,
        ..
    } = *sessions;
    let next_page = answered.and_then(|listing| listing.next_page.clone());

    rsx! {
        if pending {
            p {
                class: "alert",
                "data-slot": "session-list-status",
                role: "status",
                aria_live: "polite",
                span { class: "status status-info", aria_hidden: "true" }
                "Loading sessions."
                if answered.is_some() {
                    " Previous rows are hidden until the agent answers."
                }
            }
        } else { match answered {
            // Advertised and not yet asked: a plain line, not an alert.
            None => rsx! {
                p { class: "hint", "data-slot": "session-list-status", role: "status", "The agent says it can list its sessions." }
            },
            Some(listing) if listing.sessions.is_empty() => rsx! {
                p { class: "alert", "data-slot": "session-list-status", role: "status", aria_live: "polite", "The agent listed no sessions." }
            },
            // Only the live session was listed; said so an empty region is not
            // read as a failure.
            Some(_) if others.is_empty() => rsx! {
                p { class: "hint", "data-slot": "session-list-status", role: "status", aria_live: "polite",
                    "The agent listed no sessions besides this one."
                }
            },
            Some(_) => rsx! {
                {opening(restores)}
                {ending(closes, deletes)}
                ul { class: "session-list",
                    for session in others.iter().copied() {
                        {session_row(Some(session), &session.session_id, sessions, connected)}
                    }
                }
            },
        } }

        // The cursor goes back to core unread: it is opaque.
        SessionPaging {
            available: next_page.is_some(),
            connected,
            on_next: move |()| on_list.call(next_page.clone()),
        }
    }
}

/// The next-page button. Availability is separate from the callback so tests
/// need not build the opaque [`Cursor`] it closes over.
#[component]

fn SessionPaging(available: bool, connected: bool, on_next: EventHandler<()>) -> Element {
    rsx! {
        if available {
            div { class: "session-line",
                span { class: "hint",
                    if connected {
                        "The agent has more sessions."
                    } else {
                        "The agent has more sessions. Next page is unavailable while disconnected."
                    }
                }
                button {
                    class: "btn btn-xs btn-quiet next-page",
                    r#type: "button",
                    title: "session/list",
                    aria_label: "Ask the agent for the next page of sessions",
                    aria_disabled: (!connected).then_some("true"),
                    onclick: move |_| if connected { on_next.call(()) },
                    "Next page"
                }
            }
        }
    }
}

/// What pressing `session/new` costs: the button's tooltip and its description.
pub(crate) const SESSION_NEW_COSTS: &str = "Opens and switches to a new session. A turn still running is cancelled first; the trace keeps every frame.";

/// The ids of what the row's reopen and end controls cost, which those controls
/// are described by.
pub(crate) const OPENING: &str = "session-open-costs";

pub(crate) const ENDING: &str = "session-end-costs";

/// What opening a listed session will do, said once above them all.
///
/// Whether the conversation replays decides whether the timeline is rebuilt, so
/// it is stated: after a resume an empty timeline is correct. Decided by
/// [`Restore::replays`], not by the method name.
fn opening(restores: Option<Restore>) -> Element {
    rsx! {
        p {
            class: if restores.is_some() { "sr-only" } else { "hint" },
            id: OPENING,
            {match restores {
                None => rsx! {
                    "The agent advertised neither "
                    code { "session/load" }
                    " nor "
                    code { "session/resume" }
                    ", so there is no way in to a session it already has."
                },
                // On the property, since `Restore` is non-exhaustive.
                Some(how) if how.replays() => rsx! {
                    "Opening one makes it the live session. "
                    code { "{how.method()}" }
                    " replays its conversation into the timeline before it answers, so what was said comes back with it."
                },
                Some(how) => rsx! {
                    "Opening one makes it the live session. "
                    strong { "This agent can't replay earlier messages" }
                    ": "
                    code { "{how.method()}" }
                    " reopens a session without them, so the timeline starts empty."
                },
            }}
        }
        if restores.is_some() {
            p { class: "sr-only",
                "It is the switch "
                code { "session/new" }
                " is: a turn still running is cancelled first, and the timeline starts again — the trace keeps every frame."
            }
        }
    }
}

/// What ending a listed session will do, said once above them all (§7.5): the
/// call is sent whatever the session's state, the list is refreshed after, and
/// ending the live session leaves none open. It promises no outcome, since the
/// specification defines none.
fn ending(closes: bool, deletes: bool) -> Element {
    let named = match (closes, deletes) {
        (true, true) => "session/close and session/delete",
        (true, false) => "session/close",
        (false, true) => "session/delete",
        (false, false) => return rsx! {},
    };

    rsx! {
        p { class: "sr-only", id: ENDING,
            code { "{named}" }
            " is sent whatever state the session is in, and the list is refreshed afterwards."
            " Ending the live session leaves no session open; the trace keeps every frame."
        }
    }
}

#[cfg(test)]
mod tests {
    use acp_inspector_core::{AuthState, CallError};

    use super::*;
    use crate::agent::fixtures::*;

    #[tokio::test]
    async fn deleting_a_session_takes_a_second_press_and_can_be_called_off() {
        use std::{any::Any, rc::Rc};

        use dioxus::core::{AttributeValue, Mutation, NoOpMutations};
        use dioxus::html::{PlatformEventData, SerializedMouseData};

        #[component]
        fn Host() -> Element {
            let mut deleted = use_signal(|| 0);
            rsx! {
                DeleteSession {
                    id: v1::SessionId::new("sess-1"),
                    connected: true,
                    on_delete: move |_| deleted += 1,
                }
                span { "data-slot": "deleted", "{deleted}" }
            }
        }

        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let click = || {
            Event::new(
                Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as Rc<dyn Any>,
                true,
            )
        };
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        let delete = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(text),
                    id,
                    ..
                } if text == "Delete session sess-1" => Some(*id),
                _ => None,
            })
            .expect("the delete control");

        dom.runtime().handle_event("click", click(), delete);
        let edits = dom.render_immediate_to_vec().edits;
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(">Confirm delete</button>")
                && html.contains(r#"data-slot="cancel-delete""#)
                && html.contains(r#"data-slot="deleted">0"#),
            "the first press only arms it: {html}"
        );

        // Cancel calls it off, and nothing was sent.
        let cancel = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" && *id != delete => {
                    Some(*id)
                }
                _ => None,
            })
            .expect("the cancel control");
        dom.runtime().handle_event("click", click(), cancel);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(">Delete</button>") && html.contains(r#"data-slot="deleted">0"#),
            "{html}"
        );

        // Two presses send it, once.
        dom.runtime().handle_event("click", click(), delete);
        dom.render_immediate(&mut NoOpMutations);
        dom.runtime().handle_event("click", click(), delete);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="deleted">1"#) && html.contains(">Delete</button>"),
            "{html}"
        );
    }

    /// The panel over an agent that claims everything, with this failed call.
    fn rendered_failing(failed: Option<Failed>) -> String {
        rendered_with(
            Some(everything()),
            true,
            Some(Restore::Load),
            None,
            failed,
            undriven(),
            AuthState::default(),
        )
    }

    /// The listing for an agent that reopens sessions the way `restores` says.
    fn listing_for(restores: Option<Restore>) -> String {
        affordances(&rendered(
            everything(),
            true,
            restores,
            Some(two_sessions()),
        ))
    }

    #[test]
    fn a_session_listed_on_two_pages_is_one_row() {
        // Pages are appended as fetched; a session that moved between them
        // arrives twice, and its rows are keyed by its id.
        let mut listing = two_sessions();
        listing
            .sessions
            .push(v1::SessionInfo::new("sess-1", "/tmp").title("The first one"));
        let listed = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(listing),
        ));

        assert_eq!(rows_shown(&listed).len(), 2, "{listed}");
    }

    #[test]
    fn a_listed_session_can_be_closed_and_deleted_where_the_agent_advertised_them() {
        let listed = listing_for(Some(Restore::Load));

        for row in rows_shown(&listed) {
            assert!(
                row.contains("session/close") && row.contains("session/delete"),
                "each listed session can be closed and deleted: {row}"
            );
        }
    }

    #[test]
    fn every_listed_session_affordance_names_the_session_it_drives() {
        for (how, opening) in [
            (Restore::Load, "session/load"),
            (Restore::Resume, "session/resume"),
        ] {
            let listed = listing_for(Some(how));

            for (row, session) in rows_shown(&listed).into_iter().zip(["sess-1", "sess-2"]) {
                for affordance in [opening, "session/close", "session/delete"] {
                    let name = format!(r#"aria-label="{} session {session}""#, verb(affordance));
                    assert!(
                        row.contains(&name),
                        "{affordance} identifies {session}: {row}"
                    );
                }
                for name in [
                    format!("Edit additionalDirectories for session {session}"),
                    format!("additionalDirectories for session {session}"),
                ] {
                    assert!(
                        row.contains(&format!(r#"aria-label="{name}""#)),
                        "the repeated roots control identifies {session}: {row}"
                    );
                }
                assert!(
                    row.contains(&format!(
                        r#"title="Edit additionalDirectories for session {session}""#
                    )),
                    "the roots disclosure tooltip identifies {session}: {row}"
                );
            }
        }
    }

    #[test]
    fn listed_sessions_state_which_one_is_current_without_changing_its_affordances() {
        let listed = listing_for(Some(Restore::Load));
        let rows = rows_shown(&listed);

        // The live one's state is carried by its section heading.
        assert!(
            listed.contains(r#"<h3 class="heading">Current session</h3>"#),
            "the live Session is explicit in words: {listed}"
        );
        assert!(
            !rows[0].contains("session-state"),
            "and does not say it twice on the row under that heading: {}",
            rows[0]
        );
        assert!(
            rows[1].contains("Available session"),
            "another listed Session is explicit in words: {}",
            rows[1]
        );
        for affordance in ["session/load", "session/close", "session/delete"] {
            assert!(
                rows[0].contains(affordance),
                "being current is a state, not a gate on {affordance}: {}",
                rows[0]
            );
        }
    }

    #[test]
    fn session_lifecycle_and_roots_are_the_dialogs_own_rows() {
        let panel = rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(reporting_roots()),
        );
        assert!(
            panel.contains(r#"class="sessions-content" data-slot="sessions""#)
                && !panel.contains("card"),
            "the Session lifecycle is flat content of its dialog rather than a card in it: {panel}"
        );
        let lifecycle = affordances(&panel);

        for component in [
            r#"class="session-list""#,
            r#"class="session-row""#,
            r#"class="session-head""#,
            r#"class="session-line""#,
            r#"class="session-actions""#,
            "disclose bare roots-new",
            "disclose bare roots-supplied",
            r#"class="textarea""#,
        ] {
            assert!(
                lifecycle.contains(component),
                "the session lifecycle is drawn with {component}: {lifecycle}"
            );
        }
        assert!(
            lifecycle.contains(r#"class="disclosure-icon" aria-hidden="true""#)
                && lifecycle.contains("<svg"),
            "new and reported roots use the shared Lucide disclosure marker: {lifecycle}"
        );
        // The way in carries the accent; only delete is drawn in the failure hue.
        for (class, method) in [
            ("btn btn-sm btn-filled", "New session"),
            ("btn btn-xs btn-quiet btn-go reopen", "Load"),
            ("btn btn-xs btn-quiet session-end", "Close"),
            ("btn btn-xs btn-quiet btn-bad session-end", "Delete"),
        ] {
            let before = lifecycle
                .split_once(&format!(">{method}</button>"))
                .unwrap_or_else(|| panic!("{method} retains its visible label: {lifecycle}"))
                .0;
            let button = before
                .rsplit_once("<button")
                .expect("the method label belongs to a button")
                .1;
            assert!(
                button.contains(&format!(r#"class="{class}""#)),
                "{method} retains its visible label and agreed control hierarchy: {lifecycle}"
            );
        }

        let refused = affordances(&rendered_failing(Some(Failed {
            method: "session/delete",
            error: CallError::Rejected(v1::Error::method_not_found()),
        })));
        assert!(
            refused.contains("alert alert-error"),
            "a lifecycle refusal uses a daisyUI failure notice: {refused}"
        );

        let loading = affordances(&rendered_with_listing_state(
            Some(everything()),
            true,
            Some(Restore::Load),
            ListingState {
                listing: Some(two_sessions()),
                pending: true,
                advertised: None,
            },
            None,
            undriven(),
            AuthenticationState {
                state: AuthState::default(),
                pending: Vec::new(),
            },
        ));
        assert!(
            loading.contains(
                r#"class="alert" data-slot="session-list-status" role="status" aria-live="polite""#
            ) && loading.contains("Loading sessions.")
                && loading.contains("Previous rows are hidden")
                && loading.contains("until the agent answers"),
            "an in-flight listing is an accessible daisyUI status: {loading}"
        );
        let listed_while_loading = loading
            .split_once(r#"class="rail-group session-listing""#)
            .expect("the listing section")
            .1;
        assert!(
            !listed_while_loading.contains("session-row"),
            "stale rows are not presented as the pending page: {loading}"
        );
        // The live session keeps its id while a page is in flight, but not the
        // listing's description of it.
        assert!(
            loading.contains("session-row live") && loading.contains("sess-1"),
            "the live session is still named while the listing reloads: {loading}"
        );
        assert!(
            !loading.contains("session-cwd"),
            "and nothing the replaced page said about it stays on screen: {loading}"
        );

        let empty = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(SessionListing::default()),
        ));
        assert!(
            empty.contains(
                r#"class="alert" data-slot="session-list-status" role="status" aria-live="polite""#
            ) && empty.contains("The agent listed no sessions."),
            "an empty listing is an accessible daisyUI status: {empty}"
        );

        #[component]
        fn Paged(connected: bool) -> Element {
            rsx! { SessionPaging { available: true, connected, on_next: move |()| {} } }
        }
        let mut dom = VirtualDom::new_with_props(Paged, PagedProps { connected: true });
        dom.rebuild_in_place();
        let paged = dioxus_ssr::render(&dom);
        assert!(
            paged.contains("btn btn-xs btn-quiet next-page")
                && paged.contains("The agent has more sessions.")
                && paged.contains(r#"aria-label="Ask the agent for the next page of sessions""#)
                && paged.contains(">Next page</button>"),
            "a paged listing exposes its daisyUI paging Affordance: {paged}"
        );

        let mut dom = VirtualDom::new_with_props(Paged, PagedProps { connected: false });
        dom.rebuild_in_place();
        let unavailable = dioxus_ssr::render(&dom);
        assert!(
            unavailable.contains("Next page is unavailable while disconnected.")
                && unavailable.contains(r#"aria-disabled="true""#),
            "paging availability is stated in words rather than by dimness alone: {unavailable}"
        );
    }

    #[test]
    fn an_agent_that_advertised_one_of_the_two_is_offered_that_one() {
        // Two claims, two gates, checked both ways round (§7.5).
        for (claimed, offered, withheld) in [
            ("close", "session/close", "session/delete"),
            ("delete", "session/delete", "session/close"),
        ] {
            let capabilities = v1::AgentCapabilities::new().session_capabilities(match claimed {
                "close" => v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .close(v1::SessionCloseCapabilities::new()),
                _ => v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            });
            let listed = affordances(&rendered(
                described(capabilities),
                true,
                None,
                Some(two_sessions()),
            ));

            for row in rows_shown(&listed) {
                assert!(row.contains(offered), "the claim it made: {row}");
                assert!(!row.contains(withheld), "and only that one: {row}");
            }
            assert!(
                listed.contains(offered) && !listed.contains(withheld),
                "said once above them all: {listed}"
            );
        }
    }

    #[test]
    fn an_agent_that_advertised_neither_close_nor_delete_is_offered_neither() {
        let listed = affordances(&rendered(only_listing(), true, None, Some(two_sessions())));

        assert!(
            listed.contains("sess-1"),
            "the listing is still here: {listed}"
        );
        for row in rows_shown(&listed) {
            assert!(
                !row.contains("session/close") && !row.contains("session/delete"),
                "and no way to end a session: {row}"
            );
        }
    }

    #[test]
    fn a_capability_the_agent_advertised_and_then_failed_says_which_one() {
        // The failure names the method the agent claimed, in its own words.
        let refused = affordances(&rendered_failing(Some(Failed {
            method: "session/close",
            error: CallError::Undecodable("not what a close answers".to_owned()),
        })));

        assert!(
            refused.contains("session/close") && refused.contains("failed"),
            "the call that failed, named: {refused}"
        );
        assert!(
            refused.contains("not what a close answers"),
            "with what the agent said about it: {refused}"
        );
    }

    #[test]
    fn the_ways_out_of_a_session_are_quieter_than_the_way_in_and_never_hidden() {
        // Never revealed on hover: they are the only way to do these things.
        let listed = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));
        for row in rows_shown(&listed) {
            for ends in ["session/close", "session/delete"] {
                assert!(row.contains(ends), "{ends} is drawn: {row}");
            }
        }

        assert!(
            listed.contains("btn btn-xs btn-quiet btn-go reopen")
                && listed.contains("btn btn-xs btn-quiet session-end"),
            "the way in carries the accent and the ways out carry none: {listed}"
        );
    }

    #[test]
    fn a_connection_that_ended_says_so_where_its_controls_are() {
        // The record outlives the agent (§5); the dialog says it is disconnected.
        let gone = affordances(&rendered(
            everything(),
            false,
            Some(Restore::Load),
            Some(two_sessions()),
        ));
        assert!(
            gone.contains(r#"data-slot="availability""#)
                && gone.contains("Disconnected. These are the sessions"),
            "a dialog full of inert controls says why: {gone}"
        );
        assert!(
            gone.contains("sess-1") && gone.contains("sess-2"),
            "and keeps what the last agent reported: {gone}"
        );

        let live = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));
        assert!(
            !live.contains(r#"data-slot="availability""#),
            "and says nothing while there is an agent to answer: {live}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_cannot_be_asked_to_close_or_delete_one() {
        let listed = affordances(&rendered(
            everything(),
            false,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        for row in rows_shown(&listed) {
            assert_eq!(
                row.matches("disabled").count(),
                row.matches("<button").count(),
                "every way into or out of a session is disabled: {row}"
            );
        }
    }

    #[test]
    fn a_session_can_be_opened_whatever_the_agent_advertised_about_sessions() {
        // `session/new` is ungated (§7.5); the listing is the agent's claim to make.
        let offered = affordances(&shown(nothing(), true));

        assert!(
            offered.contains("session/new"),
            "the create affordance is not gated on a capability: {offered}"
        );
        assert!(
            !offered.contains("session/list"),
            "and the listing still is: {offered}"
        );
        assert!(
            offered.contains("cancelled first"),
            "what a switch costs is stated where the button is: {offered}"
        );

        let claimed = affordances(&shown(everything(), true));
        assert!(
            claimed.contains("session/new") && claimed.contains("session/list"),
            "{claimed}"
        );
    }

    #[test]
    fn the_listing_claim_and_affordance_cannot_contradict_each_other() {
        let html = rendered_with_listing_state(
            Some(everything()),
            true,
            None,
            ListingState {
                listing: None,
                pending: false,
                advertised: Some(false),
            },
            None,
            undriven(),
            AuthenticationState {
                state: AuthState::default(),
                pending: Vec::new(),
            },
        );
        let claim = row_for(&html, "session/list");
        assert!(claim.contains("not advertised"), "{claim}");
        assert!(
            !affordances(&html).contains(">session/list</button>"),
            "one panel must not say session/list was not advertised while offering it: {html}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_leaves_the_affordance_saying_it_cannot() {
        let offered = affordances(&shown(nothing(), false));

        let button = offered
            .split_once("session/new")
            .expect("the create affordance is drawn")
            .0
            .rsplit_once("<button")
            .expect("drawn as a button")
            .1;
        assert!(
            button.contains("disabled"),
            "an agent that is gone cannot be asked for a session: {offered}"
        );
    }

    #[test]
    fn a_listed_session_is_a_way_into_that_session() {
        let listed = listing_for(Some(Restore::Load));
        let rows = rows_shown(&listed);

        assert_eq!(rows.len(), 2, "one row per listed session: {listed}");
        for row in rows {
            assert!(
                row.contains("<button") && row.contains("session/load"),
                "each is a way in: {row}"
            );
        }
    }

    #[test]
    fn replay_is_a_property_of_the_call_and_not_two_buttons() {
        // One reopen per row: load and resume are one operation (§7.5).
        for (how, sent, other) in [
            (Restore::Load, "session/load", "session/resume"),
            (Restore::Resume, "session/resume", "session/load"),
        ] {
            let listed = listing_for(Some(how));
            for row in rows_shown(&listed) {
                assert!(row.contains(sent), "{row}");
                assert!(!row.contains(other), "and only the one: {row}");
                assert_eq!(ways_in(row), 1, "one way in, not two: {row}");
            }
        }
    }

    #[test]
    fn an_agent_that_only_resumes_says_previous_messages_cannot_be_viewed() {
        // After a resume an empty timeline is correct, so it is said.
        let resuming = listing_for(Some(Restore::Resume));

        assert!(
            resuming.contains("replay earlier messages"),
            "it says so plainly: {resuming}"
        );
        assert!(
            resuming.contains("the timeline starts empty"),
            "and says what the empty timeline means: {resuming}"
        );

        let loading = listing_for(Some(Restore::Load));
        assert!(
            loading.contains("replays its conversation into the timeline before it answers"),
            "{loading}"
        );
        assert!(
            !loading.contains("does not support viewing previous messages"),
            "{loading}"
        );
    }

    #[test]
    fn an_agent_that_advertised_neither_is_offered_neither() {
        // The listing still shows; the rows have nothing to click, and say why.
        let listed = listing_for(None);

        assert!(
            listed.contains("sess-1"),
            "the listing is still here: {listed}"
        );
        for row in rows_shown(&listed) {
            assert_eq!(ways_in(row), 0, "and no way in: {row}");
        }
        assert!(
            listed.contains("no way in to a session it already has"),
            "with the reason: {listed}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_cannot_be_asked_to_open_one() {
        let listed = affordances(&rendered(
            everything(),
            false,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        for row in rows_shown(&listed) {
            assert!(row.contains("<button") && row.contains("disabled"), "{row}");
        }
    }

    /// The dialog over [`two_sessions`] with `sess-1` live, and these updates
    /// stated about it.
    fn reported(updates: &[v1::SessionInfoUpdate]) -> String {
        #[component]
        fn Host(reports: SessionReports) -> Element {
            let claims = crate::agent::Claims {
                session: Some(v1::SessionId::new("sess-1")),
                listing: Some(two_sessions()),
                reports,
                ..crate::agent::Claims::about(Some(everything()))
            };
            rsx! { crate::session::Sessions { claims } }
        }

        let mut reports = SessionReports::default();
        for update in updates {
            reports.info(&v1::SessionId::new("sess-1"), update);
        }
        let mut dom = VirtualDom::new_with_props(Host, HostProps { reports });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn a_title_an_update_stated_replaces_the_listings_and_both_are_shown() {
        let listed = reported(&[]);
        let rows = rows_shown(&listed);
        assert!(
            rows[0].contains("The first one") && !rows[0].contains(r#"data-slot="relisted""#),
            "with no update the listing names it, once: {}",
            rows[0]
        );

        let renamed = reported(&[v1::SessionInfoUpdate::new()
            .title("Fix the flaky build")
            .updated_at("2026-10-12T09:00:00Z")]);
        let rows = rows_shown(&renamed);
        assert!(
            rows[0].contains(r#"title="From session_info_update">Fix the flaky build<"#),
            "the update names it, and says so: {}",
            rows[0]
        );
        assert!(
            rows[0].contains("listed as “The first one”"),
            "the listing's claim is still there to read: {}",
            rows[0]
        );
        assert!(
            rows[0].contains("updated 2026-10-12T09:00:00Z"),
            "{}",
            rows[0]
        );
        assert!(
            rows[1].contains("untitled session"),
            "another session's row is the listing's alone: {}",
            rows[1]
        );
    }
}
