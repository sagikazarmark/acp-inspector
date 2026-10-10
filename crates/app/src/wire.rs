//! What the Console needs of the connection beyond the frames themselves:
//! which frame answers which (F1), which calls are still waiting and a way to
//! give one up (F4), a way to send a frame by hand (F2), and which trace is
//! being shown — the live one, or one opened from a file (F5).
//!
//! **One value, made once by the window** ([`use_wire`]) and handed to the
//! Console, so the panel's props stay a rendering of stores: every answer here
//! is core's (`Trace::counterpart`, `Inspector::pending_calls`,
//! `Inspector::cancel_call`, `Inspector::send_raw`), read or called and never
//! worked out again on this side.

use std::time::{Duration, SystemTime};

use acp_inspector_core::{
    ConnectionStatus, Counterpart, Frame, HandSent, Inspector, PendingCall, Trace,
};
use dioxus::prelude::*;

use crate::raw::{Draft, Ready, Sent};
use crate::view::View;

/// What became of the last "Cancel request".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cancelled {
    pub id: i64,
    pub outcome: Result<(), String>,
}

/// The connection's tools, as the Console uses them. Copy: every field is a
/// signal or a handle.
#[derive(Clone, Copy, PartialEq)]
pub struct Wire {
    inspector: Option<CopyValue<Inspector>>,
    live: CopyValue<Trace>,
    pub view: View,
    pub pending: Signal<Vec<PendingCall>>,
    pub(crate) status: Signal<ConnectionStatus>,
    pub cancelled: Signal<Option<Cancelled>>,
    pub draft: Signal<Draft>,
    pub composing: Signal<bool>,
    pub sent: Signal<Option<Sent>>,
    /// Whether an export carries the agent's stderr (F5).
    pub include_stderr: Signal<bool>,
    /// Whether the Trace interleaves the agent's stderr lines (F14).
    pub merged: Signal<bool>,
    /// Whether the Trace shows only calls still waiting for an answer (F1).
    pub pending_only: Signal<bool>,
}

/// The window's wire: the live inspector, watched for pending calls.
pub fn use_wire(inspector: Inspector, status: Signal<ConnectionStatus>) -> Wire {
    let view = crate::view::use_view(status.into());
    let wire = use_hook(|| Wire {
        inspector: Some(CopyValue::new(inspector.clone())),
        ..Wire::with(inspector.trace().clone(), view, status)
    });

    // Re-read whenever the trace moves — a call goes out, an answer comes
    // back — and whenever the connection does. A little after the frame,
    // because the reader records a frame before it hands the answer on.
    let watched = inspector.clone();
    use_future(move || {
        let inspector = watched.clone();
        async move {
            let mut changes = inspector.trace().changes();
            while changes.next().await.is_some() {
                tokio::time::sleep(Duration::from_millis(50)).await;
                wire.refresh(&inspector);
            }
        }
    });
    use_effect(move || {
        status();
        if let Some(inspector) = wire.inspector {
            wire.refresh(&inspector.read());
        }
    });
    wire
}

impl Wire {
    /// A wire over one trace and nothing live: what tests and fixtures draw
    /// the Console with. Signals of the scope it is made in.
    #[cfg(test)]
    pub fn detached(trace: Trace) -> Self {
        Self::with(
            trace,
            View::new(),
            Signal::new(ConnectionStatus::Disconnected),
        )
    }

    fn with(trace: Trace, view: View, status: Signal<ConnectionStatus>) -> Self {
        Self {
            inspector: None,
            live: CopyValue::new(trace),
            view,
            pending: Signal::new(Vec::new()),
            status,
            cancelled: Signal::new(None),
            draft: Signal::new(Draft::default()),
            composing: Signal::new(false),
            sent: Signal::new(None),
            include_stderr: Signal::new(false),
            merged: Signal::new(false),
            pending_only: Signal::new(false),
        }
    }

    fn refresh(mut self, inspector: &Inspector) {
        let now = inspector.pending_calls();
        if *self.pending.peek() != now {
            self.pending.set(now);
        }
    }

    /// The trace on screen: the opened file's, or the live one.
    pub fn trace(&self) -> Trace {
        self.view
            .trace()
            .unwrap_or_else(|| self.live.read().clone())
    }

    /// The frame that answers, or is answered by, the one at `ordinal`.
    pub fn counterpart(&self, ordinal: u64) -> Option<Counterpart> {
        self.trace().counterpart(ordinal)
    }

    /// Whether frames can be sent and calls cancelled: an agent is connected
    /// and no file is being shown in its place.
    pub fn live(&self) -> bool {
        (self.status)() == ConnectionStatus::Connected && !self.view.is_open()
    }

    /// The call still waiting whose request the trace put at `ordinal`.
    pub fn waiting(&self, ordinal: u64) -> Option<PendingCall> {
        if self.view.is_open() {
            return None;
        }
        self.pending
            .read()
            .iter()
            .find(|call| call.captured == Some(ordinal))
            .cloned()
    }

    /// The calls still waiting, unless a file is shown.
    pub fn waiting_calls(&self) -> Vec<PendingCall> {
        if self.view.is_open() {
            Vec::new()
        } else {
            self.pending.read().clone()
        }
    }

    /// Asks the agent to give up a call (`$/cancel_request`), and says how it
    /// went.
    pub fn cancel(mut self, id: i64) {
        let Some(inspector) = self.inspector else {
            return;
        };
        let inspector = inspector.read().clone();
        self.cancelled.set(None);
        spawn(async move {
            let outcome = inspector
                .cancel_call(id)
                .await
                .map_err(|error| error.to_string());
            self.cancelled.set(Some(Cancelled { id, outcome }));
        });
    }

    /// Sends what the raw composer holds.
    pub fn send(mut self, ready: Ready) {
        let Some(inspector) = self.inspector else {
            return;
        };
        let inspector = inspector.read().clone();
        self.sent.set(None);
        spawn(async move {
            let params = match ready
                .params
                .as_deref()
                .map(serde_json::from_str::<&serde_json::value::RawValue>)
            {
                Some(Err(error)) => {
                    self.sent.set(Some(Sent {
                        method: ready.method,
                        outcome: Err(error.to_string()),
                    }));
                    return;
                }
                Some(Ok(params)) => Some(params.to_owned()),
                None => None,
            };
            let outcome: Result<HandSent, String> = inspector
                .send_raw(&ready.method, params.as_deref(), ready.notification)
                .await
                .map_err(|error| error.to_string());
            self.sent.set(Some(Sent {
                method: ready.method,
                outcome,
            }));
        });
    }

    /// Opens the composer on a copy of a frame this client sent.
    pub fn resend(mut self, frame: &Frame) {
        if let Some(draft) = Draft::of(frame) {
            self.draft.set(draft);
            self.sent.set(None);
            self.composing.set(true);
        }
    }

    /// The agent the trace on screen is of, for naming an export.
    pub fn agent(&self) -> Option<String> {
        if self.view.is_open() {
            return self.view.agent();
        }
        let inspector = self.inspector?;
        let agent = inspector.read().agent()?;
        agent.agent_info.map(|info| info.name.to_string())
    }

    /// The agent's stderr lines for the trace on screen.
    pub fn stderr(&self) -> Vec<acp_inspector_core::Diagnostic> {
        if self.view.is_open() {
            return self.view.stderr();
        }
        self.inspector
            .map(|inspector| inspector.read().diagnostics().entries())
            .unwrap_or_default()
    }
}

/// The palette's commands, and the Console's tools among them: open or close
/// a trace file, and send a frame by hand. `show_trace` puts the Trace tab in
/// front, where the composer is drawn.
pub fn with_commands(
    wire: Wire,
    show_trace: EventHandler<()>,
    mut commands: Vec<crate::palette::Command>,
) -> Vec<crate::palette::Command> {
    use crate::palette::Command;
    if crate::view::CAN_OPEN {
        commands.push(
            Command::new(
                "Trace",
                "Open trace…",
                "Open an exported trace file, read-only",
                EventHandler::new(move |()| wire.view.choose(wire.live())),
            )
            .shortcut(crate::palette::shortcut::OPEN_TRACE),
        );
    }
    if wire.view.is_open() {
        commands.push(Command::new(
            "Trace",
            "Close trace",
            "Stop viewing the file and go back to the live window",
            EventHandler::new(move |()| wire.view.close()),
        ));
    }
    if wire.live() {
        commands.push(Command::new(
            "Trace",
            "Send by hand…",
            "Write a request or notification and send it to the agent",
            EventHandler::new(move |()| {
                let mut composing = wire.composing;
                composing.set(true);
                show_trace.call(());
            }),
        ));
    }
    commands
}

/// A span of time, as short as it can be said: `412 ms`, `3.2 s`, `2 min 5 s`,
/// `1 h 3 min`.
pub fn elapsed(span: Duration) -> String {
    let millis = span.as_millis();
    if millis < 1000 {
        format!("{millis} ms")
    } else if millis < 60_000 {
        format!("{}.{} s", millis / 1000, millis % 1000 / 100)
    } else if millis < 3_600_000 {
        let seconds = millis / 1000;
        format!("{} min {} s", seconds / 60, seconds % 60)
    } else {
        let minutes = millis / 60_000;
        format!("{} h {} min", minutes / 60, minutes % 60)
    }
}

/// How long ago `since` was, said again four times a second for as long as it
/// is on screen — which is only while something is waiting.
#[component]
pub fn Elapsed(since: SystemTime) -> Element {
    let mut tick = use_signal(|| 0_u64);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            tick += 1;
        }
    });
    tick.read();
    let span = SystemTime::now().duration_since(since).unwrap_or_default();
    rsx! {
        span { class: "elapsed", "data-slot": "elapsed", "{elapsed(span)}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_is_said_as_briefly_as_it_can_be() {
        for (millis, said) in [
            (0, "0 ms"),
            (412, "412 ms"),
            (999, "999 ms"),
            (1000, "1.0 s"),
            (3_249, "3.2 s"),
            (59_999, "59.9 s"),
            (125_000, "2 min 5 s"),
            (3_780_000, "1 h 3 min"),
        ] {
            assert_eq!(elapsed(Duration::from_millis(millis)), said);
        }
    }
}
