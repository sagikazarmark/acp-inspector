//! The raw request composer (F2): a method, params as JSON, request or
//! notification — sent on the inspector's own connection with an id from its
//! own sequence, and the answer shown underneath, paired by the trace.
//!
//! **What is sent is what was typed.** The params are checked to be JSON and
//! sent as written (core only takes the line breaks out), so key order and
//! number spellings survive. Params that are valid JSON but not an object or an
//! array are sent anyway, with a caution: an inspector is where you find out
//! what an agent does with exactly that.
//!
//! **And nothing it sends changes what the window says the agent did**: core
//! marks the frame hand-sent and keeps its answer out of every store
//! (`Inspector::send_raw`).

use acp_inspector_core::{Direction, Frame, FrameKind, HandSent, Indentation, TracedFrame};
use dioxus::prelude::*;
use serde_json::value::RawValue;

use crate::wire::{Elapsed, Wire, elapsed};

/// What is in the composer's fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub method: String,
    /// The params as typed; empty sends none.
    pub params: String,
    pub notification: bool,
}

/// A draft that can be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    pub method: String,
    /// Valid JSON, as typed; `None` sends no `params` at all.
    pub params: Option<String>,
    pub notification: bool,
    /// Something worth saying before sending, which does not stop it.
    pub caution: Option<String>,
}

/// What became of the last send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    pub method: String,
    pub outcome: Result<HandSent, String>,
}

/// Whether a draft can be sent, and if not, why — in a sentence.
pub fn validate(draft: &Draft) -> Result<Ready, String> {
    let method = draft.method.trim();
    if method.is_empty() {
        return Err("Enter a method.".to_owned());
    }
    let params = draft.params.trim();
    let (params, caution) = if params.is_empty() {
        (None, None)
    } else {
        let parsed = serde_json::from_str::<&RawValue>(params)
            .map_err(|error| format!("Params aren't valid JSON: {error}."))?;
        let structured = matches!(parsed.get().trim_start().chars().next(), Some('{' | '['));
        let caution = (!structured).then(|| {
            "JSON-RPC expects params to be an object or an array. It will be sent as written."
                .to_owned()
        });
        (Some(params.to_owned()), caution)
    };
    Ok(Ready {
        method: method.to_owned(),
        params,
        notification: draft.notification,
        caution,
    })
}

impl Draft {
    /// A copy of a request or notification this client sent, laid out for
    /// editing; `None` for anything else.
    pub fn of(frame: &Frame) -> Option<Self> {
        let summary = frame.summary();
        let notification = match summary.kind {
            FrameKind::Call => false,
            FrameKind::Notification => true,
            _ => return None,
        };
        #[derive(serde::Deserialize)]
        struct Envelope<'a> {
            #[serde(borrow)]
            params: Option<&'a RawValue>,
        }
        let params = serde_json::from_str::<Envelope<'_>>(frame.as_str())
            .ok()
            .and_then(|envelope| envelope.params)
            .map(|params| Indentation::Indented.draw(params.get()).into_owned())
            .unwrap_or_default();
        Some(Self {
            method: summary.method.clone()?,
            params,
            notification,
        })
    }
}

/// Whether a row's frame is one "Edit and resend" can start from.
pub fn resendable(traced: &TracedFrame) -> bool {
    traced.direction == Direction::ToAgent
        && matches!(
            traced.frame.summary().kind,
            FrameKind::Call | FrameKind::Notification
        )
}

/// The composer, over the Trace.
#[component]
pub fn Composer(
    wire: Wire,
    frames: ReadSignal<Vec<TracedFrame>>,
    dropped: usize,
    /// A frame, on its way to being selected in the Trace.
    on_show: EventHandler<u64>,
) -> Element {
    let mut draft = wire.draft;
    let mut composing = wire.composing;
    let live = wire.live();
    let checked = validate(&draft.read());
    let typed = !draft.read().params.trim().is_empty();
    let reason = match (&checked, live) {
        (_, false) => Some("Connect an agent to send."),
        (Err(_), true) => None,
        (Ok(_), true) => None,
    };
    let sendable = live && checked.is_ok();
    let submit = checked.clone();
    let notification = draft.read().notification;

    rsx! {
        form {
            class: "raw-composer",
            "data-slot": "raw-composer",
            aria_label: "Send a frame by hand",
            onsubmit: move |event| {
                event.prevent_default();
                if let (true, Ok(ready)) = (sendable, submit.clone()) {
                    wire.send(ready);
                }
            },
            header { class: "raw-head",
                h3 { "Send by hand" }
                span { class: "hint",
                    "Sent on this connection with the next id and marked hand-sent. The answer doesn't change the session, turn or settings."
                }
                span { class: "grow" }
                button {
                    class: "btn btn-xs btn-quiet",
                    r#type: "button",
                    "data-slot": "raw-close",
                    onclick: move |_| composing.set(false),
                    "Close"
                }
            }
            div { class: "raw-fields",
                label { class: "raw-field",
                    span { "Method" }
                    input {
                        class: "input mono",
                        "data-slot": "raw-method",
                        r#type: "text",
                        spellcheck: false,
                        placeholder: "session/list",
                        value: "{draft.read().method}",
                        oninput: move |event| draft.write().method = event.value(),
                    }
                }
                div { class: "seg", role: "group", aria_label: "What to send", "data-slot": "raw-kind",
                    button {
                        class: "seg-item",
                        r#type: "button",
                        aria_pressed: !notification,
                        title: "Sent with an id; the agent answers it",
                        onclick: move |_| draft.write().notification = false,
                        "Request"
                    }
                    button {
                        class: "seg-item",
                        r#type: "button",
                        aria_pressed: notification,
                        title: "Sent without an id; nothing answers it",
                        onclick: move |_| draft.write().notification = true,
                        "Notification"
                    }
                }
                label { class: "raw-field raw-params",
                    span { "Params (JSON, optional)" }
                    textarea {
                        class: "textarea mono",
                        "data-slot": "raw-params",
                        rows: "8",
                        spellcheck: false,
                        placeholder: "{{}}",
                        value: "{draft.read().params}",
                        oninput: move |event| draft.write().params = event.value(),
                    }
                }
            }
            match &checked {
                Err(problem) if typed || !draft.read().method.trim().is_empty() => rsx! {
                    p { class: "bad", "data-slot": "raw-problem", role: "status", "{problem}" }
                },
                Ok(Ready { caution: Some(caution), .. }) => rsx! {
                    p { class: "hint", "data-slot": "raw-caution", "{caution}" }
                },
                _ => rsx! {},
            }
            div { class: "raw-actions",
                button {
                    class: "btn btn-sm btn-filled",
                    r#type: "submit",
                    "data-slot": "raw-send",
                    aria_disabled: (!sendable).then_some("true"),
                    if notification { "Send notification" } else { "Send request" }
                }
                if let Some(reason) = reason {
                    span { class: "hint", "{reason}" }
                }
            }
            {answered(wire, &frames.read(), dropped, on_show)}
        }
    }
}

/// What came of the last send: the frame, and the answer paired with it.
fn answered(
    wire: Wire,
    frames: &[TracedFrame],
    dropped: usize,
    on_show: EventHandler<u64>,
) -> Element {
    let Some(sent) = wire.sent.read().clone() else {
        return rsx! {};
    };
    let handed = match sent.outcome {
        Err(problem) => {
            return rsx! {
                p { class: "bad", "data-slot": "raw-result", role: "status",
                    "Not sent: {problem}"
                }
            };
        }
        Ok(handed) => handed,
    };
    let at = |ordinal: u64| {
        usize::try_from(ordinal)
            .ok()
            .and_then(|ordinal| ordinal.checked_sub(dropped))
            .and_then(|position| frames.get(position))
    };
    let request = handed.captured;
    let sent_line = match handed.id {
        Some(id) => format!("Sent {} request #{id}.", sent.method),
        None => format!("Sent {} notification.", sent.method),
    };
    let answer = request.and_then(|ordinal| wire.counterpart(ordinal));
    let waiting = request.and_then(|ordinal| wire.waiting(ordinal));
    let since = request.and_then(at).map(|traced| traced.at);

    rsx! {
        div { class: "raw-result", "data-slot": "raw-result", role: "status",
            p {
                "{sent_line} "
                if let Some(ordinal) = request {
                    button {
                        class: "btn btn-xs btn-quiet",
                        r#type: "button",
                        "data-slot": "raw-show-request",
                        onclick: move |_| on_show.call(ordinal),
                        "Show in trace"
                    }
                }
            }
            if handed.id.is_some() {
                match answer {
                    Some(answer) => {
                        let ordinal = answer.ordinal;
                        let body = at(ordinal).map(|traced| traced.frame.clone());
                        rsx! {
                            p { "data-slot": "raw-answer",
                                "Response #{ordinal} · +{elapsed(answer.elapsed)} "
                                button {
                                    class: "btn btn-xs btn-quiet",
                                    r#type: "button",
                                    onclick: move |_| on_show.call(ordinal),
                                    "Show in trace"
                                }
                            }
                            if let Some(body) = body {
                                pre { class: "frame-json",
                                    for (position, (token, text)) in crate::json::tokens(&Indentation::Indented.draw(crate::page::prefix(body.as_str(), 16 * 1024))).into_iter().enumerate() {
                                        span { key: "{position}", class: token.class(), "{text}" }
                                    }
                                }
                            }
                        }
                    }
                    None => rsx! {
                        p { "data-slot": "raw-waiting",
                            "No response yet"
                            if let Some(since) = since {
                                " · "
                                Elapsed { since }
                            }
                            if let Some(call) = waiting {
                                " "
                                button {
                                    class: "btn btn-xs btn-quiet btn-bad",
                                    r#type: "button",
                                    "data-slot": "cancel-call",
                                    title: "Send $/cancel_request for this call",
                                    onclick: move |_| wire.cancel(call.id),
                                    "Cancel request"
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(method: &str, params: &str) -> Draft {
        Draft {
            method: method.to_owned(),
            params: params.to_owned(),
            notification: false,
        }
    }

    #[test]
    fn a_draft_is_sendable_when_it_names_a_method_and_its_params_are_json() {
        assert_eq!(validate(&draft("", "")), Err("Enter a method.".to_owned()));
        assert_eq!(
            validate(&draft("  ", "{}")),
            Err("Enter a method.".to_owned())
        );
        let none = validate(&draft(" session/list ", "  ")).unwrap();
        assert_eq!(none.method, "session/list");
        assert_eq!(
            none.params, None,
            "empty params are left out, not sent as null"
        );
        let object = validate(&draft("x", "{\n \"b\": 1.0, \"a\": [] }")).unwrap();
        assert_eq!(
            object.params.as_deref(),
            Some("{\n \"b\": 1.0, \"a\": [] }")
        );
        assert_eq!(object.caution, None);
        let broken = validate(&draft("x", "{\"a\": }")).unwrap_err();
        assert!(broken.starts_with("Params aren't valid JSON"), "{broken}");
        // Valid but not structured: sent anyway, with a caution.
        let scalar = validate(&draft("x", "42")).unwrap();
        assert!(scalar.caution.unwrap().contains("object or an array"));
    }

    #[test]
    fn edit_and_resend_starts_from_the_frame_as_sent() {
        let call = Frame::new(
            r#"{"jsonrpc":"2.0","id":4,"method":"session/list","params":{"z":1,"a":{"b":2}}}"#,
        );
        let draft = Draft::of(&call).unwrap();
        assert_eq!(draft.method, "session/list");
        assert!(!draft.notification);
        assert!(
            draft.params.starts_with("{\n  \"z\": 1"),
            "{}",
            draft.params
        );
        let note = Draft::of(&Frame::new(
            r#"{"jsonrpc":"2.0","method":"session/cancel"}"#,
        ))
        .unwrap();
        assert!(note.notification && note.params.is_empty());
        assert!(Draft::of(&Frame::new(r#"{"jsonrpc":"2.0","id":4,"result":{}}"#)).is_none());
    }

    #[test]
    fn the_composer_says_why_it_cannot_send() {
        fn host() -> Element {
            let wire = use_hook(|| {
                let mut wire = Wire::detached(acp_inspector_core::Trace::default());
                wire.draft.set(Draft {
                    method: "x".to_owned(),
                    params: "{oops".to_owned(),
                    notification: true,
                });
                wire
            });
            let frames = use_signal(Vec::<TracedFrame>::new);
            rsx! { Composer { wire, frames, dropped: 0, on_show: move |_| {} } }
        }
        let mut dom = VirtualDom::new(host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(r#"data-slot="raw-problem""#), "{html}");
        assert!(html.contains("Connect an agent to send."), "{html}");
        assert!(html.contains("Send notification"), "{html}");
        assert!(html.contains(r#"aria-disabled="true""#), "{html}");
    }
}
