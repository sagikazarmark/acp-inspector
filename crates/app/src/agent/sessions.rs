//! The Sessions dialog: the live session, the listing, and the ways in and out.

use acp_inspector_core::{Cursor, Restore, Roots, SessionListing, v1};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdPlus, LdRefreshCw},
};

use super::roots::{Reopen, roots_field, roots_rule};
use super::{Failed, Opening};

/// The session lifecycle affordances, and whatever the agent last answered into
/// them (§7.5).
///
/// **Three parts, in the order a reader needs them**: the session they are in,
/// the sessions they could be in, and the way to a new one. It was one flat
/// run — two buttons, five paragraphs of consequence, then every session the
/// agent named as an undifferentiated list with the live one marked by a word
/// halfway down its row. That is the shape a rail leaves behind, where a block
/// is read top to bottom once; a surface opened to *change* the session is read
/// by looking for the one you are in and the one you want.
///
/// **The live session is stated whether or not it was listed.** It is what the
/// window is showing, and it used to appear only if `session/list` had been
/// pressed and only as one row among the others — so a dialog opened on a
/// working connection could say nothing at all about the session behind it. An
/// agent that never advertised listing still has a live session, and the calls
/// that end one carry nothing but its id.
///
/// **The way to a new session is at the foot**, where a dialog's own action
/// goes, because it is what a reader does when none of the above was what they
/// wanted. The roots the session opens with are beside it (§7.7), where the
/// agent advertised `additionalDirectories` and nowhere else.
pub(crate) fn sessions(
    listing: Listing<'_>,
    connected: bool,
    failed: Option<&Failed>,
    creating: Signal<String>,
    on_new_session: EventHandler<Option<Roots>>,
) -> Element {
    let roots = listing.roots;
    let on_list = listing.on_list;
    // The listed sessions, split on the one the window is showing. Two lists
    // rather than one with a marked row: which session you are in is the first
    // question this surface is opened with, and a word in the middle of the
    // fourth row is an answer a reader has to search for.
    let answered = listing
        .answered
        .map(|answered| answered.sessions.as_slice());
    // **While a listing is in flight, the live session keeps its id and loses
    // its description.** Which session the window is in is not the listing's to
    // say — it is the one that was opened — but where it runs and what it is
    // called came out of the answer now being replaced, and a row that went on
    // showing them would be presenting a stale page as the current one.
    let current = (!listing.pending)
        .then(|| {
            answered.and_then(|sessions| {
                sessions
                    .iter()
                    .find(|session| Some(&session.session_id) == listing.live)
            })
        })
        .flatten();
    let others: Vec<&v1::SessionInfo> = answered
        .unwrap_or_default()
        .iter()
        .filter(|session| Some(&session.session_id) != listing.live)
        .collect();

    rsx! {
        div { class: "dialog-body", "data-slot": "sessions",
            // **What is on this surface outlives the agent on purpose** (§5),
            // and until this line existed the whole of what it said about that
            // was a screen of controls nobody could press. The record stays —
            // an agent that went away mid-session is the finding, and a window
            // that emptied itself would be deleting the evidence to tidy up —
            // so what changes is that it says which of the two it is. The same
            // sentence in the same register as Session Settings and the login,
            // and only where there is a control it is about.
            if !connected {
                p {
                    class: "hint availability",
                    id: UNAVAILABLE,
                    "data-slot": "availability",
                    role: "status",
                    "Disconnected. These are the sessions the last agent reported; start an agent to act on them."
                }
            }

            // **Where you are**, drawn from the listing where the agent has
            // been asked for one and from the live id alone where it has not.
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

            // **What else the agent is holding**, behind the advertisement that
            // gates it: an agent that never claimed `session/list` has no
            // listing and no button to ask for one (§7.5).
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

            // The agent's own refusal, or the news that it is no longer there:
            // an affordance that did nothing and said nothing would be the
            // window keeping a secret core is there to tell. The call is named
            // because there are four of them here — an agent that advertised a
            // capability and then failed it is a finding about *that* method,
            // and one is worth reading precisely because the other three may
            // have worked.
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
                        // What pressing it costs, on the control rather than
                        // under it, with the method it sends: the label is the
                        // verb, and the wire name is the tooltip (D7).
                        title: "{v1::AGENT_METHOD_NAMES.session_new}: {SESSION_NEW_COSTS}",
                        aria_describedby: if connected { "session-new-costs".to_owned() } else { format!("session-new-costs {UNAVAILABLE}") },
                        // `None` where the agent advertised no control, so that a click
                        // says *no roots were asked for* rather than *the empty control
                        // was*: for a session that does not exist yet the two ask for
                        // the same thing, and only one of them is true.
                        onclick: move |_| if connected { on_new_session.call(roots.then(|| Roots::supplied(creating()))) },
                        span { class: "disclosure-icon", aria_hidden: "true",
                            Icon { class: "icon", width: 16, height: 16, icon: LdPlus }
                        }
                        "New session"
                    }

                    // Said once for every roots control in this block — the one
                    // beside `session/new` and the one on each listed row —
                    // rather than on each of them.
                    if roots {
                        details { class: "disclose bare roots-new",
                            summary {
                                aria_label: "Edit additionalDirectories for a new session",
                                title: "Edit additionalDirectories for a new session",
                                {crate::disclosure::chevron()}
                                code { "additionalDirectories" }
                            }
                            div { class: "disclose-body",
                                // Inside the disclosure, which is where the field it
                                // is a rule about is. Above it, it was a paragraph
                                // about a box nobody had opened yet — always drawn,
                                // read once, and read at the moment it is least
                                // useful.
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

                // The same sentence the control carries, kept in the document
                // for the reader who is not pointing at anything: a tooltip is
                // not an accessible name and `aria-describedby` needs something
                // to point at.
                p { class: "sr-only", id: "session-new-costs", {SESSION_NEW_COSTS} }
            }
        }
    }
}

/// One session, as a row of either list.
///
/// **The same row for the live session and for the rest**, which is what keeps
/// the two lists two lists rather than two designs: every affordance the agent
/// advertised is on both, gated the way §7.5 gates it, and what differs is the
/// heading above the list and the word the row says about itself.
///
/// `info` is `None` for a live session the agent has not listed — there is no
/// `SessionInfo` to draw, and no reopen either, because reopening one hands the
/// agent back its own description of it. What the row can still carry is the
/// two calls that take nothing but an id.
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

    rsx! {
        li {
            key: "{id}",
            class: if current { "session-row live" } else { "session-row" },
            // The drawing's row: a dot for which one this is, the title the
            // agent gave it, and what it last said about it at the far end.
            div { class: "session-head",
                span {
                    class: if current { "dot dot-live" } else { "dot dot-idle" },
                    aria_hidden: "true",
                }
                span { class: "session-title",
                    match info.and_then(|info| info.title.as_ref()) {
                        Some(title) => rsx! { "{title}" },
                        None => rsx! { span { class: "cleared", "untitled session" } },
                    }
                }
                // What a row *is*, in words, because a dot is decoration (§7.5).
                span { class: "sr-only",
                    if current { "Current session. " } else { "Available session. " }
                }
                span { class: "session-meta",
                    if let Some(updated) = info.and_then(|info| info.updated_at.as_ref()) {
                        "updated {updated}"
                    } else if info.is_none() && !sessions.pending {
                        "not in a listing"
                    }
                }
            }
            div { class: "session-line",
                code { class: "session-id", "{id}" }
                if let Some(info) = info {
                    span { class: "session-id", title: "{info.cwd.display()}", "{info.cwd.display()}" }
                }
            // **Every control on this row is bounded**, which is the one thing
            // the row's hierarchy was saying that nobody meant. Reopen and
            // close were `ghost` — a label and no boundary — beside a `delete`
            // that carries the outline its tone comes with, so a row of three
            // calls read as one control with two headings over it, and the
            // control it read as was the destructive one. They are one group of
            // push buttons now and the tone is the only thing that still tells
            // them apart, which is the difference there actually is.
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
                // Each on its own claim: an agent that advertised one of the
                // two is offered that one, and the row is a report of what it
                // said rather than a menu of what a session can have done to
                // it. **A step down from the way in, and never hidden.** They
                // were revealed on hover, which is the rule for a control that
                // repeats something already on the row and the wrong one for
                // the only way to do a thing: what is not drawn cannot be
                // looked for, and a list that has to be swept with a pointer to
                // be read is a list that cannot be read at a glance. `btn-xs`
                // against the reopen's `btn-sm` is the difference that was
                // meant all along.
                // **One disabled pattern, with its reason** (A5): unavailable
                // controls stay focusable and say why through the availability
                // line, rather than dropping out of the Tab order unexplained.
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

/// `session/delete`, confirmed in two steps (B6).
///
/// **Delete, then Confirm delete.** It is the one call in the dialog that
/// cannot be taken back, and it sat beside Close at the same size. The first
/// press arms it, the second sends; Cancel, Escape, or a few seconds of
/// nothing disarm it. The label changes on the same button, so focus stays
/// where the reader pressed.
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

/// What the listing affordance is made of: what the agent said it can do, what
/// it last answered, and the two ways of asking it for more.
///
/// One value rather than a fistful of arguments threaded through two functions
/// — they are one question the panel asks about sessions, and what a row can do
/// is only meaningful beside the row it does it to.
#[derive(Clone, Copy)]
pub(crate) struct Listing<'a> {
    /// Whether the agent advertised `session/list` — core's answer, and the
    /// gate this whole group sits behind.
    pub(super) advertised: bool,
    /// What it answered the last time it was asked, if it has been.
    pub(super) answered: Option<&'a SessionListing>,
    /// The live Session, used only to state current versus available in words.
    pub(super) live: Option<&'a v1::SessionId>,
    /// Whether the answer, or its next page, is currently in flight.
    pub(super) pending: bool,
    /// How a listed session is opened, or `None` where the agent advertised no
    /// way to.
    pub(super) restores: Option<Restore>,
    /// Whether the agent advertised `session/close`, and whether it advertised
    /// `session/delete` — two gates because they are two claims, each made in
    /// its own field (§7.5).
    pub(super) closes: bool,
    pub(super) deletes: bool,
    /// Whether the agent advertised `additionalDirectories` — the gate on the
    /// roots control, beside the create button and on every row (§7.7).
    pub(super) roots: bool,
    pub(super) on_list: EventHandler<Option<Cursor>>,
    pub(super) on_open: EventHandler<Opening>,
    pub(super) on_close: EventHandler<v1::SessionId>,
    pub(super) on_delete: EventHandler<v1::SessionId>,
}

/// The sessions the agent named that are not the one in front of the reader,
/// and the states that stand in for them.
///
/// **A listed session is a way into that session** (§7.5), which is what makes
/// the listing actionable rather than text to read. One affordance per row and
/// never two: `session/load` and `session/resume` restore the same thing and
/// differ only in whether the conversation replays first, so the *call* carries
/// the difference ([`Restore`]) and the screen states it — [`opening`] is where
/// it is stated. Which call this agent gets is core's answer, preference and
/// all; a row is offered it whenever the agent advertised one, and an agent that
/// advertised a method it never implemented is driven anyway, because its
/// refusal is the finding.
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
            // Advertised and unasked: the button is here because the agent
            // said it lists, not because there is anything to see yet.
            None => rsx! {
                // A line rather than a bounded notice. The other two states of
                // this region are news that arrived — a page loading, a page
                // that came back empty — and a box is right for those; this one
                // is what the surface says before anything has been asked, and
                // drawing it as an alert made *nothing has happened yet* the
                // most boxed-in thing in the dialog.
                p { class: "hint", "data-slot": "session-list-status", role: "status", "The agent says it can list its sessions." }
            },
            Some(listing) if listing.sessions.is_empty() => rsx! {
                p { class: "alert", "data-slot": "session-list-status", role: "status", aria_live: "polite", "The agent listed no sessions." }
            },
            // Listed, and every session it named is the one already in front of
            // the reader. Said rather than left as a gap under a heading: an
            // empty region where rows were is a listing that failed, and this
            // one succeeded.
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

        // A page the agent kept back. The cursor goes straight back to core,
        // unread: what is inside it is the agent's business, and a client that
        // parsed it would be guessing at a token no agent in the survey has
        // ever sent.
        SessionPaging {
            available: next_page.is_some(),
            connected,
            on_next: move |()| on_list.call(next_page.clone()),
        }
    }
}

/// The Affordance for a page the Agent kept back.
///
/// Availability is separate from the callback so presentation tests never need
/// to manufacture or inspect the opaque [`Cursor`] the callback closes over.
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

/// What every one of those controls is held to, said once above all of them
/// (§7.7).
///
/// **It is a sentence about this tool, so it is said once.** The rule is the
/// inspector's — the schema names the session's `cwd` as the base, and this
/// window resolves a relative path against it before anything crosses rather
/// than letting a malformed request go out ([`Roots`]) — and a control drawn on
/// `session/new` and again on every listed session repeated it verbatim on each.
/// An agent advertising `additionalDirectories` with ten sessions listed printed
/// this paragraph eleven times, which is the shape §7.7 already refused for the
/// fourth fact: one fact about the inspector drawn as eleven about the agent.
///
/// The control keeps the half that is about *its own* field — one absolute path
/// per line, beside the box it is typed in — because that half is a label rather
/// than a rule, and a label belongs on the thing it labels.
/// What pressing `session/new` costs, in the words the rail used to print.
///
/// One string, because it is now said in two places that must not drift: on the
/// control as its tooltip, and in the document as what the control is described
/// by.
pub(crate) const SESSION_NEW_COSTS: &str = "Opens and switches to a new session. A turn still running is cancelled first; the trace keeps every frame.";

/// What the row's own controls cost, by the ids they point at.
///
/// **The same treatment `session/new` already gets.** These sentences were five
/// visible paragraphs above the list — written for a rail read once, and a wall
/// between a reader and the thing they opened this to press. They are on the
/// controls they are about now: on the tooltip for a pointer, in the document
/// for `aria-describedby`, and the accessibility tree carries exactly what it
/// carried before. What stays visible is the one that is not about a control at
/// all — an agent that advertised no way back into a session has an absence to
/// explain, and an absence has nothing to hang a description on.
pub(crate) const OPENING: &str = "session-open-costs";

pub(crate) const ENDING: &str = "session-end-costs";

/// What opening one of the listed sessions will do, said once above them all.
///
/// **Replay is a property of the call, and this is where the property is
/// read.** The two calls restore the same thing; the one difference is whether
/// the conversation arrives with it, and that difference decides whether the
/// timeline below can be rebuilt at all — so it is stated in words rather than
/// left to be inferred from what does or does not turn up.
///
/// Where the agent offers only `session/resume`, saying so is the whole point:
/// a resumed session's **empty timeline is correct**, and a screen that let the
/// reader discover that on their own would have them reading a fault in the
/// inspector instead of a fact about the agent.
///
/// Which sentence is drawn is decided by [`Restore::replays`] and not by which
/// call it is — the property is core's, and a window that re-derived it from
/// the method name would be a second place to get it wrong.
fn opening(restores: Option<Restore>) -> Element {
    rsx! {
        p {
            class: if restores.is_some() { "sr-only" } else { "hint" },
            id: OPENING,
            {match restores {
                // Gated on the advertisement, here as everywhere: an agent that
                // claimed neither is asked for neither, and its listing is a
                // description with nothing to click.
                None => rsx! {
                    "The agent advertised neither "
                    code { "session/load" }
                    " nor "
                    code { "session/resume" }
                    ", so there is no way in to a session it already has."
                },
                // Split on the *property* rather than on which call it is, which
                // is what makes this two sentences and not one per method:
                // `Restore` is non-exhaustive, and a way of opening a session
                // that this window has never heard of still either brings the
                // conversation with it or does not.
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

/// What ending one of the listed sessions will do, said once above them all.
///
/// **Three things a reader cannot get from the button** (§7.5). That the
/// listing is asked again afterwards, because whether a closed session is still
/// there is the agent's answer and the reason to press the button at all. That
/// ending the *live* session is allowed and leaves the inspector with none open
/// — the timeline goes with it and the trace keeps every frame. And that the
/// corners the specification leaves undefined are offered rather than withheld,
/// so a button that does something surprising is the agent being surprising.
///
/// Nothing here says what the agent *should* do with any of it, because the
/// specification does not: close and delete are advertised capabilities with
/// undefined edges, and a window that promised an outcome would be inventing
/// one.
fn ending(closes: bool, deletes: bool) -> Element {
    let named = match (closes, deletes) {
        (true, true) => "session/close and session/delete",
        (true, false) => "session/close",
        (false, true) => "session/delete",
        // The agent claimed neither, so there is nothing to explain: the rows
        // below carry no way to end a session.
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

    /// The panel over an agent that claims everything, with a lifecycle call
    /// that did not do what it said.
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

    /// The listing as it is drawn, for an agent that lists and opens sessions
    /// the way `restores` says.
    fn listing_for(restores: Option<Restore>) -> String {
        affordances(&rendered(
            everything(),
            true,
            restores,
            Some(two_sessions()),
        ))
    }

    #[test]
    fn a_listed_session_can_be_closed_and_deleted_where_the_agent_advertised_them() {
        // The end of the lifecycle ring (§7.5): every row carries the two calls
        // this agent claimed, named by the method each will send.
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

        // The live one is under a heading that says so, which is where the
        // word moved when the marked row became a section of its own: what §7.5
        // asks is that the state is carried in words, not that every row
        // repeats its own section.
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
            panel.contains(r#"class="dialog-body" data-slot="sessions""#)
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
        // **The hierarchy is the tone now and no longer the size.** Every
        // control on a row is the row's own height in the drawing, so what
        // separates the way in from the ways out is the accent one carries and
        // the other two do not — and the destructive one is the only one drawn
        // in the failure hue.
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
        // And the live session keeps its id while that page is in flight,
        // because which session the window is in is not the listing's answer —
        // but it loses the description that *was* the listing's, so a refresh
        // never leaves a stale working directory on screen.
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
        // Two claims in two fields, so two gates: an agent that closes sessions
        // and does not delete them is offered close and not delete (§7.5). Both
        // ways round, because a screen wired to the wrong field would pass every
        // test that only ever advertised both.
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
            // And the sentence above the rows names what it will send, which is
            // the same one claim.
            assert!(
                listed.contains(offered) && !listed.contains(withheld),
                "said once above them all: {listed}"
            );
        }
    }

    #[test]
    fn an_agent_that_advertised_neither_close_nor_delete_is_offered_neither() {
        // Gated on the advertisement and on nothing else, each gate its own: the
        // listing is still here, the rows are still here, and there is nothing
        // on them to close or delete with.
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
        // §7.5's capability lie, where a reader meets it: the affordance was
        // drawn because the agent claimed the method, so the failure is about
        // that method — and four affordances sit here, three of which may have
        // worked. The agent's own words are kept whole beside it.
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
        // Revealing a control on hover is the rule for one that repeats
        // something already on the row — the Timeline's `raw` and its way back
        // to the wire. It is the wrong rule for the only way to do a thing:
        // what is not drawn cannot be looked for, and a list that has to be
        // swept with a pointer to be read cannot be read at a glance.
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

        // What separates them from the way in is the size the two are drawn at,
        // which is the difference that was meant when they were hidden.
        assert!(
            listed.contains("btn btn-xs btn-quiet btn-go reopen")
                && listed.contains("btn btn-xs btn-quiet session-end"),
            "the way in carries the accent and the ways out carry none: {listed}"
        );
    }

    #[test]
    fn a_connection_that_ended_says_so_where_its_controls_are() {
        // The record outlives the agent on purpose (§5) — an agent that went
        // away mid-session is the finding, and a window that emptied itself
        // would be deleting the evidence to tidy up. What it owes is to say
        // which of the two states it is in, where there are controls it is
        // about.
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
        // The rule every button here follows: one that cannot do what it says is
        // worse than one that says it cannot.
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
        // `session/new` is the one lifecycle method the protocol does not gate
        // (§7.5): every v1 agent opens sessions, so the affordance is there for
        // an agent that advertised nothing at all — while the listing beside it
        // is absent, because that one *is* the agent's claim to make.
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

        // The other half of the same rule: an agent that claimed listing is
        // offered both, and neither of them because there is anything to list.
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
        // What the panel already does with the buttons it had: the claims
        // outlive the agent that made them (§5), and a button that cannot do
        // what it says is worse than one that says it cannot.
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
        // The ring's point about the listing (§7.5): it stops being text you
        // can read. Every row the agent named carries the affordance, and the
        // affordance is named by the method it will send — the reader of an
        // inspector is reading the protocol.
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
        // **One screen, one affordance per session** (§7.5). `session/load` and
        // `session/resume` restore the same thing and differ only in whether
        // the conversation replays first, so a row offers the one this agent
        // gets and says what that means — never both, which would ask the
        // reader to choose between two calls the protocol says are the same
        // call.
        for (how, sent, other) in [
            (Restore::Load, "session/load", "session/resume"),
            (Restore::Resume, "session/resume", "session/load"),
        ] {
            let listed = listing_for(Some(how));
            for row in rows_shown(&listed) {
                assert!(row.contains(sent), "{row}");
                assert!(!row.contains(other), "and only the one: {row}");
                // Counted among the ways *in* rather than among the buttons:
                // the row also carries the two ways out (§7.5), and those are
                // two calls the protocol keeps apart rather than one with a
                // property.
                assert_eq!(ways_in(row), 1, "one way in, not two: {row}");
            }
        }
    }

    #[test]
    fn an_agent_that_only_resumes_says_previous_messages_cannot_be_viewed() {
        // Which is the whole reason the screen says anything at all: a resumed
        // session's timeline is empty and *correct*, and a reader left to work
        // that out reads it as a fault in the inspector.
        let resuming = listing_for(Some(Restore::Resume));

        assert!(
            resuming.contains("replay earlier messages"),
            "it says so plainly: {resuming}"
        );
        assert!(
            resuming.contains("the timeline starts empty"),
            "and says what the empty timeline means: {resuming}"
        );

        // And the other way round, the replay is what is promised.
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
        // Gated on the advertisement, like everything else here: the listing is
        // still shown — the agent said it lists — and there is nothing to click
        // on a row, with the reason said rather than left as an absence.
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
        // The rule the other two buttons already follow: a button that cannot
        // do what it says is worse than one that says it cannot.
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
}
