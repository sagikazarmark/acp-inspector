//! The inline permission panel: the request the turn stopped for, rendered in
//! the timeline where it stopped. See `docs/architecture.md` §7.2.
//!
//! Every button is an option the agent sent, in its order, and sends back the
//! option id unchanged. Whether the buttons are live is core's
//! [`PermissionState`], never worked out here.

use acp_inspector_core::{PermissionPolicy, PermissionRequest, PermissionState, v1};
use dioxus::prelude::*;

use crate::update;

/// The user's answer to a blocking request: the request, which carries its own
/// resolver, and the option they picked.
#[derive(Clone, PartialEq)]
pub struct Answer {
    pub request: PermissionRequest,
    pub option: v1::PermissionOptionId,
}

/// The card for a permission request: the method, the tool call in question,
/// and the agent's options.
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
            // Nothing in the card is a press of the Timeline row, whose click
            // reveals frames and moves focus.
            onclick: move |event| event.stop_propagation(),
            div { class: "blocking-head",
                span { class: "wire-name", "{v1::CLIENT_METHOD_NAMES.session_request_permission}" }
                span { class: "blocking-wait",
                    if waiting { "Waiting for you" } else { "Resolved" }
                }
            }
            div { class: "blocking-body",
                // Drawn as a tool call is drawn anywhere else.
                {update::tool_call_update(request.tool_call())}
                span { class: "blocking-meta", "{request.id()}" }

                {said(&state)}
                if let Some(policy) = request.by_policy() {
                    p { class: "detail", "data-slot": "by-policy",
                        "Answered automatically by policy ({policy_named(policy)})."
                    }
                }

                if request.options().is_empty() {
                    // A finding: the agent blocked its turn on a question nobody
                    // can answer.
                    p { class: "unnamed",
                        "The agent offered no options. Cancel the turn to end this."
                    }
                } else {
                    {choices(request.options(), &state, on_choose)}
                }
            }
        }
    }
}

/// Every option the agent supplied, in its order. None is drawn as the
/// default; an `*_always` option is tagged because it outlives the click.
fn choices(
    options: &[v1::PermissionOption],
    state: &PermissionState,
    on_choose: EventHandler<v1::PermissionOptionId>,
) -> Element {
    rsx! {
        div { class: "answers",
            for option in options.iter() {
                {choice(option, state, on_choose)}
            }
        }
    }
}

/// One option, as the button that sends its id.
fn choice(
    option: &v1::PermissionOption,
    state: &PermissionState,
    on_choose: EventHandler<v1::PermissionOptionId>,
) -> Element {
    let (label, tone) = kind(option.kind);
    let always = matches!(
        option.kind,
        v1::PermissionOptionKind::AllowAlways | v1::PermissionOptionKind::RejectAlways
    );
    let picked = matches!(
        state,
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(selected))
            if selected.option_id == option.option_id
    );
    let classes = match (tone, picked) {
        // Only the option that was sent is filled: a record, not a suggestion.
        (_, true) => "btn btn-filled option picked",
        ("allow", false) => "btn btn-quiet btn-go option",
        ("reject", false) => "btn btn-quiet btn-bad option",
        (_, false) => "btn btn-quiet option",
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
        PermissionState::CancelledByAgent => "withdrawn",
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
            "data-kind": label,
            "data-always": "{always}",
            aria_label: accessible_name,
            aria_pressed: picked,
            // Native `disabled` can drop focus from a button. ARIA and tab
            // order make an answered option inert while the request settles.
            aria_disabled: (!available).then_some("true"),
            tabindex: (!available).then_some("-1"),
            onclick: {
                let option = option.option_id.clone();
                move |event: Event<MouseData>| {
                    // Not a press of the row, whose click reveals its frames.
                    event.stop_propagation();
                    if available {
                        on_choose.call(option.clone());
                    }
                }
            },
            // The kind is carried by the accessible name, not drawn.
            "{option.name}"
            if always {
                span { class: "option-always", "data-slot": "always", aria_hidden: "true", "always" }
            }
            span { class: "option-id", "{option.option_id}" }
        }
    }
}

/// Where the request stands, in a line. Every state has one, including a
/// request the turn ended without answering.
fn said(state: &PermissionState) -> Element {
    match state {
        PermissionState::Waiting => rsx! {
            p { class: "detail detail-live", "The turn is stopped here until you answer." }
        },
        PermissionState::Answering => rsx! {
            p { class: "detail", "Sending…" }
        },
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(selected)) => rsx! {
            p { class: "detail",
                "Sent "
                code { "selected" }
                " with "
                code { "{selected.option_id}" }
                "."
            }
        },
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled) => rsx! {
            p { class: "detail detail-warn",
                "Turn cancelled. Answered "
                code { "cancelled" }
                "."
            }
        },
        // The turn it blocked ended, or the agent went away holding it.
        PermissionState::Abandoned => rsx! {
            p { class: "detail detail-warn",
                "Can no longer be answered: the turn ended or the agent disconnected."
            }
        },
        // The agent took the question back with `$/cancel_request`, and was
        // answered with the cancellation error.
        PermissionState::CancelledByAgent => rsx! {
            p { class: "detail detail-warn", "data-slot": "withdrawn",
                "The agent withdrew this request."
            }
        },
        // Both enums are non-exhaustive; an unknown state is not drawn as a
        // known one.
        _ => rsx! {
            p { class: "detail", "A state this window has no rendering for." }
        },
    }
}

/// A permission policy in the words its control uses.
pub fn policy_named(policy: PermissionPolicy) -> &'static str {
    match policy {
        PermissionPolicy::Ask => "ask",
        PermissionPolicy::AllowOnce => "allow once",
        PermissionPolicy::RejectOnce => "reject once",
    }
}

/// An option kind's protocol name, and the tone it reads in.
fn kind(kind: v1::PermissionOptionKind) -> (&'static str, &'static str) {
    match kind {
        v1::PermissionOptionKind::AllowOnce => ("allow_once", "allow"),
        v1::PermissionOptionKind::AllowAlways => ("allow_always", "allow"),
        v1::PermissionOptionKind::RejectOnce => ("reject_once", "reject"),
        v1::PermissionOptionKind::RejectAlways => ("reject_always", "reject"),
        // Non-exhaustive: an unknown kind gets no tone, and is still offered.
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
    fn a_request_the_agent_withdrew_says_so() {
        let html = shown(options(), PermissionState::CancelledByAgent);
        assert!(html.contains("The agent withdrew this request."), "{html}");
        assert!(html.contains(r#"data-state="withdrawn""#), "{html}");
        assert!(!html.contains("no rendering for"), "{html}");
    }

    #[test]
    fn every_policy_has_a_name_for_the_card() {
        assert_eq!(policy_named(PermissionPolicy::AllowOnce), "allow once");
        assert_eq!(policy_named(PermissionPolicy::RejectOnce), "reject once");
        assert_eq!(policy_named(PermissionPolicy::Ask), "ask");
    }

    #[test]
    fn every_kind_the_spec_names_reads_as_itself() {
        // The four kinds §7.2 requires, by protocol name; allow and reject
        // must never read alike.
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
            (PermissionState::Answering, "Sending…"),
            (
                PermissionState::Answered(selected),
                "with <code>never</code>",
            ),
            (
                PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
                "Turn cancelled. Answered <code>cancelled</code>",
            ),
            (PermissionState::Abandoned, "Can no longer be answered"),
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
    fn no_option_is_drawn_as_the_default_and_always_options_are_tagged() {
        let html = shown(options(), PermissionState::Waiting);
        assert!(
            !html.contains("btn-filled") && !html.contains("primary"),
            "an unanswered request recommends nothing: {html}"
        );
        let button = |id: &str| {
            html.split("<button")
                .find(|button| button.contains(&format!(r#"data-option="{id}""#)))
                .unwrap_or_else(|| panic!("the {id} option: {html}"))
                .split("</button>")
                .next()
                .unwrap()
                .to_owned()
        };
        for always in ["always", "never"] {
            assert!(
                button(always).contains(r#"data-slot="always""#)
                    && button(always).contains(r#"data-always="true""#),
                "a standing grant or refusal says so: {}",
                button(always)
            );
        }
        for once in ["yes", "no"] {
            assert!(!button(once).contains(r#"data-slot="always""#));
        }
    }

    #[test]
    fn answering_an_option_does_not_press_the_row_it_sits_in() {
        // The row around a request reveals its frames on click; an answer must
        // not. The plain span is the control showing the harness bubbles.
        use std::{any::Any, rc::Rc};

        use dioxus::core::{AttributeValue, Mutation};
        use dioxus::html::{PlatformEventData, SerializedMouseData};

        #[component]
        fn Host() -> Element {
            let mut answers = use_signal(|| 0);
            let mut reveals = use_signal(|| 0);
            // Dynamic, so the harness can find the element by it.
            let plain = "plain";
            rsx! {
                article { onclick: move |_| reveals += 1,
                    span { "data-slot": "{plain}", onclick: move |_| {}, "row text" }
                    {choices(
                        &options(),
                        &PermissionState::Waiting,
                        EventHandler::new(move |_: v1::PermissionOptionId| answers += 1),
                    )}
                }
                span { "data-slot": "counts", "{answers}/{reveals}" }
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
        let element = |slot: &str, value: &str| {
            edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name,
                        value: AttributeValue::Text(text),
                        id,
                        ..
                    } if *name == slot && text == value => Some(*id),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{slot}={value} in {edits:?}"))
        };

        dom.runtime()
            .handle_event("click", click(), element("data-option", "yes"));
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="counts">1/0"#),
            "the option answers and the row is not pressed: {html}"
        );

        dom.runtime()
            .handle_event("click", click(), element("data-slot", "plain"));
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        assert!(
            dioxus_ssr::render(&dom).contains(r#"data-slot="counts">1/1"#),
            "a click on the row's own text still reaches it"
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
