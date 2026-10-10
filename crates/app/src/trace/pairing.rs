//! Which frame answers which, as the list and the pane say it (F1).

use std::time::SystemTime;

use acp_inspector_core::{Counterpart, Direction, Frame, FrameKind, Trace, TracedFrame};
use dioxus::prelude::*;

use super::mistyped;
use crate::wire::{Elapsed, Wire, elapsed};

/// Whether a frame is an answer: a result or an error, carrying no method.
pub(super) fn is_answer(frame: &Frame) -> bool {
    let summary = frame.summary();
    matches!(summary.kind, FrameKind::Result | FrameKind::Error) && summary.method.is_none()
}

/// Which frames pair with which, read off the trace on screen.
pub(super) struct Pairs<'a> {
    pub(super) trace: Option<Trace>,
    pub(super) frames: &'a [TracedFrame],
    pub(super) dropped: usize,
}

impl<'a> Pairs<'a> {
    pub(super) fn counterpart(&self, ordinal: u64) -> Option<Counterpart> {
        self.trace.as_ref()?.counterpart(ordinal)
    }

    /// The frame captured under `ordinal`, where the trace still holds it.
    pub(super) fn frame(&self, ordinal: u64) -> Option<&'a TracedFrame> {
        let position = usize::try_from(ordinal).ok()?.checked_sub(self.dropped)?;
        self.frames.get(position)
    }

    /// For an answer, the method of the call it answers, where that call is
    /// still held.
    pub(super) fn answering(&self, ordinal: u64, traced: &TracedFrame) -> Option<&'a str> {
        if !is_answer(&traced.frame) {
            return None;
        }
        let request = self.counterpart(ordinal)?;
        self.frame(request.ordinal)?
            .frame
            .summary()
            .method
            .as_deref()
    }

    /// Whether a request has no answer in the trace.
    pub(super) fn unanswered(&self, ordinal: u64, traced: &TracedFrame) -> bool {
        self.trace.is_some()
            && traced.frame.summary().kind == FrameKind::Call
            && self.counterpart(ordinal).is_none()
    }
}

/// What the pane says about the other half of a request/response pair.
pub(super) enum Pairing {
    Nothing,
    /// A request, and the answer at this ordinal.
    Answered(Counterpart),
    /// An answer, and the request at this ordinal.
    Answers {
        request: Counterpart,
        /// Set where the id's type differs from the request's.
        mistyped: Option<String>,
    },
    /// A request nothing has answered yet, waiting since then — and the call
    /// to cancel, where this client can still give it up.
    Waiting {
        since: SystemTime,
        cancel: Option<i64>,
    },
    /// A request nothing in the trace answers, and nothing is waiting on.
    Unanswered,
}

pub(super) fn pairing(
    pairs: &Pairs<'_>,
    ordinal: u64,
    entry: &TracedFrame,
    wire: Option<Wire>,
) -> Pairing {
    let Some(wire) = wire else {
        return Pairing::Nothing;
    };
    let summary = entry.frame.summary();
    match pairs.counterpart(ordinal) {
        Some(other) if is_answer(&entry.frame) => Pairing::Answers {
            request: other,
            mistyped: other
                .mistyped
                .then(|| mistyped(&entry.frame, pairs.frame(other.ordinal))),
        },
        Some(other) if summary.kind == FrameKind::Call => Pairing::Answered(other),
        None if summary.kind == FrameKind::Call => {
            if let Some(call) = wire.waiting(ordinal) {
                Pairing::Waiting {
                    since: entry.at,
                    cancel: wire.live().then_some(call.id),
                }
            } else if entry.direction == Direction::FromAgent
                && wire.live()
                && pairs
                    .frames
                    .last()
                    .is_some_and(|last| last.connection == entry.connection)
            {
                // The agent is waiting on this client.
                Pairing::Waiting {
                    since: entry.at,
                    cancel: None,
                }
            } else {
                Pairing::Unanswered
            }
        }
        _ => Pairing::Nothing,
    }
}

impl Pairing {
    pub(super) fn render(self, go: std::rc::Rc<dyn Fn(u64)>, wire: Option<Wire>) -> Element {
        match self {
            Self::Nothing => rsx! {},
            Self::Answered(answer) => {
                let target = answer.ordinal;
                rsx! {
                    p { class: "frame-pair", "data-slot": "pairing",
                        "Response #{target} · +{elapsed(answer.elapsed)} "
                        button {
                            class: "btn btn-xs btn-quiet",
                            r#type: "button",
                            "data-slot": "go-response",
                            onclick: move |_| go(target),
                            "Go to response"
                        }
                    }
                }
            }
            Self::Answers { request, mistyped } => {
                let target = request.ordinal;
                rsx! {
                    p { class: "frame-pair", "data-slot": "pairing",
                        "Request #{target} · +{elapsed(request.elapsed)} "
                        button {
                            class: "btn btn-xs btn-quiet",
                            r#type: "button",
                            "data-slot": "go-request",
                            onclick: move |_| go(target),
                            "Go to request"
                        }
                    }
                    if let Some(said) = mistyped {
                        p { class: "bad", "data-slot": "mistyped", "{said}. JSON-RPC does not match it, so the request is still waiting." }
                    }
                }
            }
            Self::Waiting { since, cancel } => rsx! {
                p { class: "frame-pair", "data-slot": "pairing",
                    "No response yet · "
                    Elapsed { since }
                    if let (Some(wire), Some(id)) = (wire, cancel) {
                        " "
                        button {
                            class: "btn btn-xs btn-quiet btn-bad",
                            r#type: "button",
                            "data-slot": "cancel-call",
                            title: "Send $/cancel_request for call {id}. The call stays pending until the agent answers it.",
                            onclick: move |_| wire.cancel(id),
                            "Cancel request"
                        }
                    }
                }
            },
            Self::Unanswered => rsx! {
                p { class: "frame-pair", "data-slot": "pairing", "No response in the trace" }
            },
        }
    }
}
