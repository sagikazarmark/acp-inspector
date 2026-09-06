//! The inline permission panel (`docs/architecture.md` §7.2, §9): the request
//! the turn stopped for, rendered where it stopped.
//!
//! **Inline, and nothing else.** No modal, no toast, nothing to dismiss: in ACP
//! a permission request is the main traffic of a turn rather than an
//! exceptional alert, so it sits in the timeline in wire order like every other
//! entry, and the trace beside it stays readable while it waits. Where the turn
//! blocked is where the reader is looking.
//!
//! **The options are the agent's.** Every button here is one the agent sent —
//! its id, its name and its kind, in the order it offered them — and there is
//! never a button for an option it did not offer. The kind decides only how a
//! button reads; what it *does* is send back the id, unchanged, because that id
//! is the agent's own word and this window has no opinion about it.
//!
//! **Whether the buttons are live is read, never worked out here.** Core says
//! where a request stands ([`PermissionState`]) — waiting, being answered,
//! answered, or given up on because the turn ended or the agent went away — and
//! this file renders that answer. A panel that decided for itself, out of a
//! turn state and a connection status, would be a second answer to a question
//! that already has one, wrong in exactly the moments that matter.

use acp_inspector_core::{PermissionRequest, PermissionState, v1};
use dioxus::prelude::*;

use crate::update;

/// The user's answer to a blocking request: the request, and the option they
/// picked.
///
/// The request travels with the choice because the request is what knows how to
/// be answered — it carries its own resolver — so a handler holding one of
/// these needs nothing else to finish the job.
#[derive(Clone, PartialEq)]
pub struct Answer {
    pub request: PermissionRequest,
    pub option: v1::PermissionOptionId,
}

/// The request, the tool call under question, and the options — as the card the
/// drawing gives a question the turn stopped on: a header naming the method in
/// its own colour, the ask, and the agent's options as the row of answers.
pub fn panel(request: &PermissionRequest, on_answer: EventHandler<Answer>) -> Element {
    let state = request.state();
    let waiting = state == PermissionState::Waiting;
    let request_to_answer = request.clone();
    let on_choose = EventHandler::new(move |option| {
        on_answer.call(Answer {
            request: request_to_answer.clone(),
            option,
        });
    });

    rsx! {
        div {
            class: if waiting { "evidence blocking" } else { "evidence blocking answered" },
            "data-slot": "permission",
            div { class: "blocking-head",
                span { class: "eyebrow", "{v1::CLIENT_METHOD_NAMES.session_request_permission}" }
                span { class: "blocking-wait",
                    if waiting { "awaiting client" } else { "resolved" }
                }
            }
            div { class: "blocking-body",
                // The tool call the agent named, rendered as a tool call is
                // rendered anywhere else: what is being asked about is the
                // question, and a second way of drawing it would be a second
                // thing to read.
                {update::tool_call_update(request.tool_call())}
                span { class: "blocking-meta", "{request.id()}" }

                {said(&state)}

                if request.options().is_empty() {
                    // A request with nothing to answer it with. It is a finding,
                    // not an empty list: the agent blocked its own turn on a
                    // question nobody can answer.
                    p { class: "unnamed",
                        "The agent offered no options, so there is nothing to answer it with. Cancelling the turn is what ends this."
                    }
                } else {
                    {choices(request.options(), &state, on_choose)}
                }
            }
        }
    }
}

/// Every option the Agent supplied, in the order it supplied them.
fn choices(
    options: &[v1::PermissionOption],
    state: &PermissionState,
    on_choose: EventHandler<v1::PermissionOptionId>,
) -> Element {
    // The agent's own order, and the first allow in it is the one drawn filled.
    let primary = options
        .iter()
        .position(|option| kind(option.kind).1 == "allow");

    rsx! {
        div { class: "answers",
            for (position, option) in options.iter().enumerate() {
                {choice(option, Some(position) == primary, state, on_choose)}
            }
        }
    }
}

/// One option, as the button that sends its id.
///
/// **The first allow is the filled one**, which is the drawing's own hierarchy
/// for a row of answers: one primary and the rest bounded. Which one that is
/// remains the *agent's* order and never a judgement — the tone still says
/// allow from reject, and every option the agent sent is on the row.
fn choice(
    option: &v1::PermissionOption,
    primary: bool,
    state: &PermissionState,
    on_choose: EventHandler<v1::PermissionOptionId>,
) -> Element {
    let (label, tone) = kind(option.kind);
    let picked = matches!(
        state,
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(selected))
            if selected.option_id == option.option_id
    );
    let classes = match (tone, primary, picked) {
        // **The first allow is the filled one**, which is the drawing's own
        // hierarchy for a row of answers: one primary and the rest bounded.
        // Which one that is remains the *agent's* order and never a judgement —
        // the tone still tells allow from reject, and every option the agent
        // sent is on the row at the same size.
        ("allow", true, false) => "btn btn-filled option primary",
        ("allow", _, false) => "btn btn-quiet btn-go option",
        ("reject", _, false) => "btn btn-quiet btn-bad option",
        (_, _, false) => "btn btn-quiet option",
        (_, _, true) => "btn btn-filled option picked",
    };
    let state_name = match state {
        PermissionState::Waiting => "waiting",
        PermissionState::Answering => "sending",
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(_)) if picked => {
            "selected"
        }
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(_)) => "disabled",
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled) => "refused",
        PermissionState::Abandoned => "unavailable",
        _ => "unrecognized",
    };
    let accessible_name = format!("{}; option {}; {label}", option.name, option.option_id);
    let available = *state == PermissionState::Waiting;

    rsx! {
        button {
            key: "{option.option_id}",
            class: "{classes}",
            r#type: "button",
            "data-option": "{option.option_id}",
            "data-state": state_name,
            aria_label: accessible_name,
            aria_pressed: picked,
            // Native `disabled` can drop focus from a button. ARIA and tab
            // order make an answered option inert while the request settles.
            aria_disabled: (!available).then_some("true"),
            tabindex: (!available).then_some("-1"),
            onclick: {
                let option = option.option_id.clone();
                move |_| {
                    if available {
                        on_choose.call(option.clone());
                    }
                }
            },
            // The agent's name for it, and the id that will go on the wire
            // beside it — the kind is what the accessible name carries, because
            // a row of four buttons each saying its kind twice is the drawing's
            // own reason for keeping one of them.
            "{option.name}"
            span { class: "option-id", "{option.option_id}" }
        }
    }
}

/// Where the request stands, in a line.
///
/// Every state has one, including the two that are nobody's choice: a request
/// the turn ended without answering says so rather than looking like it is
/// still waiting for a click, and one answered `cancelled` says who cancelled
/// it.
fn said(state: &PermissionState) -> Element {
    match state {
        PermissionState::Waiting => rsx! {
            p { class: "detail detail-live", "The turn is stopped here until you answer." }
        },
        PermissionState::Answering => rsx! {
            p { class: "detail", "Sending the selected answer..." }
        },
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(selected)) => rsx! {
            p { class: "detail",
                "Sent selected answer "
                code { "selected" }
                " with "
                code { "{selected.option_id}" }
                ". The turn went on."
            }
        },
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled) => rsx! {
            p { class: "detail detail-warn",
                "The Turn was cancelled, so the Agent was answered "
                code { "cancelled" }
                ". This is what a client owes every request it leaves waiting."
            }
        },
        // Nobody answered it and nobody can: the turn it blocked ended, or the
        // agent went away holding it. Saying it plainly is the alternative to a
        // row of dead buttons explaining nothing.
        PermissionState::Abandoned => rsx! {
            p { class: "detail detail-warn",
                "Nobody answered this, and nobody can now. The Turn ended, the Agent went away, or an answer could not be sent."
            }
        },
        // Both enums are non-exhaustive, and a state this window has not been
        // taught must not be rendered as one it has.
        _ => rsx! {
            p { class: "detail", "A state this window has no rendering for." }
        },
    }
}

/// What an option kind is called, and the tone it reads in.
///
/// The protocol's own words: the reader of an inspector is reading ACP, and a
/// panel that renamed `reject_always` to "never ask again" would be a
/// translation they did not ask for.
fn kind(kind: v1::PermissionOptionKind) -> (&'static str, &'static str) {
    match kind {
        v1::PermissionOptionKind::AllowOnce => ("allow_once", "allow"),
        v1::PermissionOptionKind::AllowAlways => ("allow_always", "allow"),
        v1::PermissionOptionKind::RejectOnce => ("reject_once", "reject"),
        v1::PermissionOptionKind::RejectAlways => ("reject_always", "reject"),
        // `PermissionOptionKind` is non-exhaustive. A kind this window has not
        // been taught must not read as one it has — an unknown option rendered
        // as "allow" would be a guess with a green button's worth of confidence
        // behind it — and the option is still offered, because the agent
        // offered it.
        _ => ("a kind this window has no name for", "unnamed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(options: Vec<v1::PermissionOption>, state: PermissionState) -> String {
        #[component]
        fn Host(options: Vec<v1::PermissionOption>, state: PermissionState) -> Element {
            let on_choose = EventHandler::new(move |_: v1::PermissionOptionId| {});
            rsx! {
                div {
                    {said(&state)}
                    {choices(&options, &state, on_choose)}
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { options, state });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    fn options() -> Vec<v1::PermissionOption> {
        vec![
            v1::PermissionOption::new("yes", "Allow once", v1::PermissionOptionKind::AllowOnce),
            v1::PermissionOption::new(
                "always",
                "Always allow this operation even when its Agent-provided explanation wraps onto several lines",
                v1::PermissionOptionKind::AllowAlways,
            ),
            v1::PermissionOption::new("no", "Reject once", v1::PermissionOptionKind::RejectOnce),
            v1::PermissionOption::new(
                "never",
                "Always reject",
                v1::PermissionOptionKind::RejectAlways,
            ),
        ]
    }

    #[test]
    fn every_kind_the_spec_names_reads_as_itself() {
        // The four kinds §7.2 requires the panel to show, each named by the
        // protocol's word for it and toned by what it does. Allowing and
        // rejecting must never read alike: the two buttons that differ only in
        // `once`/`always` are the ones a reader picks between at speed.
        let named: Vec<_> = [
            v1::PermissionOptionKind::AllowOnce,
            v1::PermissionOptionKind::AllowAlways,
            v1::PermissionOptionKind::RejectOnce,
            v1::PermissionOptionKind::RejectAlways,
        ]
        .into_iter()
        .map(kind)
        .collect();

        assert_eq!(
            named,
            [
                ("allow_once", "allow"),
                ("allow_always", "allow"),
                ("reject_once", "reject"),
                ("reject_always", "reject"),
            ]
        );
    }

    #[test]
    fn every_agent_option_is_enabled_named_and_rendered_in_agent_order() {
        let html = shown(options(), PermissionState::Waiting);

        let positions = ["yes", "always", "no", "never"].map(|id| {
            html.find(&format!(r#"data-option="{id}""#))
                .unwrap_or_else(|| panic!("the Agent option {id} is rendered: {html}"))
        });
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "the options remain in Agent order: {html}"
        );
        assert_eq!(html.matches("<button").count(), 4, "{html}");
        assert!(
            html.contains(r#"aria-label="Allow once; option yes; allow_once""#),
            "the visible label, wire id and kind form a complete accessible name: {html}"
        );
        assert!(
            !html.contains("disabled"),
            "waiting choices remain keyboard operable: {html}"
        );
    }

    #[test]
    fn permission_interaction_states_remain_explicit_without_discarding_button_focus() {
        let selected = v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
            v1::PermissionOptionId::new("never"),
        ));
        let cases = [
            (PermissionState::Answering, "Sending the selected answer"),
            (PermissionState::Answered(selected), "Sent selected"),
            (
                PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
                "Agent was answered",
            ),
            (PermissionState::Abandoned, "or an answer could not be sent"),
        ];

        for (state, expected) in cases {
            let html = shown(options(), state);
            assert!(html.contains(expected), "{expected} is readable: {html}");
            assert_eq!(
                html.matches(r#"aria-disabled="true""#).count(),
                4,
                "every unavailable option says it is disabled: {html}"
            );
            assert_eq!(
                html.matches(r#"tabindex="-1""#).count(),
                4,
                "unavailable options leave future Tab order without dropping current focus: {html}"
            );
            assert!(
                !html.contains("disabled=true"),
                "the option nodes remain focusable by the browser until focus moves: {html}"
            );
        }

        let html = shown(
            options(),
            PermissionState::Answered(v1::RequestPermissionOutcome::Selected(
                v1::SelectedPermissionOutcome::new(v1::PermissionOptionId::new("never")),
            )),
        );
        let selected = html
            .split("<button")
            .find(|button| button.contains(r#"data-option="never""#))
            .expect("the selected option remains visible");
        assert!(
            selected.contains(r#"data-state="selected""#),
            "the option sent to the Agent remains explicit: {selected}"
        );
    }

    #[test]
    fn the_activated_option_stays_mounted_and_cannot_answer_twice() {
        use std::{any::Any, rc::Rc};

        use dioxus::core::Mutation;
        use dioxus::html::{PlatformEventData, SerializedMouseData};

        #[component]
        fn Host() -> Element {
            let mut state = use_signal(|| PermissionState::Waiting);
            let mut answers = use_signal(|| 0);
            let shown = state();
            rsx! {
                {choices(
                    &options(),
                    &shown,
                    EventHandler::new(move |_: v1::PermissionOptionId| {
                        answers += 1;
                        state.set(PermissionState::Answering);
                    }),
                )}
                span { "data-slot": "answer-count", "{answers}" }
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
        let initial = dom.rebuild_to_vec().edits;
        let option = initial
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .expect("a permission option");

        dom.runtime().handle_event("click", click(), option);
        let edits = dom.render_immediate_to_vec().edits;
        let replaced = edits.iter().any(|edit| {
            let target = match edit {
                Mutation::Remove { id }
                | Mutation::ReplaceWith { id, .. }
                | Mutation::NewEventListener { id, .. }
                | Mutation::RemoveEventListener { id, .. } => id,
                _ => return false,
            };
            target == &option
        });
        assert!(!replaced, "the focused option remains mounted: {edits:?}");
        assert!(
            dioxus_ssr::render(&dom).contains(r#"data-slot="answer-count">1"#),
            "the first activation answers"
        );

        dom.runtime().handle_event("click", click(), option);
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="answer-count">1"#),
            "ARIA-disabled options guard synthetic repeat activation: {html}"
        );
    }
}
