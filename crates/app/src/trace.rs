//! The trace view: every frame, both directions, timestamped, and the selected
//! one read whole in the pane under the list. A body of the Console's Trace tab
//! (`console::Console`). See `docs/architecture.md` §8–§9.
//!
//! - Raw always: rows and pane show the frame's own bytes; nothing here parses
//!   or rewrites them, so an unparseable frame is still drawn in full.
//! - A row is selected, not expanded: one pane at the foot of the list shows
//!   the chosen frame, or the newest displayed one until a reader chooses
//!   (ADR 0009).
//! - The pane lays out its one document whatever the Indentation switch says
//!   (`crate::indent`); copy and export still take the bytes that crossed.
//! - Requests and answers point at each other through core's pairing; lossy,
//!   dropped and hand-sent frames say what they are.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use acp_inspector_core::{
    Diagnostic, DiagnosticKind, Direction, Frame, FrameKind, Indentation, Omission, TracedFrame,
};
use dioxus::prelude::*;

use crate::console::{Directions, contains_folded, narrowed};
use crate::copy::Copy;
use crate::disclosure::{Role, icon};
use crate::export::Outcome;
use crate::json;
use crate::time::stamp;
use crate::wire::{Elapsed, Wire, elapsed};

mod filter;
mod pairing;

use filter::{Index, Query};
use pairing::{Pairs, is_answer, pairing};

/// The frame list's id, as the scroll watcher knows it (`tail.rs`).
pub(crate) const FRAMES: &str = "frames";

/// Which envelope shapes are shown. Presentation only: the trace still captures,
/// counts and exports everything. Frames in none of the four JSON-RPC shapes get
/// a chip too, so nothing can be hidden without a way back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kinds {
    call: bool,
    result: bool,
    notification: bool,
    error: bool,
    unreadable: bool,
}

impl Default for Kinds {
    /// Everything, which is what the surface opens in.
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
            // Non-exhaustive: an unknown shape goes with the unreadable ones.
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

/// What a shape is called on its chip and row: JSON-RPC's own four nouns.
pub fn named_kind(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Call => "request",
        FrameKind::Result => "response",
        FrameKind::Notification => "notif",
        FrameKind::Error => "error",
        _ => "other",
    }
}

/// The class a shape's word is drawn in.
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

/// One row of the merged view: a frame, or a line of the agent's stderr.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// By position in the frames held.
    Frame(usize),
    /// By position in the diagnostic lines held.
    Line(usize),
}

/// Frames and stderr lines in read order, each keyed by how many frames and lines
/// were captured before it, so keys survive either list aging out. Frames keep
/// the trace's order; a line goes before the first frame read after it.
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
        // Every line before this one counts, so the key survives either kind
        // aging out.
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
    /// How many frames the trace captured and no longer holds; numbers the rows.
    /// The tab says it, so a partial count never looks like the whole session.
    dropped: usize,
    /// What became of the last export: the file it wrote, or why it did not.
    saved: Option<Outcome>,
    /// Puts that notice away.
    #[props(default)]
    on_dismiss_saved: Option<EventHandler<()>>,
    /// What the Console's bar narrows this surface to. `method:NAME` and `id:N`
    /// match those envelope fields; the rest is text.
    filter: String,
    /// Which envelope shapes are shown, from the chips beside the filter.
    kinds: Kinds,
    /// Which directions are shown.
    #[props(default)]
    directions: Directions,
    /// The frames a Timeline entry was made of, sent here from that entry.
    /// Marked and scrolled to, never filtered: the traffic around them stays.
    revealed: ReadSignal<Vec<u64>>,
    /// Which captured frames became a Timeline entry, so a row can say so.
    decoded: ReadSignal<HashSet<u64>>,
    /// The frame the pane is reading, or `None` for the newest displayed one.
    selected: Option<u64>,
    on_choose: EventHandler<u64>,
    /// A row's frame, on its way back to the entry it became.
    on_seek: EventHandler<u64>,
    /// A narrowing the pane offers ("only this method", "only this id"), for
    /// the Console's filter box.
    #[props(default)]
    on_filter: Option<EventHandler<String>>,
    /// The connection's tools: pairing, pending calls, cancelling and sending
    /// by hand. Without one the rows are drawn unpaired.
    #[props(default)]
    wire: Option<Wire>,
    /// The diagnostic lines, for the merged view.
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
    // Computed once so the page and the pane agree: the pane must never read a
    // frame the reader has narrowed away.
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

    // Merged, the agent's stderr lines that match the text interleave by read
    // time.
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
    // Typed or pressed, a narrowing is said where its rows would have been.
    let narrowing = !filter.trim().is_empty()
        || kinds != Kinds::default()
        || directions != Directions::default()
        || pending_only;
    // Connection ordinals tell rows apart only when more than one agent is held.
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
    // The list's one Tab stop: the row last focused while it is on the page,
    // then the one the pane reads, then the newest.
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
                    // Oldest first in the DOM. The scroller lays out bottom-up,
                    // so a live trace stays on its newest frame without a scroll
                    // script. Keyed by capture ordinal, not position: the ring
                    // shifts positions as it drops frames.
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

                // Always drawn while there are frames, so the list never moves
                // under the click that selected a row.
                {reading(&pairs, &page_frames, selected, on_filter, on_choose, anchor, wire)}
            }
        }
    }
}

/// The calls this client is still waiting on, with how long each has waited and
/// a way to give one up.
fn pending_calls(
    wire: Wire,
    waiting: &[acp_inspector_core::PendingCall],
    on_choose: EventHandler<u64>,
    mut anchor: Signal<Option<u64>>,
) -> Element {
    // Said while the call is still waiting, or where cancelling failed; once
    // answered, the answer says the rest.
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

/// One line of the agent's stderr, between the frames.
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
    /// Whether the previous frame crossed a different connection. False for the
    /// oldest frame held.
    starts: bool,
    /// Whether the trace holds more than one connection's traffic.
    several: bool,
    /// For an answer, the method of the call it answers, while that is held.
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
            // Where one agent gave way to the next.
            "data-switched": "{starts}",
            div { class: if starts { "frame-line switched" } else { "frame-line" },
                button {
                    class: if marked { "frame-row revealed" } else { "frame-row" },
                    // `aria-current`, not `aria-selected`: the row the pane
                    // reads, not a listbox selection.
                    aria_current: chosen.then_some("true"),
                    tabindex,
                    "data-revealed": "{marked}",
                    aria_label: frame_name(entry, answering, several),
                    title: "Read frame {ordinal} in the pane below",
                    onclick: move |_| on_choose.call(ordinal),
                    onfocus: move |_| focused.set(Some(ordinal)),
                    // The connection ordinal, as the JSONL export writes it.
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
                    // The envelope's shape, in front of the bytes.
                    {summarised(&entry.frame, answering)}
                    {tags(&entry.frame, mistyped, unanswered)}
                    // The wire's own first line whatever the indentation: the
                    // first line of an indented document is `{`.
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
                    span { class: "tag frame-kind {kind_tone(kind)}", "{named_kind(kind)}" }
                }
                // Controls sit beside the row, never inside. A Tab stop only on
                // the list's current row.
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
                    // An icon, since a column of words would be wider than the
                    // frames; the pane says it in words.
                    if let Some(wire) = resend {
                        button {
                            class: "btn btn-xs btn-quiet btn-square",
                            "data-slot": "resend",
                            r#type: "button",
                            tabindex,
                            title: "Edit and resend: copy this frame into the raw composer",
                            aria_label: "Edit and resend Frame {ordinal}",
                            onclick: move |_| wire.resend(&copy),
                            dioxus_free_icons::Icon { class: "icon", width: 16, height: 16, icon: dioxus_free_icons::icons::ld_icons::LdPencilLine }
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

/// The frame the pane is reading: the chosen one, or the newest on the page.
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
            // The newest the reader can see, never one they narrowed away.
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
    // Always laid out: one selected document (see the module docs).
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

/// The envelope's shape word and id, drawn in front of the frame. An answer whose
/// call is held names the call (`↳ initialize`). Nothing is drawn for a frame
/// whose envelope could not be read.
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

/// How a direction reads: the inspector is always one end.
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

/// A row's accessible name: method, kind and id, direction, and time; the
/// connection only where there are several. E.g. "initialize request #1, sent
/// to the agent, 21:20:24.000".
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

/// "id has the wrong type (string "3" for call 3)".
// Here rather than in `pairing`: crates/core/tests/decode_boundary.rs pins this
// JSON read to this file.
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use acp_inspector_core::Trace;

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

    /// One frame as a test describes it. Not a [`TracedFrame`], because a
    /// screen's props must be comparable.
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

    /// The default trace: a call out, its answer, then a notification.
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

    /// What the surface is asked to show.
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
        /// Calls still awaiting answers: id, and the ordinal it went out under.
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

    /// Every row, in the order the list draws them.
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
        // One trace can hold two agents whose JSON-RPC ids both restart at 1;
        // the ordinal (as the export writes it) tells their frames apart.
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
        // Method, kind and id lead the accessible name; an answer names its call.
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
        // Oldest first in the DOM: the line read between the first two frames is
        // second from the top.
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
        // Never on the oldest frame held: there is no frame before it.
        let first = rows_of(&html).first().cloned().unwrap_or_default();
        assert!(
            first.contains(r#"data-switched="false""#),
            "the top of the list is where the record begins: {first}"
        );
    }

    #[test]
    fn a_row_says_what_the_envelope_is_without_standing_in_for_the_bytes() {
        let html = shown(Screen::default());
        assert!(
            html.contains(r#"data-slot="envelope""#)
                && html.contains(">initialize</span>")
                && html.contains(">#1</span>")
                && html.contains(r#"class="tag frame-kind call""#)
                && html.contains(r#"class="tag frame-kind result""#)
                && html.contains(r#"class="tag frame-kind notification""#),
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
            html.contains("not json at all")
                && html.contains(r#"class="tag frame-kind unreadable""#),
            "the bytes are the row, and the shape says it could not be read: {html}"
        );
    }

    #[test]
    fn the_pane_reads_the_frame_a_reader_chose_and_the_newest_until_they_do() {
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
        // One selected document, laid out (`crate::indent`).
        assert!(
            pane.contains(r#"data-slot="frame-payload""#)
                && pane.contains("\n  ")
                && pane.contains(r#"class="json-key""#)
                && pane.contains(r#"class="json-string""#),
            "the pane draws the payload laid out: {pane}"
        );
        // And every byte of it.
        assert!(
            pane.contains(r#"aria-label="Copy this frame""#),
            "the copy beside it takes the bytes that crossed: {pane}"
        );
        // The row's preview stays the wire's first line.
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
        // Matched against the bytes, not anything decoded.
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
        // Copy is offered only where the whole frame is drawn: the pane, not a
        // truncated row.
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
        // A chip for the frames in none of the four shapes, so nothing can be
        // hidden without a way back.
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
