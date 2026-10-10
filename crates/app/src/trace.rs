//! The trace view (`docs/architecture.md` §9): every frame, both directions,
//! timestamped — and the one a reader selected, read whole in the pane under
//! the list.
//!
//! It is a body of the Console's Trace tab rather than a panel of its own
//! (`console::Console`), which is where its tabs, its counts, its filters and
//! its two verbs live. Everything else about it is unchanged: the panel that
//! holds it does not decide what a frame is or how one is drawn.
//!
//! **Raw always** (§8). The row shows what the envelope says it is and the
//! frame's own first line; the pane under it shows the same text again, whole.
//! Nothing here parses, decodes or rewrites what crossed the wire, because a
//! frame the inspector could not parse is exactly the frame its user came to
//! see — the pane draws one of those in one colour and every byte of it
//! (`crate::json`).
//!
//! **A row is selected rather than expanded.** It was a `<details>` per row for
//! five rings, which is the right shape for a full-width panel under the turn
//! and the wrong one beside it: a frame opened in place pushes every row under
//! it down the list a reader is watching arrive, and the width a payload needs
//! is not the width a column of rows has. One pane, at the foot of the list,
//! showing the frame the reader pointed at — and the newest displayed frame until they
//! point at one, because an empty pane under a live trace is a region asking to
//! be told something it can already see (ADR 0009).
//!
//! **The pane lays the payload out, and that is the third exception to the
//! Indentation switch** (`crate::indent`): the switch exists so that a list of
//! ten thousand frames does not lay out ten thousand documents for nobody, and
//! this is one document, selected, on the surface whose whole subject it is.
//! What leaves the window is untouched either way — the copy beside it takes the
//! bytes that crossed, and so does the export.
//!
//! **The buttons are buttons.** What an export contains, where it is written,
//! what clearing means for one already taken and how many frames a trace keeps
//! are all core's, asserted in `crates/core/tests/export.rs`.
//!
//! **A request and its answer point at each other** (F1). Which frame answers
//! which is core's (`Trace::counterpart`), read here and never worked out
//! again: an answer row says how long it took, the pane says where the other
//! half is and goes there, and a call nobody has answered says so — with the
//! time it has been waiting, ticking only while it is.
//!
//! **And the traffic the trace holds that is not an ordinary frame says what
//! it is**: a frame that was not UTF-8 is drawn decoded with a badge, a frame
//! the transport dropped for its length is a placeholder that says how long it
//! was, and a frame a person sent by hand is tagged as one.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use acp_inspector_core::{
    Counterpart, Diagnostic, DiagnosticKind, Direction, Frame, FrameKind, Indentation, Omission,
    Trace, TracedFrame,
};
use dioxus::prelude::*;

use crate::console::{Directions, contains_folded, narrowed};
use crate::copy::Copy;
use crate::disclosure::{Role, icon};
use crate::export::Outcome;
use crate::json;
use crate::time::stamp;
use crate::wire::{Elapsed, Wire, elapsed};

/// The frame list, by the name the scroll watcher and its control know it under
/// (`tail.rs`).
pub(crate) const FRAMES: &str = "frames";

/// Which of the envelope's four shapes a reader has asked to see.
///
/// **Presentation and nothing else** (§5): the trace goes on capturing,
/// counting and exporting everything, and the line above the rows says what is
/// being hidden. The five are the four JSON-RPC shapes plus the frames in none
/// of them, because a chip for every shape and none for the leftovers would be
/// a surface that can hide traffic with nothing to press to bring it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kinds {
    call: bool,
    result: bool,
    notification: bool,
    error: bool,
    unreadable: bool,
}

impl Default for Kinds {
    /// Everything, which is the only honest state for a surface to open in.
    fn default() -> Self {
        Self {
            call: true,
            result: true,
            notification: true,
            error: true,
            unreadable: true,
        }
    }
}

impl Kinds {
    /// The chips, in the order the envelope's own shapes are usually read.
    pub const ALL: [FrameKind; 5] = [
        FrameKind::Call,
        FrameKind::Result,
        FrameKind::Notification,
        FrameKind::Error,
        FrameKind::Unreadable,
    ];

    pub fn shows(self, kind: FrameKind) -> bool {
        match kind {
            FrameKind::Call => self.call,
            FrameKind::Result => self.result,
            FrameKind::Notification => self.notification,
            FrameKind::Error => self.error,
            // `FrameKind` is non-exhaustive, and a shape this window has not
            // been taught is shown with the ones it could not read: the chip
            // that hides them says so in the same word.
            _ => self.unreadable,
        }
    }

    /// The same set with one chip turned over.
    #[must_use]
    pub fn toggled(mut self, kind: FrameKind) -> Self {
        let held = match kind {
            FrameKind::Call => &mut self.call,
            FrameKind::Result => &mut self.result,
            FrameKind::Notification => &mut self.notification,
            FrameKind::Error => &mut self.error,
            _ => &mut self.unreadable,
        };
        *held = !*held;
        self
    }
}

/// What a shape is called on the chip that hides it, and on the row.
///
/// **JSON-RPC's own four nouns**, which is what the drawing labels these rows
/// with: a frame carrying a method and an id is a Request, the frame answering
/// it is a Response, one with a method and no id is a Notification, and one
/// carrying `error` is an Error. The window said *call* and *result* for five
/// rings, which are the same four shapes under words the specification does not
/// use — and a reader checking this list against the specification had to
/// translate one of them.
pub fn named_kind(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Call => "request",
        FrameKind::Result => "response",
        FrameKind::Notification => "notif",
        FrameKind::Error => "error",
        _ => "other",
    }
}

/// The class a shape's word is drawn in, which is the one thing on the row that
/// is a colour rather than a word.
fn kind_tone(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Result => "result",
        FrameKind::Error => "error",
        FrameKind::Notification => "notification",
        FrameKind::Call => "call",
        _ => "unreadable",
    }
}

/// A size, as a reader compares it: bytes below a kibibyte, else one decimal.
pub fn size(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    let bytes_f = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes_f < KIB * KIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{:.1} MiB", bytes_f / (KIB * KIB))
    }
}

/// What a dropped frame's placeholder says in place of the frame.
pub fn omitted(omission: Omission) -> String {
    format!(
        "Frame of {} dropped (limit {})",
        size(omission.length),
        size(omission.limit)
    )
}

/// Whether a frame is an answer: a result or an error, carrying no method.
fn is_answer(frame: &Frame) -> bool {
    let summary = frame.summary();
    matches!(summary.kind, FrameKind::Result | FrameKind::Error) && summary.method.is_none()
}

/// Which frames pair with which, read off the trace on screen.
struct Pairs<'a> {
    trace: Option<Trace>,
    frames: &'a [TracedFrame],
    dropped: usize,
}

impl<'a> Pairs<'a> {
    fn counterpart(&self, ordinal: u64) -> Option<Counterpart> {
        self.trace.as_ref()?.counterpart(ordinal)
    }

    /// The frame captured under `ordinal`, where the trace still holds it.
    fn frame(&self, ordinal: u64) -> Option<&'a TracedFrame> {
        let position = usize::try_from(ordinal).ok()?.checked_sub(self.dropped)?;
        self.frames.get(position)
    }

    /// For an answer, the method of the call it answers, where that call is
    /// still held.
    fn answering(&self, ordinal: u64, traced: &TracedFrame) -> Option<&'a str> {
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
    fn unanswered(&self, ordinal: u64, traced: &TracedFrame) -> bool {
        self.trace.is_some()
            && traced.frame.summary().kind == FrameKind::Call
            && self.counterpart(ordinal).is_none()
    }
}

/// One row of the merged view (F14): a frame, or a line of the agent's stderr.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// By position in the frames held.
    Frame(usize),
    /// By position in the diagnostic lines held.
    Line(usize),
}

/// Frames and stderr lines in one order, by when each was read, each with a
/// key that stays the same as either list ages out: the number of frames and
/// lines captured before it, which counts up by one along the merged list.
///
/// The frames keep the trace's order (it is authoritative); a line goes in
/// before the first frame read after it.
fn merge(
    frames: &[TracedFrame],
    dropped: usize,
    lines: &[Diagnostic],
    dropped_lines: usize,
) -> Vec<(u64, Row)> {
    let stderr: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| matches!(line.kind, DiagnosticKind::Stderr(_)))
        .map(|(position, _)| position)
        .collect();
    let mut merged = Vec::with_capacity(frames.len() + stderr.len());
    let (mut frame, mut line) = (0, 0);
    while frame < frames.len() || line < stderr.len() {
        let take_line = match (frames.get(frame), stderr.get(line)) {
            (Some(traced), Some(&position)) => lines[position].at < traced.at,
            (None, Some(_)) => true,
            _ => false,
        };
        // Every line before this one counts, stderr or not, so the key is the
        // same whichever kinds of line age out.
        let lines_before = stderr.get(line).copied().unwrap_or(lines.len());
        let key = (dropped + frame + dropped_lines + lines_before) as u64;
        if take_line {
            merged.push((key, Row::Line(stderr[line])));
            line += 1;
        } else {
            merged.push((key, Row::Frame(frame)));
            frame += 1;
        }
    }
    merged
}

/// The frames, oldest at the top, following the newest as they arrive — and the
/// selected one, whole, underneath.
#[component]
pub fn TraceView(
    frames: ReadSignal<Vec<TracedFrame>>,
    /// How many frames the trace captured and no longer holds — cleared, or
    /// aged out of its cap. Said on the tab rather than here, because a count
    /// that is not the whole session should never look like one; what it does
    /// in this body is number the rows.
    dropped: usize,
    /// What became of the last export: the file it wrote, or why it did not.
    saved: Option<Outcome>,
    /// Puts that notice away.
    #[props(default)]
    on_dismiss_saved: Option<EventHandler<()>>,
    /// What the Console's bar is narrowing this surface to. Presentation only:
    /// the Trace goes on capturing, counting and exporting everything.
    /// `method:NAME` and `id:N` match those envelope fields; the rest is text.
    filter: String,
    /// And which envelope shapes it is showing, from the chips beside the
    /// filter.
    kinds: Kinds,
    /// And which directions (F8).
    #[props(default)]
    directions: Directions,
    /// The frames a Timeline entry was made of, sent here by a reader who was
    /// reading that entry (§9's two screens, asked to point at each other).
    /// Marked and scrolled to; nothing is filtered away, because what a reader
    /// asked to *see* is not a reason to stop showing them the traffic around
    /// it.
    revealed: ReadSignal<Vec<u64>>,
    /// Which captured frames became a Timeline entry, so a row can say that it
    /// did. Not a judgement about the frame: an answer to a call this window
    /// made is no less real for having no entry.
    decoded: ReadSignal<HashSet<u64>>,
    /// The frame the pane is reading, or `None` while nobody has chosen one —
    /// which the pane answers with the newest displayed frame rather than with an empty
    /// region.
    selected: Option<u64>,
    on_choose: EventHandler<u64>,
    /// A row's frame, on its way back to the entry it became.
    on_seek: EventHandler<u64>,
    /// A narrowing the pane offers ("only this method", "only this id"), on
    /// its way to the Console's filter box.
    #[props(default)]
    on_filter: Option<EventHandler<String>>,
    /// The connection's tools: pairing, pending calls, cancelling and sending
    /// by hand. Without one the rows are drawn unpaired.
    #[props(default)]
    wire: Option<Wire>,
    /// The diagnostic lines, for the merged view (F14).
    #[props(default)]
    lines: Option<ReadSignal<Vec<Diagnostic>>>,
    #[props(default)] dropped_lines: usize,
) -> Element {
    let mut anchor = use_signal(|| revealed.read().first().copied());
    use_effect(move || {
        if let Some(first) = revealed.read().first().copied() {
            anchor.set(Some(first));
        }
    });
    // Which frames match the filter's text, remembered between renders (L1).
    let index = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(Index::default())));
    let held_frames = frames.read();
    let empty = held_frames.is_empty();
    let held = held_frames.len();
    let query = Query::parse(&filter);
    let pairs = Pairs {
        trace: wire.map(|wire| wire.trace()),
        frames: &held_frames,
        dropped,
    };
    let pending_only = wire.is_some_and(|wire| (wire.pending_only)());
    let merged_on = wire.is_some_and(|wire| (wire.merged)()) && lines.is_some();
    let mut indexed = index.borrow_mut();
    indexed.update(dropped, &query);
    // Which rows survive the narrowings, by their place in the list. Computed
    // once: the list pages them, and the pane reads the newest on that page —
    // a pane reading a frame the reader has hidden would be the one surface
    // answering a narrowing with the row it just took away.
    let survives = |position: usize, traced: &TracedFrame, indexed: &mut Index| {
        let ordinal = (dropped + position) as u64;
        kinds.shows(traced.frame.summary().kind)
            && directions.shows(traced.direction)
            && (!pending_only || pairs.unanswered(ordinal, traced))
            && indexed.matches((ordinal, traced), &query, &pairs)
    };
    let visible: Vec<usize> = held_frames
        .iter()
        .enumerate()
        .filter(|(position, traced)| survives(*position, traced, &mut indexed))
        .map(|(position, _)| position)
        .collect();
    drop(indexed);
    let shown = visible.len();

    // The rows the list draws: the visible frames, or — merged — those and the
    // agent's stderr lines that match the text, in the order they were read.
    let held_lines = match lines {
        Some(lines) if merged_on => lines.read().clone(),
        _ => Vec::new(),
    };
    let rows: Vec<(u64, Row)> = if merged_on {
        let kept: HashSet<usize> = visible.iter().copied().collect();
        merge(&held_frames, dropped, &held_lines, dropped_lines)
            .into_iter()
            .filter(|(_, row)| match row {
                Row::Frame(position) => kept.contains(position),
                Row::Line(position) => {
                    query.method.is_none()
                        && query.id.is_none()
                        && !pending_only
                        && contains_folded(&held_lines[*position].to_string(), &query.text)
                }
            })
            .collect()
    } else {
        visible
            .iter()
            .map(|&position| ((dropped + position) as u64, Row::Frame(position)))
            .collect()
    };
    let ordinals: Vec<u64> = rows.iter().map(|(key, _)| *key).collect();
    let range = crate::page::range(&ordinals, anchor());
    // What the surface is hiding, said where the rows it hid would have been —
    // whether the narrowing was typed or pressed, because a chip that is off
    // takes rows away exactly as a filter does.
    let narrowing = !filter.trim().is_empty()
        || kinds != Kinds::default()
        || directions != Directions::default()
        || pending_only;
    // Connection ordinals only count from one agent to the next, so a trace of
    // one agent has nothing to tell its rows apart by (D11).
    let several = held_frames
        .first()
        .zip(held_frames.last())
        .is_some_and(|(first, last)| first.connection != last.connection);
    let page = &rows[range.clone()];
    let page_frames: Vec<usize> = page
        .iter()
        .filter_map(|(_, row)| match row {
            Row::Frame(position) => Some(*position),
            Row::Line(_) => None,
        })
        .collect();
    // The list's one Tab stop (A4): the row the reader last focused while it
    // is on the page, then the one the pane reads, then the newest.
    let focused = use_signal(|| None::<u64>);
    let on_page = |ordinal: u64| {
        page_frames
            .iter()
            .any(|position| (dropped + position) as u64 == ordinal)
    };
    let current = focused()
        .filter(|ordinal| on_page(*ordinal))
        .or(selected.filter(|ordinal| on_page(*ordinal)))
        .or_else(|| {
            page_frames
                .last()
                .map(|position| (dropped + position) as u64)
        });
    let waiting = wire.map(|wire| wire.waiting_calls()).unwrap_or_default();

    rsx! {
        div { class: "trace", "data-slot": "trace", "data-merged": "{merged_on}",
            {narrowed(shown, held, "frames", narrowing)}
            {crate::page::controls(&ordinals, range.clone(), anchor, FRAMES)}
            if let Some(saved) = saved {
                crate::export::Notice {
                    key: "{saved.id}",
                    outcome: saved,
                    on_dismiss: move |()| {
                        if let Some(dismiss) = on_dismiss_saved {
                            dismiss.call(());
                        }
                    },
                }
            }
            if let Some(wire) = wire {
                {pending_calls(wire, &waiting, on_choose, anchor)}
            }

            if empty {
                p { class: "empty",
                    "Every JSON-RPC frame, both directions, exactly as it crossed the wire."
                }
            } else {
                div { class: "wire-scroll", "data-tail": FRAMES,
                    "data-page-latest": "{range.end == ordinals.len()}",
                ol { class: "wire-rows",
                    onmounted: move |_| crate::console::rove(
                        &format!(r#"[data-tail="{FRAMES}"]"#),
                        "button.frame-row",
                        None,
                    ),
                    // Oldest first, in the DOM as on screen (A3). The scroller
                    // around the list is laid out bottom-up with the list as its
                    // one child, which keeps a live trace showing its latest
                    // frame without a scroll script — and leaves it where the
                    // reader put it once they scroll back.
                    //
                    // **Keyed by the ordinal the frame was captured under, not
                    // by where it now sits in the list.** The trace is a ring:
                    // once it is full, every frame recorded drops the oldest and
                    // shifts every position by one, and a row keyed by position
                    // would hand whatever the reader had selected to whichever
                    // frame landed there next.
                    for (key, row) in page.iter().copied() {
                        match row {
                            Row::Frame(position) => {
                                let traced = &held_frames[position];
                                let ordinal = (dropped + position) as u64;
                                let pair = pairs.counterpart(ordinal);
                                frame_row(
                                    traced,
                                    Shown {
                                        ordinal,
                                        starts: position > 0 && held_frames[position - 1].connection != traced.connection,
                                        several,
                                        answering: pairs.answering(ordinal, traced),
                                        current: current == Some(ordinal),
                                        took: pair.filter(|_| is_answer(&traced.frame)).map(|pair| pair.elapsed),
                                        mistyped: pair.is_some_and(|pair| pair.mistyped),
                                        unanswered: pairs.unanswered(ordinal, traced),
                                        resend: wire.filter(|wire| !wire.view.is_open() && crate::raw::resendable(traced)),
                                    },
                                    revealed,
                                    decoded,
                                    selected,
                                    focused,
                                    on_choose,
                                    on_seek,
                                )
                            }
                            Row::Line(position) => stderr_row(key, &held_lines[position]),
                        }
                    }
                }
                }
                {crate::tail::to_latest(FRAMES, "frame", Some(anchor))}

                // And the one the reader is reading. Always drawn while there
                // are frames, because a pane that came and went would move the
                // list under the click that selected a row in it.
                {reading(&pairs, &page_frames, selected, on_filter, on_choose, anchor, wire)}
            }
        }
    }
}

/// The calls this client is still waiting on, with how long each has waited
/// and a way to give one up (F1, F4).
fn pending_calls(
    wire: Wire,
    waiting: &[acp_inspector_core::PendingCall],
    on_choose: EventHandler<u64>,
    mut anchor: Signal<Option<u64>>,
) -> Element {
    // Said while the call it was about is still waiting, or where it failed:
    // once the agent answers, the call leaves the list and the answer says
    // the rest.
    let cancelled = (wire.cancelled)().filter(|cancelled| {
        cancelled.outcome.is_err() || waiting.iter().any(|call| call.id == cancelled.id)
    });
    if waiting.is_empty() && cancelled.is_none() {
        return rsx! {};
    }
    let live = wire.live();
    let said = match waiting.len() {
        0 => String::new(),
        1 => "Waiting for 1 response".to_owned(),
        count => format!("Waiting for {count} responses"),
    };
    rsx! {
        section { class: "pending-calls", "data-slot": "pending-calls", aria_label: "Calls waiting for a response",
            if !waiting.is_empty() {
                p { class: "pending-head", "{said}" }
                ul {
                    for call in waiting.iter().cloned() {
                        li { key: "{call.id}", "data-slot": "pending-call",
                            span { class: "frame-method", "{crate::page::prefix(&call.method, 120)}" }
                            span { class: "frame-id", " #{call.id}" }
                            if call.hand_sent {
                                span { class: "frame-tag", "data-slot": "hand-sent", "hand-sent" }
                            }
                            " · "
                            Elapsed { since: SystemTime::now() - call.since.elapsed() }
                            if let Some(ordinal) = call.captured {
                                button {
                                    class: "btn btn-xs btn-quiet",
                                    r#type: "button",
                                    "data-slot": "show-pending",
                                    onclick: move |_| {
                                        anchor.set(None);
                                        on_choose.call(ordinal);
                                    },
                                    "Show"
                                }
                            }
                            if live {
                                button {
                                    class: "btn btn-xs btn-quiet btn-bad",
                                    r#type: "button",
                                    "data-slot": "cancel-call",
                                    title: "Send $/cancel_request for call {call.id}. The call stays pending until the agent answers it.",
                                    onclick: move |_| wire.cancel(call.id),
                                    "Cancel request"
                                }
                            }
                        }
                    }
                }
            }
            if let Some(cancelled) = cancelled {
                {cancel_feedback(&cancelled)}
            }
        }
    }
}

/// What came of asking the agent to give up a call.
fn cancel_feedback(cancelled: &crate::wire::Cancelled) -> Element {
    match &cancelled.outcome {
        Ok(()) => rsx! {
            p { class: "hint", "data-slot": "cancel-sent", role: "status",
                "Sent $/cancel_request for #{cancelled.id}. The call ends when the agent answers it."
            }
        },
        Err(problem) => rsx! {
            p { class: "bad", "data-slot": "cancel-failed", role: "status",
                "Could not cancel #{cancelled.id}: {problem}"
            }
        },
    }
}

/// One line of the agent's stderr, between the frames (F14).
fn stderr_row(key: u64, line: &Diagnostic) -> Element {
    let text = line.to_string();
    let preview = crate::page::prefix(&text, 512).to_owned();
    let cut = preview.len() < text.len();
    rsx! {
        li { key: "line-{key}", class: "stderr-row", "data-slot": "stderr-line",
            div { class: "frame-line",
                span { class: "frame-at", "{stamp(line.at)}" }
                span { class: "frame-tag", "data-slot": "stderr", "stderr" }
                span { class: "stderr-text", "{preview}" }
                if cut {
                    span { class: "hint", " (preview; {text.len()} bytes in Diagnostics)" }
                }
            }
        }
    }
}

/// What a row is drawn with besides its frame.
struct Shown<'a> {
    ordinal: u64,
    /// Whether the frame before it crossed a *different* connection — the
    /// handover from one agent to the next, seen. False for the oldest frame
    /// held, which has no frame before it to have changed from.
    starts: bool,
    /// Whether the trace holds more than one connection's traffic.
    several: bool,
    /// For an answer, the method of the call it answers, where that call is
    /// still known.
    answering: Option<&'a str>,
    /// Whether this row is the list's Tab stop.
    current: bool,
    /// For an answer, how long after its request it crossed.
    took: Option<Duration>,
    /// An answer whose id matches its call's only with its type changed.
    mistyped: bool,
    /// A request nothing in the trace answers.
    unanswered: bool,
    /// Where "Edit and resend" sends this frame, for one this client sent.
    resend: Option<Wire>,
}

/// One frame, as the row that selects it.
#[allow(clippy::too_many_arguments)]
fn frame_row(
    entry: &TracedFrame,
    shown: Shown<'_>,
    revealed: ReadSignal<Vec<u64>>,
    decoded: ReadSignal<HashSet<u64>>,
    selected: Option<u64>,
    mut focused: Signal<Option<u64>>,
    on_choose: EventHandler<u64>,
    on_seek: EventHandler<u64>,
) -> Element {
    let Shown {
        ordinal,
        starts,
        several,
        answering,
        current,
        took,
        mistyped,
        unanswered,
        resend,
    } = shown;
    let way = way(entry.direction);
    let summary = entry.frame.summary();
    let kind = summary.kind;
    let marked = revealed().contains(&ordinal);
    let chosen = selected == Some(ordinal);
    let tabindex = if current { "0" } else { "-1" };
    let omission = entry.frame.omission();
    let copy = entry.frame.clone();

    rsx! {
        li {
            key: "{ordinal}",
            "data-frame": "{ordinal}",
            "data-kind": kind_tone(kind),
            "data-direction": way.tone,
            "data-hand-sent": entry.frame.is_hand_sent().then_some("true"),
            "data-dropped": omission.is_some().then_some("true"),
            // The row where an agent gave way to the next one draws the
            // boundary: the ordinal on every row says whose traffic each is,
            // and this says where the conversation changed.
            "data-switched": "{starts}",
            div { class: if starts { "frame-line switched" } else { "frame-line" },
                button {
                    class: if marked { "frame-row revealed" } else { "frame-row" },
                    // `aria-current`, not `aria-selected`: this is the row the
                    // pane below is reading, which is the one thing a reader can
                    // point at rather than a selection in a listbox.
                    aria_current: chosen.then_some("true"),
                    tabindex,
                    "data-revealed": "{marked}",
                    aria_label: frame_name(entry, answering, several),
                    title: "Read frame {ordinal} in the pane below",
                    onclick: move |_| on_choose.call(ordinal),
                    onfocus: move |_| focused.set(Some(ordinal)),
                    // Which pipe it crossed, where there is more than one to
                    // tell apart — the same ordinal the JSONL export writes.
                    if several {
                        span {
                            class: "frame-crossing",
                            "data-slot": "crossing",
                            title: "Connection {entry.connection}, counting from the first agent this window launched",
                            "#{entry.connection}"
                        }
                    }
                    span { class: "frame-at", "{stamp(entry.at)}" }
                    span { class: "frame-arrow {way.tone}", title: "{way.whence}", {icon(way.role)} }
                    // What the envelope says this frame is, in front of the
                    // bytes and never instead of them (`Frame::summary`).
                    {summarised(&entry.frame, answering)}
                    {tags(&entry.frame, mistyped, unanswered)}
                    // One line of the frame, as the frame has it. The wire's own
                    // line whatever the indentation switch says: a preview is one
                    // line by construction, and the first line of an indented
                    // document is `{`.
                    match omission {
                        Some(omission) => rsx! {
                            span { class: "frame-peek", "data-slot": "omitted", "{omitted(omission)}" }
                        },
                        None => rsx! {
                            span { class: "frame-peek", "{crate::page::prefix(entry.frame.as_str(), 512)}" }
                        },
                    }
                    if let Some(took) = took {
                        span { class: "frame-took", "data-slot": "took", title: "Time since the request", "+{elapsed(took)}" }
                    }
                    span { class: "frame-kind {kind_tone(kind)}", "{named_kind(kind)}" }
                }
                // The row's own controls sit beside it and never inside it, so
                // each has one unambiguous job. A Tab stop only on the list's
                // current row, which is where Tab from the row lands.
                span { class: "controls",
                    if decoded().contains(&ordinal) {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "reach",
                            r#type: "button",
                            tabindex,
                            title: "Show what the timeline made of this frame",
                            aria_label: "Show Timeline entry for Frame {ordinal}",
                            onclick: move |_| on_seek.call(ordinal),
                            "Timeline"
                        }
                    }
                    if let Some(wire) = resend {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "resend",
                            r#type: "button",
                            tabindex,
                            title: "Copy this frame into the raw composer to edit and send again",
                            aria_label: "Edit and resend Frame {ordinal}",
                            onclick: move |_| wire.resend(&copy),
                            "Edit and resend"
                        }
                    }
                }
            }
        }
    }
}

/// The marks a frame carries besides its shape: who wrote it, whether its
/// bytes were UTF-8, whether its id is the right type, whether it is answered.
fn tags(frame: &Frame, mistyped: bool, unanswered: bool) -> Element {
    rsx! {
        if frame.is_hand_sent() {
            span { class: "frame-tag", "data-slot": "hand-sent", title: "Sent by hand from the raw composer", "hand-sent" }
        }
        if frame.is_lossy() {
            span {
                class: "frame-tag bad",
                "data-slot": "lossy",
                title: "These bytes were not valid UTF-8; they are shown decoded, and the export carries the original bytes",
                "not UTF-8"
            }
        }
        if mistyped {
            span { class: "frame-tag bad", "data-slot": "mistyped", title: "The id's JSON type differs from the request's", "wrong id type" }
        }
        if unanswered {
            span { class: "frame-tag", "data-slot": "unanswered", title: "Nothing in the trace answers this request", "no response" }
        }
    }
}

/// The frame the pane is reading: the one that was chosen, or the newest on the page.
///
/// **The newest rather than nothing**, because that is what a reader watching a
/// live trace is looking at anyway — and because a pane that is empty until
/// something is clicked spends two fifths of the Console saying *click
/// something*.
fn reading(
    pairs: &Pairs<'_>,
    visible: &[usize],
    selected: Option<u64>,
    on_filter: Option<EventHandler<String>>,
    on_choose: EventHandler<u64>,
    anchor: Signal<Option<u64>>,
    wire: Option<Wire>,
) -> Element {
    let dropped = pairs.dropped;
    let found = selected
        .and_then(|ordinal| pairs.frame(ordinal).map(|entry| (ordinal, entry)))
        .or_else(|| {
            // The newest frame the reader can *see*: a pane reading one they
            // narrowed away would be the surface answering the chip with the
            // row it just hid.
            let position = visible.last().copied()?;
            Some(((dropped + position) as u64, pairs.frames.get(position)?))
        });

    let Some((ordinal, entry)) = found else {
        return rsx! {};
    };
    let way = way(entry.direction);
    let summary = entry.frame.summary();
    let answering = pairs.answering(ordinal, entry);
    let label = match answering {
        Some(method) => envelope_preview(method).into_owned(),
        None => envelope_preview(summary.label().unwrap_or("unreadable envelope")).into_owned(),
    };
    let on_this_page = |ordinal: u64| {
        visible
            .iter()
            .any(|position| (dropped + position) as u64 == ordinal)
    };
    let outside_page = selected == Some(ordinal) && !on_this_page(ordinal);
    let omission = entry.frame.omission();
    // Laid out, always, and this is the surface that says why (see the module
    // note): one selected document, on the one screen whose subject it is.
    let large = entry.frame.as_str().len() > 16 * 1024;
    let payload = if large || omission.is_some() {
        String::new()
    } else {
        Indentation::Indented
            .draw(entry.frame.as_str())
            .into_owned()
    };
    // The narrowings this frame offers: its method, and its id.
    let method = summary
        .method
        .clone()
        .or(answering.map(str::to_owned))
        .filter(|method| method.len() <= 512);
    let id = summary.id.clone().filter(|id| id.len() <= 512);
    let pairing = pairing(pairs, ordinal, entry, wire);
    let page: Vec<u64> = visible
        .iter()
        .map(|position| (dropped + position) as u64)
        .collect();
    let go: std::rc::Rc<dyn Fn(u64)> = std::rc::Rc::new(move |target: u64| {
        if !page.contains(&target) {
            let mut anchor = anchor;
            anchor.set(Some(target));
        }
        on_choose.call(target);
    });
    let resend = wire
        .filter(|wire| !wire.view.is_open() && crate::raw::resendable(entry))
        .map(|wire| (wire, entry.frame.clone()));

    rsx! {
        aside {
            class: "frame-pane",
            "data-slot": "frame-detail",
            aria_label: "The selected frame",
            header { class: "frame-pane-head",
                span { class: "frame-arrow {way.tone}", title: "{way.whence}", {icon(way.role)} }
                span { class: "frame-method",
                    if answering.is_some() { span { aria_hidden: "true", "↳ " } span { class: "sr-only", "response to " } }
                    "{label}"
                }
                {tags(&entry.frame, false, false)}
                span { class: "frame-pane-meta",
                    "{named_kind(summary.kind)} · #{ordinal} · {stamp(entry.at)}"
                }
                if let Some(narrow) = on_filter {
                    if let Some(method) = method {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "filter-method",
                            r#type: "button",
                            title: "Show only frames for {method}",
                            onclick: move |_| narrow.call(format!("method:{method}")),
                            "Only this method"
                        }
                    }
                    if let Some(id) = id {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "filter-id",
                            r#type: "button",
                            title: "Show only the call with id {id} and its answer",
                            onclick: move |_| narrow.call(format!("id:{id}")),
                            "Only this id"
                        }
                    }
                }
                if let Some((wire, frame)) = resend {
                    button {
                        class: "btn btn-xs btn-quiet",
                        "data-slot": "resend",
                        r#type: "button",
                        title: "Copy this frame into the raw composer to edit and send again",
                        onclick: move |_| wire.resend(&frame),
                        "Edit and resend"
                    }
                }
                if omission.is_none() {
                    Copy { frame: entry.frame.clone(), what: "this frame" }
                }
            }
            {pairing.render(go, wire)}
            if outside_page {
                p { class: "narrowed", "Selected frame (outside this page)" }
            }
            if entry.frame.is_lossy() {
                p { class: "hint", "data-slot": "lossy-note",
                    "These bytes were not valid UTF-8. Invalid bytes are shown as \u{FFFD}; Copy takes the decoded text, and the export carries the original bytes."
                }
            }
            if let Some(omission) = omission {
                p { class: "frame-omitted", "data-slot": "omitted",
                    "{omitted(omission)}. The transport could not carry it; only its length is known."
                }
            } else if large {
                crate::page::RawFrame { key: "{ordinal}", frame: entry.frame.clone() }
            } else { pre { class: "frame-json", "data-slot": "frame-payload",
                for (position, (token, text)) in json::tokens(&payload).into_iter().enumerate() {
                    span { key: "{position}", class: token.class(), "{text}" }
                }
            } }
        }
    }
}

/// What the pane says about the other half of a request/response pair.
enum Pairing {
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

fn pairing(pairs: &Pairs<'_>, ordinal: u64, entry: &TracedFrame, wire: Option<Wire>) -> Pairing {
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

/// "id has the wrong type (string "3" for call 3)".
fn mistyped(answer: &Frame, request: Option<&TracedFrame>) -> String {
    let id = serde_json::from_str::<serde_json::Value>(answer.as_str())
        .ok()
        .and_then(|message| message.get("id").cloned());
    let said = match &id {
        Some(serde_json::Value::String(text)) => {
            format!("string {}", serde_json::Value::from(text.as_str()))
        }
        Some(other) => format!("number {other}"),
        None => "a different type".to_owned(),
    };
    let call = request
        .and_then(|request| request.frame.summary().id.clone())
        .unwrap_or_else(|| "?".to_owned());
    format!("id has the wrong type ({said} for call {call})")
}

impl Pairing {
    fn render(self, go: std::rc::Rc<dyn Fn(u64)>, wire: Option<Wire>) -> Element {
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

/// What a frame says it is, drawn in front of what it says.
///
/// Core reads the envelope ([`Frame::summary`]); this decides what a row does
/// with the answer, which is one word and, where there is one, the id under
/// which its answer will come back. An answer whose call is still held says
/// which call it answers, `↳ initialize`, rather than the word `result` (D11).
/// Nothing is drawn for a frame the envelope could not be read out of — the row
/// is the bytes then, which is what every row was before this existed.
fn summarised(frame: &Frame, answering: Option<&str>) -> Element {
    let summary = frame.summary();
    let Some(label) = answering.or(summary.label()) else {
        return rsx! {};
    };
    let label = envelope_preview(label);
    let id = summary.id.as_deref().map(envelope_preview);

    rsx! {
        span { class: "frame-method", "data-slot": "envelope",
            if answering.is_some() {
                span { class: "frame-answers", aria_hidden: "true", "↳ " }
            }
            "{label}"
        }
        if let Some(id) = id {
            span { class: "frame-id", title: "JSON-RPC id {id}", "#{id}" }
        }
    }
}

/// Labels are previews too, including their tooltips. The raw byte reader and
/// Copy/export retain the complete envelope fields.
fn envelope_preview(text: &str) -> std::borrow::Cow<'_, str> {
    let prefix = crate::page::prefix(text, 512);
    if prefix.len() == text.len() {
        prefix.into()
    } else {
        format!("{prefix}… (preview)").into()
    }
}

/// An envelope field as part of a name read aloud: shorter than a preview,
/// because a row's accessible name is heard whole on every visit.
fn spoken(text: &str) -> std::borrow::Cow<'_, str> {
    let prefix = crate::page::prefix(text, 80);
    if prefix.len() == text.len() {
        prefix.into()
    } else {
        format!("{prefix}…").into()
    }
}

/// What the Trace filter box asked for (F8): `method:NAME` and `id:N` match
/// those envelope fields, and every other word is text the frame must contain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Query {
    method: Option<String>,
    id: Option<String>,
    /// The free text, lowercased once.
    text: String,
}

impl Query {
    fn parse(filter: &str) -> Self {
        let mut query = Self::default();
        let mut text = Vec::new();
        for word in filter.split_whitespace() {
            if let Some(method) = word.strip_prefix("method:").filter(|rest| !rest.is_empty()) {
                query.method = Some(method.to_lowercase());
            } else if let Some(id) = word.strip_prefix("id:").filter(|rest| !rest.is_empty()) {
                query.id = Some(id.trim_matches('"').to_owned());
            } else {
                text.push(word);
            }
        }
        query.text = text.join(" ").to_lowercase();
        query
    }
}

/// What the Trace remembers between renders (L1): which frames matched the
/// text of the current filter.
#[derive(Default)]
struct Index {
    /// The filter text the matches below are for.
    text: String,
    matches: std::collections::HashMap<u64, bool>,
}

impl Index {
    /// Forgets what the filter matched when the filter changed or frames aged
    /// out.
    fn update(&mut self, dropped: usize, query: &Query) {
        let first = dropped as u64;
        self.matches.retain(|ordinal, _| *ordinal >= first);
        if self.text != query.text {
            self.text.clone_from(&query.text);
            self.matches.clear();
        }
    }

    /// Whether a frame survives the query, remembering the text match.
    fn matches(
        &mut self,
        (ordinal, traced): (u64, &TracedFrame),
        query: &Query,
        pairs: &Pairs<'_>,
    ) -> bool {
        let summary = traced.frame.summary();
        if let Some(method) = &query.method {
            let named = summary
                .method
                .as_deref()
                .or_else(|| pairs.answering(ordinal, traced))
                .unwrap_or_default();
            if !contains_folded(named, method) {
                return false;
            }
        }
        if let Some(id) = &query.id
            && summary.id.as_deref().map(|own| own.trim_matches('"')) != Some(id.as_str())
        {
            return false;
        }
        if query.text.is_empty() {
            return true;
        }
        *self
            .matches
            .entry(ordinal)
            .or_insert_with(|| contains_folded(traced.frame.as_str(), &query.text))
    }
}

/// How a direction reads: the inspector is always one end of it (`CONTEXT.md`,
/// *Connection*), so both of them are its own.
struct Way {
    role: Role,
    whence: &'static str,
    tone: &'static str,
}

fn way(direction: Direction) -> Way {
    match direction {
        Direction::ToAgent => Way {
            role: Role::Outgoing,
            whence: "sent to the agent",
            tone: "out",
        },
        Direction::FromAgent => Way {
            role: Role::Incoming,
            whence: "received from the agent",
            tone: "in",
        },
    }
}

/// A row's accessible name (A1): what the frame is — method, kind and id —
/// which way it went, and when; the connection only where there are several.
/// "initialize request #1, sent to the agent, 21:20:24.000".
fn frame_name(frame: &TracedFrame, answering: Option<&str>, several: bool) -> String {
    let summary = frame.frame.summary();
    let kind = named_kind(summary.kind);
    let what = match (summary.method.as_deref(), answering) {
        (Some(method), _) => format!("{} {kind}", spoken(method)),
        (None, Some(call)) => format!("{kind} to {}", spoken(call)),
        (None, None) if frame.frame.omission().is_some() => "dropped frame".to_owned(),
        (None, None) if summary.kind == FrameKind::Unreadable => "unreadable frame".to_owned(),
        (None, None) => kind.to_owned(),
    };
    let id = summary
        .id
        .as_deref()
        .map(|id| format!(" #{}", spoken(id)))
        .unwrap_or_default();
    let connection = if several {
        format!(", connection {}", frame.connection)
    } else {
        String::new()
    };
    let hand = if frame.frame.is_hand_sent() {
        ", sent by hand"
    } else {
        ""
    };
    format!(
        "{what}{id}, {}{hand}{connection}, {}",
        way(frame.direction).whence,
        stamp(frame.at),
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    #[test]
    fn older_page_reads_its_newest_frame_until_a_frame_is_explicitly_selected() {
        for selected in [None, Some(999)] {
            let mut dom = mounted(Screen {
                frames: vec![crossed(1, Direction::FromAgent, "{}"); 1000],
                selected,
                ..Screen::default()
            });
            let edits = dom.rebuild_to_vec().edits;
            let older = crate::page::tests::labelled(&edits, "Older evidence page");
            crate::page::tests::click(&mut dom, older);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains("601–800 of 1000 retained rows"));
            assert!(html.contains(r#"data-frame="799""#));
            assert!(!html.contains(r#"data-frame="999""#));
            let detail = pane(&html).unwrap();
            if selected.is_some() {
                assert!(detail.contains("· #999 ·"));
                assert!(detail.contains("Selected frame (outside this page)"));
            } else {
                assert!(detail.contains("· #799 ·"));
            }
        }
    }

    #[test]
    fn newest_frame_returns_from_an_older_page() {
        let mut dom = mounted(Screen {
            frames: vec![crossed(1, Direction::FromAgent, "{}"); 1000],
            ..Screen::default()
        });
        let edits = dom.rebuild_to_vec().edits;
        let older = crate::page::tests::labelled(&edits, "Older evidence page");
        for label in ["Newest frame", "Latest frames page"] {
            let newest = crate::page::tests::labelled(&edits, label);
            crate::page::tests::click(&mut dom, older);
            assert!(dioxus_ssr::render(&dom).contains("601–800 of 1000 retained rows"));
            crate::page::tests::click(&mut dom, newest);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains("801–1000 of 1000 retained rows"));
            assert!(pane(&html).unwrap().contains("· #999 ·"));
        }
    }

    #[test]
    fn retained_rows_are_paged_and_reveal_reaches_an_older_ordinal() {
        let traffic = vec![crossed(1, Direction::FromAgent, "{}"); 1000];
        let newest = shown(Screen {
            frames: traffic.clone(),
            ..Screen::default()
        });
        assert_eq!(rows_of(&newest).len(), crate::page::SIZE);
        assert!(newest.contains("801–1000 of 1000 retained rows"));
        let older = shown(Screen {
            frames: traffic,
            revealed: vec![20],
            ..Screen::default()
        });
        assert_eq!(rows_of(&older).len(), crate::page::SIZE);
        assert!(older.contains(r#"data-frame="20""#));
        assert!(older.contains(r#"data-revealed="true""#));
    }

    #[test]
    fn multi_megabyte_envelope_fields_are_bounded_in_rows_and_detail() {
        let method = format!(
            "{}🦀{}METHOD_END",
            "m".repeat(511),
            "m".repeat(2 * 1024 * 1024)
        );
        let id = format!("{}é{}ID_END", "i".repeat(511), "i".repeat(2 * 1024 * 1024));
        let raw = serde_json::json!({"method": method, "id": id}).to_string();
        let html = shown(Screen {
            frames: vec![crossed(1, Direction::FromAgent, &raw)],
            ..Screen::default()
        });
        assert!(
            rows_of(&html)[0].len() < 4000,
            "row labels and titles must be bounded"
        );
        assert!(
            pane(&html).unwrap().len() < 22000,
            "detail header must be bounded too"
        );
        assert!(!html.contains("METHOD_END") && !html.contains("ID_END"));
        assert!(
            html.contains("preview")
                && html.contains("Next bytes")
                && html.contains("Copy this frame")
        );
    }

    #[test]
    fn large_raw_preview_is_bounded_without_rewriting_the_frame() {
        let raw = format!(
            r#" {{"method":"large","params":"{}🦀end"}} "#,
            "x".repeat(100_000)
        );
        let frame = Frame::new(raw.clone());
        let html = shown(Screen {
            frames: vec![crossed(1, Direction::FromAgent, &raw)],
            ..Screen::default()
        });
        assert!(html.len() < 25_000);
        assert!(html.contains("Next bytes"));
        assert!(html.contains("literal raw UTF-8"));
        assert!(html.contains("Copy this frame"));
        assert_eq!(frame.as_str(), raw);
        assert!(rows_of(&html)[0].len() < 2000);
    }

    #[test]
    #[ignore]
    fn rendering_probe() {
        let frame = crossed(
            1,
            Direction::FromAgent,
            &format!(r#"{{"method":"probe","params":"{}"}}"#, "x".repeat(512)),
        );
        let start = std::time::Instant::now();
        let html = shown(Screen {
            frames: vec![frame; 10_000],
            ..Screen::default()
        });
        eprintln!(
            "10k Trace SSR: {:?}; HTML bytes={}; rows={}",
            start.elapsed(),
            html.len(),
            rows_of(&html).len()
        );
    }

    fn at(second: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(second)
    }

    /// One frame, as a test describes it: which connection it crossed, which
    /// way, and what it said. Not a [`TracedFrame`], because the props of a
    /// rendered screen have to be comparable and a frame is not.
    #[derive(Clone, PartialEq)]
    struct Crossed {
        connection: u64,
        direction: Direction,
        json: String,
        /// Milliseconds after the connection's second it crossed at.
        later: u64,
    }

    fn crossed(connection: u64, direction: Direction, json: &str) -> Crossed {
        Crossed {
            connection,
            direction,
            json: json.to_owned(),
            later: 0,
        }
    }

    impl Crossed {
        /// The same frame, crossing `millis` later.
        fn later(mut self, millis: u64) -> Self {
            self.later = millis;
            self
        }

        /// `HAND:` marks a frame sent by hand, `LOSSY:` one whose bytes end in
        /// one that is not UTF-8, and `DROPPED:N` a placeholder for N bytes.
        fn traced(&self) -> TracedFrame {
            let frame = if let Some(text) = self.json.strip_prefix("HAND:") {
                Frame::by_hand(text)
            } else if let Some(text) = self.json.strip_prefix("LOSSY:") {
                let mut bytes = text.as_bytes().to_vec();
                bytes.push(0xff);
                Frame::read(bytes)
            } else if let Some(length) = self.json.strip_prefix("DROPPED:") {
                Frame::omitted(
                    b"{",
                    Omission {
                        length: length.parse().unwrap(),
                        limit: 10 * 1024 * 1024,
                    },
                )
            } else {
                Frame::new(self.json.as_str())
            };
            TracedFrame {
                at: at(self.connection) + Duration::from_millis(self.later),
                connection: self.connection,
                direction: self.direction,
                frame,
            }
        }
    }

    /// The default trace this module's tests read: one call out, its answer
    /// back, and a notification after them.
    fn traffic() -> Vec<Crossed> {
        vec![
            crossed(
                1,
                Direction::ToAgent,
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            ),
            crossed(
                1,
                Direction::FromAgent,
                r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#,
            ),
            crossed(
                1,
                Direction::FromAgent,
                r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-1"}}"#,
            ),
        ]
    }

    /// What the surface is being asked to show. The screen itself and never a
    /// value on the way to one: what a row says is the question here.
    #[derive(Clone, PartialEq)]
    struct Screen {
        frames: Vec<Crossed>,
        dropped: usize,
        filter: String,
        kinds: Kinds,
        directions: Directions,
        revealed: Vec<u64>,
        decoded: Vec<u64>,
        selected: Option<u64>,
        pending_only: bool,
        /// Calls the client is still waiting on: id, and the ordinal it went
        /// out under.
        waiting: Vec<(i64, u64)>,
        connected: bool,
    }

    impl Default for Screen {
        fn default() -> Self {
            Self {
                frames: traffic(),
                dropped: 0,
                filter: String::new(),
                kinds: Kinds::default(),
                directions: Directions::default(),
                revealed: Vec::new(),
                decoded: Vec::new(),
                selected: None,
                pending_only: false,
                waiting: Vec::new(),
                connected: false,
            }
        }
    }

    fn mounted(screen: Screen) -> VirtualDom {
        #[component]
        fn Host(screen: Screen) -> Element {
            let frames = use_signal(|| {
                screen
                    .frames
                    .iter()
                    .map(Crossed::traced)
                    .collect::<Vec<_>>()
            });
            let revealed = use_signal(|| screen.revealed.clone());
            let decoded = use_signal(|| screen.decoded.iter().copied().collect::<HashSet<u64>>());
            let wire = use_hook(|| {
                let mut wire =
                    Wire::detached(Trace::from_records(frames.peek().clone(), screen.dropped));
                wire.pending_only.set(screen.pending_only);
                if screen.connected {
                    wire.status
                        .set(acp_inspector_core::ConnectionStatus::Connected);
                }
                wire.pending.set(
                    screen
                        .waiting
                        .iter()
                        .map(|&(id, ordinal)| acp_inspector_core::PendingCall {
                            id,
                            method: "session/prompt".to_owned(),
                            since: std::time::Instant::now(),
                            captured: Some(ordinal),
                            hand_sent: true,
                        })
                        .collect(),
                );
                wire
            });
            rsx! {
                TraceView {
                    frames,
                    dropped: screen.dropped,
                    saved: None,
                    wire,
                    filter: screen.filter.clone(),
                    kinds: screen.kinds,
                    directions: screen.directions,
                    revealed,
                    decoded,
                    selected: screen.selected,
                    on_choose: move |_: u64| {},
                    on_seek: move |_: u64| {},
                }
            }
        }

        VirtualDom::new_with_props(Host, HostProps { screen })
    }

    fn shown(screen: Screen) -> String {
        let mut dom = mounted(screen);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// Every row, in the order the list draws them — newest first in the DOM,
    /// because the list is laid out bottom-up.
    fn rows_of(html: &str) -> Vec<String> {
        html.split("<li")
            .skip(1)
            .filter_map(|row| row.split_once("</li>"))
            .map(|(row, _)| row.to_owned())
            .collect()
    }

    /// The pane at the foot of the list, if there is one.
    fn pane(html: &str) -> Option<String> {
        html.split_once(r#"data-slot="frame-detail""#)
            .map(|(_, rest)| rest.to_owned())
    }

    #[test]
    fn every_row_says_which_connection_it_crossed_and_which_ordinal_it_is() {
        // The trace outlives any one agent, so one trace holds two agents'
        // traffic while both agents' JSON-RPC ids start again at 1. The ordinal
        // is the same one the JSONL export writes, so a row and a record of the
        // same frame agree — and the same number is what the Timeline points at.
        let html = shown(Screen {
            dropped: 4,
            ..Screen::default()
        });
        for ordinal in [4, 5, 6] {
            assert!(
                html.contains(&format!(r#"data-frame="{ordinal}""#)),
                "the row is numbered from what the trace has let go: {html}"
            );
        }
        // One agent's traffic has nothing to tell its rows apart by (D11).
        assert!(
            !html.contains(r#"data-slot="crossing""#),
            "a single connection draws no ordinal: {html}"
        );

        let mut two = traffic();
        two.push(crossed(
            2,
            Direction::ToAgent,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        ));
        let html = shown(Screen {
            frames: two,
            ..Screen::default()
        });
        assert!(
            html.contains(r#"data-slot="crossing""#)
                && html.contains("#1")
                && html.contains("#2")
                && html.contains(", connection 2,"),
            "and with several, every row says whose traffic it is: {html}"
        );
    }

    #[test]
    fn a_row_is_named_by_what_it_is_and_an_answer_by_its_call() {
        // A1: method, kind and id lead the accessible name; D11: an answer
        // says which call it answers.
        let html = shown(Screen::default());
        assert!(
            html.contains(r#"aria-label="initialize request #1, sent to the agent, "#),
            "{html}"
        );
        assert!(
            html.contains(r#"aria-label="response to initialize #1, received from the agent, "#),
            "{html}"
        );
        assert!(
            html.contains(r#"aria-label="session/update notif, received from the agent, "#),
            "{html}"
        );
        let answer = rows_of(&html)
            .into_iter()
            .find(|row| row.contains(r#"data-kind="result""#))
            .expect("the answer row");
        assert!(
            answer.contains("↳ </span>initialize</span>") && !answer.contains(">result<"),
            "{answer}"
        );
        assert!(
            !html.contains(", connection 1,"),
            "one connection is not named: {html}"
        );
    }

    #[test]
    fn the_filter_matches_method_and_id_fields_and_directions_narrow_too() {
        let by_method = shown(Screen {
            filter: "method:initialize".to_owned(),
            ..Screen::default()
        });
        assert!(
            by_method.contains("2 of 3 frames"),
            "the call and its answer: {by_method}"
        );
        let by_id = shown(Screen {
            filter: "id:1".to_owned(),
            ..Screen::default()
        });
        assert!(by_id.contains("2 of 3 frames"), "{by_id}");
        let received = shown(Screen {
            directions: Directions::default().toggled(Direction::ToAgent),
            ..Screen::default()
        });
        assert!(
            received.contains("2 of 3 frames") && !received.contains(r#"data-direction="out""#),
            "{received}"
        );
        assert!(
            Query::parse("method:Session/Prompt hello")
                .method
                .as_deref()
                == Some("session/prompt")
        );
    }

    #[test]
    fn the_list_is_one_tab_stop() {
        let html = shown(Screen::default());
        assert_eq!(
            html.matches(r#"tabindex="0""#).count(),
            1,
            "one row is the list's Tab stop: {html}"
        );
        // Two rows that are not the stop, and the sent call's own control.
        assert_eq!(html.matches(r#"tabindex="-1""#).count(), 3, "{html}");
    }

    #[test]
    fn an_answer_says_how_long_it_took_and_the_pane_points_at_the_other_half() {
        let frames = vec![
            crossed(
                1,
                Direction::ToAgent,
                r#"{"jsonrpc":"2.0","id":1,"method":"session/list"}"#,
            ),
            crossed(
                1,
                Direction::FromAgent,
                r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            )
            .later(412),
        ];
        let html = shown(Screen {
            frames: frames.clone(),
            selected: Some(0),
            ..Screen::default()
        });
        let answer = rows_of(&html)
            .into_iter()
            .find(|row| row.contains(r#"data-frame="1""#))
            .expect("the answer row");
        assert!(
            answer.contains(r#"data-slot="took""#) && answer.contains("+412 ms"),
            "{answer}"
        );
        let request = pane(&html).unwrap();
        assert!(
            request.contains("Response #1 · +412 ms")
                && request.contains(r#"data-slot="go-response""#),
            "{request}"
        );
        let answered = pane(&shown(Screen {
            frames,
            selected: Some(1),
            ..Screen::default()
        }))
        .unwrap();
        assert!(
            answered.contains("Request #0 · +412 ms")
                && answered.contains(r#"data-slot="go-request""#),
            "{answered}"
        );
    }

    #[test]
    fn a_request_nobody_answered_says_so_and_the_pending_chip_keeps_only_those() {
        let frames = vec![
            crossed(
                1,
                Direction::ToAgent,
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            ),
            crossed(
                1,
                Direction::FromAgent,
                r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            ),
            crossed(
                1,
                Direction::ToAgent,
                r#"{"jsonrpc":"2.0","id":2,"method":"session/prompt"}"#,
            ),
            crossed(
                1,
                Direction::FromAgent,
                r#"{"jsonrpc":"2.0","method":"session/update"}"#,
            ),
        ];
        let all = shown(Screen {
            frames: frames.clone(),
            selected: Some(2),
            ..Screen::default()
        });
        assert_eq!(all.matches(r#"data-slot="unanswered""#).count(), 1, "{all}");
        assert!(
            pane(&all).unwrap().contains("No response in the trace"),
            "{all}"
        );
        let pending = shown(Screen {
            frames,
            pending_only: true,
            ..Screen::default()
        });
        assert!(pending.contains("1 of 4 frames"), "{pending}");
        assert_eq!(rows_of(&pending).len(), 1);
        assert!(rows_of(&pending)[0].contains("session/prompt"));
    }

    #[test]
    fn a_call_still_waiting_says_for_how_long_and_is_listed() {
        let html = shown(Screen {
            frames: vec![crossed(
                1,
                Direction::ToAgent,
                r#"HAND:{"jsonrpc":"2.0","id":2,"method":"session/prompt"}"#,
            )],
            selected: Some(0),
            waiting: vec![(2, 0)],
            ..Screen::default()
        });
        let detail = pane(&html).unwrap();
        assert!(
            detail.contains("No response yet · ") && detail.contains(r#"data-slot="elapsed""#),
            "{detail}"
        );
        assert!(
            html.contains(r#"data-slot="pending-calls""#)
                && html.contains("Waiting for 1 response"),
            "{html}"
        );
        // Not connected, so nothing to cancel with.
        assert!(!html.contains(r#"data-slot="cancel-call""#), "{html}");

        // Connected: the pane and the list both offer to give it up.
        let live = shown(Screen {
            frames: vec![crossed(
                1,
                Direction::ToAgent,
                r#"{"jsonrpc":"2.0","id":2,"method":"session/prompt"}"#,
            )],
            selected: Some(0),
            waiting: vec![(2, 0)],
            connected: true,
            ..Screen::default()
        });
        assert_eq!(
            live.matches(r#"data-slot="cancel-call""#).count(),
            2,
            "{live}"
        );
        assert!(live.contains(">Cancel request<"), "{live}");
        assert!(
            live.contains(r#"data-slot="resend""#),
            "a sent call can be edited and sent again: {live}"
        );
    }

    #[test]
    fn an_answer_with_the_wrong_id_type_is_annotated() {
        let html = shown(Screen {
            frames: vec![
                crossed(
                    1,
                    Direction::ToAgent,
                    r#"{"jsonrpc":"2.0","id":3,"method":"session/list"}"#,
                ),
                crossed(
                    1,
                    Direction::FromAgent,
                    r#"{"jsonrpc":"2.0","id":"3","result":{}}"#,
                ),
            ],
            selected: Some(1),
            ..Screen::default()
        });
        assert!(html.contains(r#"data-slot="mistyped""#), "{html}");
        let detail = pane(&html).unwrap();
        assert!(
            detail.contains("id has the wrong type (string ")
                && detail.contains("3")
                && detail.contains(" for call 3)"),
            "{detail}"
        );
    }

    #[test]
    fn lossy_dropped_and_hand_sent_frames_say_what_they_are() {
        let html = shown(Screen {
            frames: vec![
                crossed(
                    1,
                    Direction::ToAgent,
                    r#"HAND:{"jsonrpc":"2.0","id":9,"method":"x/poke"}"#,
                ),
                crossed(
                    1,
                    Direction::FromAgent,
                    r#"LOSSY:{"jsonrpc":"2.0","method":"x/bytes","params":"#,
                ),
                crossed(1, Direction::FromAgent, "DROPPED:20971520"),
            ],
            selected: Some(2),
            ..Screen::default()
        });
        assert!(
            html.contains(r#"data-slot="hand-sent""#) && html.contains(">hand-sent<"),
            "{html}"
        );
        assert!(
            html.contains(r#"data-slot="lossy""#) && html.contains(">not UTF-8<"),
            "{html}"
        );
        assert!(
            html.contains("Frame of 20.0 MiB dropped (limit 10.0 MiB)"),
            "a placeholder says how long the frame was: {html}"
        );
        assert!(
            html.contains(r#"aria-label="dropped frame, received from the agent"#),
            "{html}"
        );
        assert!(html.contains(", sent by hand, "), "{html}");
        assert!(
            !pane(&html).unwrap().contains(r#"data-slot="copy""#),
            "there is nothing of a dropped frame to copy"
        );
    }

    #[test]
    fn merged_rows_interleave_stderr_by_read_time_with_keys_that_survive_ageing() {
        let frame = |second: u64| TracedFrame {
            at: at(second),
            connection: 1,
            direction: Direction::FromAgent,
            frame: Frame::new("{}"),
        };
        let line = |second: u64, kind: DiagnosticKind| Diagnostic {
            at: at(second),
            kind,
        };
        let stderr = |text: &str| DiagnosticKind::Stderr(text.to_owned());
        let frames = vec![frame(1), frame(3), frame(5)];
        let lines = vec![
            line(2, stderr("a")),
            line(2, DiagnosticKind::StderrLineLossy),
            line(4, stderr("b")),
            line(6, stderr("c")),
        ];
        let merged = merge(&frames, 0, &lines, 0);
        assert_eq!(
            merged.iter().map(|(_, row)| *row).collect::<Vec<_>>(),
            vec![
                Row::Frame(0),
                Row::Line(0),
                Row::Frame(1),
                Row::Line(2),
                Row::Frame(2),
                Row::Line(3),
            ],
            "each line between the frames read either side of it; transport remarks left out"
        );
        assert!(
            merged.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "{merged:?}"
        );
        // The oldest frame and line age out: what is left keeps its keys.
        let aged = merge(&frames[1..], 1, &lines[1..], 1);
        let keys = |rows: &[(u64, Row)]| rows.iter().map(|(key, _)| *key).collect::<Vec<_>>();
        assert_eq!(keys(&merged[2..]), keys(&aged), "{aged:?}");
        // A frame and a line read at the same moment: the frame first.
        let tied = merge(&[frame(2)], 0, &[line(2, stderr("x"))], 0);
        assert_eq!(tied[0].1, Row::Frame(0));
    }

    #[test]
    fn the_merged_view_draws_stderr_lines_among_the_frames() {
        fn host() -> Element {
            let frames = use_signal(|| {
                traffic()
                    .iter()
                    .enumerate()
                    .map(|(nth, crossed)| crossed.clone().later(nth as u64 * 100).traced())
                    .collect::<Vec<_>>()
            });
            let lines = use_signal(|| {
                vec![Diagnostic {
                    at: at(1) + Duration::from_millis(50),
                    kind: DiagnosticKind::Stderr("agent says hello".to_owned()),
                }]
            });
            let revealed = use_signal(Vec::<u64>::new);
            let decoded = use_signal(HashSet::<u64>::new);
            let wire = use_hook(|| {
                let mut wire = Wire::detached(Trace::from_records(frames.peek().clone(), 0));
                wire.merged.set(true);
                wire
            });
            rsx! {
                TraceView {
                    frames, dropped: 0, saved: None, filter: String::new(), kinds: Kinds::default(),
                    revealed, decoded, selected: None, on_choose: move |_: u64| {}, on_seek: move |_: u64| {},
                    wire, lines: Some(lines.into()),
                }
            }
        }
        let mut dom = VirtualDom::new(host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        let rows = rows_of(&html);
        assert_eq!(rows.len(), 4, "{html}");
        // Oldest first in the DOM (A3): the line read between the first two
        // frames is second from the top.
        assert!(
            rows[1].contains(r#"data-slot="stderr-line""#) && rows[1].contains("agent says hello"),
            "{rows:?}"
        );
        assert!(rows[0].contains(r#"data-frame="0""#), "{rows:?}");
    }

    #[test]
    fn where_one_connections_traffic_ends_and_the_nexts_begins_is_marked() {
        let html = shown(Screen {
            frames: vec![
                crossed(
                    1,
                    Direction::ToAgent,
                    r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
                ),
                crossed(
                    2,
                    Direction::ToAgent,
                    r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
                ),
            ],
            ..Screen::default()
        });
        assert_eq!(
            html.matches(r#"data-switched="true""#).count(),
            1,
            "one boundary, on the row where the conversation changed: {html}"
        );
        // Never on the oldest frame the trace still holds: it has no frame
        // before it to have changed from, and marking it would be the screen
        // claiming a handover it cannot have watched.
        let first = rows_of(&html).first().cloned().unwrap_or_default();
        assert!(
            first.contains(r#"data-switched="false""#),
            "the top of the list is where the record begins: {first}"
        );
    }

    #[test]
    fn a_row_says_what_the_envelope_is_without_standing_in_for_the_bytes() {
        let html = shown(Screen::default());
        // The method a reader scans for, the id that makes an answer findable
        // from the call, and the shape the envelope is in — in front of the
        // bytes and never instead of them.
        assert!(
            html.contains(r#"data-slot="envelope""#)
                && html.contains(">initialize</span>")
                && html.contains(">#1</span>")
                && html.contains(r#"class="frame-kind call""#)
                && html.contains(r#"class="frame-kind result""#)
                && html.contains(r#"class="frame-kind notification""#),
            "the envelope's own vocabulary leads the row: {html}"
        );
        assert!(
            html.contains(r#"class="frame-peek""#) && html.contains("params"),
            "and the frame's own first line is still on it: {html}"
        );
    }

    #[test]
    fn a_frame_the_envelope_cannot_be_read_out_of_is_drawn_as_it_always_was() {
        let html = shown(Screen {
            frames: vec![crossed(1, Direction::FromAgent, "not json at all")],
            ..Screen::default()
        });
        assert!(
            !html.contains(r#"data-slot="envelope""#),
            "nothing is put in front of a frame in none of the four shapes: {html}"
        );
        assert!(
            html.contains("not json at all") && html.contains(r#"class="frame-kind unreadable""#),
            "the bytes are the row, and the shape says it could not be read: {html}"
        );
    }

    #[test]
    fn the_pane_reads_the_frame_a_reader_chose_and_the_newest_until_they_do() {
        // An empty pane under a live trace is a region asking to be told
        // something it can already see.
        let newest = pane(&shown(Screen::default())).expect("the pane is drawn");
        assert!(
            newest.contains("session/update"),
            "with nothing chosen the pane reads the newest frame: {newest}"
        );
        let chosen = pane(&shown(Screen {
            selected: Some(0),
            ..Screen::default()
        }))
        .expect("the pane is drawn");
        assert!(
            chosen.contains("initialize") && !chosen.contains("session/update"),
            "and the one the reader pointed at once they have: {chosen}"
        );
        // The row being read says so, and says it as the current one rather
        // than as a selection in a listbox.
        assert!(
            shown(Screen {
                selected: Some(0),
                ..Screen::default()
            })
            .contains(r#"aria-current="true""#),
        );
    }

    #[test]
    fn the_pane_lays_the_payload_out_and_the_row_keeps_the_wires_own_line() {
        let html = shown(Screen {
            selected: Some(0),
            ..Screen::default()
        });
        let pane = pane(&html).expect("the pane is drawn");
        // One document, selected, on the surface whose whole subject it is
        // (`crate::indent`'s third exception) — and coloured by what it is made
        // of, which is the only mark the pane adds.
        assert!(
            pane.contains(r#"data-slot="frame-payload""#)
                && pane.contains("\n  ")
                && pane.contains(r#"class="json-key""#)
                && pane.contains(r#"class="json-string""#),
            "the pane draws the payload laid out: {pane}"
        );
        // And every byte of it: what leaves the window is the wire's, and so is
        // what a reader compares against it.
        assert!(
            pane.contains(r#"aria-label="Copy this frame""#),
            "the copy beside it takes the bytes that crossed: {pane}"
        );
        // The row's own preview is one line by construction: the first line of
        // an indented document is `{`, and a column of them says nothing about
        // a thousand frames.
        let row = rows_of(&html).last().cloned().unwrap_or_default();
        assert!(
            row.contains(r#"class="frame-peek""#) && !row.contains("\n  "),
            "the row is the wire's own line: {row}"
        );
    }

    #[test]
    fn a_chip_hides_the_shape_it_names_and_the_surface_says_what_went() {
        let html = shown(Screen {
            kinds: Kinds::default().toggled(FrameKind::Notification),
            ..Screen::default()
        });
        assert!(
            !html.contains("session/update"),
            "the shape the reader turned off is not drawn: {html}"
        );
        assert!(
            html.contains(r#"data-slot="narrowed""#) && html.contains("2 of 3 frames"),
            "and the surface says what it is hiding, whether it was typed or pressed: {html}"
        );
        // The whole surface is still captured, exported and counted.
        assert!(html.contains("still captured, exported and counted"));
    }

    #[test]
    fn what_the_reader_types_narrows_the_rows_and_nothing_else() {
        let html = shown(Screen {
            filter: "initialize".to_owned(),
            ..Screen::default()
        });
        // Matched against the bytes the reader is looking at rather than
        // against anything decoded: the answer to a call does not carry the
        // call's method, and this surface does not correlate them.
        assert!(
            html.contains("1 of 3 frames") && html.contains("initialize"),
            "the filter reads the frame as the wire had it: {html}"
        );
        let nothing = shown(Screen {
            filter: "nothing crossed with this in it".to_owned(),
            ..Screen::default()
        });
        assert!(
            nothing.contains("No frames match"),
            "a filter that matched nothing says so rather than looking like a quiet agent: {nothing}"
        );
    }

    #[test]
    fn the_rows_a_reader_asked_for_are_marked_and_the_rest_are_not() {
        let html = shown(Screen {
            revealed: vec![1],
            ..Screen::default()
        });
        assert_eq!(
            html.matches(r#"data-revealed="true""#).count(),
            1,
            "what a reader asked to see is marked and the traffic around it is not: {html}"
        );
        // Marked, never filtered away: the rows either side are still drawn.
        assert_eq!(rows_of(&html).len(), 3);
    }

    #[test]
    fn only_a_frame_that_became_an_entry_offers_the_way_back_to_it() {
        let html = shown(Screen {
            decoded: vec![2],
            ..Screen::default()
        });
        assert_eq!(
            html.matches(r#"data-slot="reach""#).count(),
            1,
            "an answer to a call this window made is no less real for having no entry: {html}"
        );
    }

    #[test]
    fn the_frame_that_is_drawn_whole_is_the_one_that_can_be_taken_away() {
        // The density rule read as an affordance, and read exactly: wherever
        // *the bytes* are drawn, they can be copied. A row is not that — it is
        // the head of a payload with the rest cut off — so a copy on it would
        // either hand over bytes the reader cannot see or hand over the
        // ellipsis. The row selects, the pane reads it whole, and the copy is
        // where the whole of it is.
        let html = shown(Screen::default());
        assert_eq!(
            html.matches(r#"data-slot="copy""#).count(),
            1,
            "the pane reading the selected frame is where a frame is taken from: {html}"
        );
        let rows = html
            .split_once(r#"data-slot="frame-detail""#)
            .expect("the pane")
            .0;
        assert!(
            !rows.contains(r#"data-slot="copy""#),
            "and no row carries one: {rows}"
        );
    }

    #[test]
    fn an_empty_trace_says_what_will_be_in_it() {
        let html = shown(Screen {
            frames: Vec::new(),
            ..Screen::default()
        });
        assert!(
            html.contains("Every JSON-RPC frame, both directions, exactly as it crossed the wire."),
            "{html}"
        );
        assert!(
            pane(&html).is_none(),
            "and there is no pane, because there is nothing to read in it: {html}"
        );
    }

    #[test]
    fn the_shapes_a_chip_can_hide_are_the_envelopes_own_and_all_of_them() {
        // Four shapes and the frames in none of them: a chip for every shape
        // and none for the leftovers would be a surface that can hide traffic
        // with nothing to press to bring it back.
        assert_eq!(Kinds::ALL.len(), 5);
        let everything = Kinds::default();
        for kind in Kinds::ALL {
            assert!(everything.shows(kind), "a surface opens showing everything");
            assert!(
                !everything.toggled(kind).shows(kind),
                "and a chip hides the shape it names"
            );
            assert!(
                Kinds::ALL
                    .into_iter()
                    .filter(|other| *other != kind)
                    .all(|other| everything.toggled(kind).shows(other)),
                "and nothing else"
            );
        }
    }
}
