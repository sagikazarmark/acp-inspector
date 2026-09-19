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
//! showing the frame the reader pointed at — and the newest frame until they
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

use std::collections::HashSet;
use std::path::PathBuf;

use acp_inspector_core::{Direction, Frame, FrameKind, Indentation, TracedFrame};
use dioxus::prelude::*;

use crate::console::{matching, narrowed};
use crate::copy::Copy;
use crate::json;
use crate::time::stamp;

/// Put a Frame named by the Timeline under both the viewport and keyboard
/// focus. A Frame's row is a button, so it is the useful destination rather
/// than the list item around it.
const FOCUS_FRAME: &str = r#"
const ordinals = await dioxus.recv();
let target;
for (let attempt = 0; attempt < 60 && !target; attempt += 1) {
  await new Promise((resolve) => requestAnimationFrame(resolve));
  target = ordinals
    .map((ordinal) => document.querySelector(`[data-frame="${ordinal}"] .frame-row`))
    .find((row) => row?.getClientRects().length && row.dataset.revealed === "true");
}
if (target) {
  target.scrollIntoView({ block: "center" });
  target.focus({ preventScroll: true });
}
"#;

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

/// Focus the first named Frame the bounded Trace still holds after the Console
/// has rendered its Trace surface.
pub(crate) fn focus_frames(ordinals: Vec<u64>) {
    let focusing = document::eval(FOCUS_FRAME);
    let _ = focusing.send(ordinals);
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
    saved: Option<Result<PathBuf, String>>,
    /// What the Console's bar is narrowing this surface to. Presentation only:
    /// the Trace goes on capturing, counting and exporting everything.
    filter: String,
    /// And which envelope shapes it is showing, from the chips beside the
    /// filter.
    kinds: Kinds,
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
    /// which the pane answers with the newest frame rather than with an empty
    /// region.
    selected: Option<u64>,
    on_choose: EventHandler<u64>,
    /// A row's frame, on its way back to the entry it became.
    on_seek: EventHandler<u64>,
) -> Element {
    let mut anchor = use_signal(|| revealed.read().first().copied());
    use_effect(move || {
        if let Some(first) = revealed.read().first().copied() {
            anchor.set(Some(first));
        }
    });
    let held_frames = frames.read();
    let empty = held_frames.is_empty();
    let held = held_frames.len();
    // The frame as the wire had it, which is what the row's preview draws and
    // what an export writes — so what the reader types is matched against the
    // same bytes they are looking at, `method`, `id`, session and all.
    let rows = rows(held_frames.clone());
    // Which rows survive the two narrowings, by their place in the list.
    // Computed once: the list draws them, and the pane reads the newest of them
    // — a pane reading a frame the reader has hidden would be the one surface
    // answering a narrowing with the row it just took away.
    let visible: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| kinds.shows(row.entry.frame.summary().kind))
        .filter(|(_, row)| filter.trim().is_empty() || matching(row.entry.frame.as_str(), &filter))
        .map(|(position, _)| position)
        .collect();
    let shown = visible.len();
    let ordinals: Vec<_> = visible.iter().map(|p| (dropped + p) as u64).collect();
    let range = crate::page::range(&ordinals, anchor());
    // What the surface is hiding, said where the rows it hid would have been —
    // whether the narrowing was typed or pressed, because a chip that is off
    // takes rows away exactly as a filter does.
    let narrowing = !filter.trim().is_empty() || kinds != Kinds::default();

    rsx! {
        div { class: "trace", "data-slot": "trace",
            {narrowed(shown, held, "frames", narrowing)}
            {crate::page::controls(&ordinals, range.clone(), anchor)}
            if let Some(saved) = saved {
                match saved {
                    Ok(path) => rsx! {
                        p { class: "saved", role: "status",
                            "Exported to "
                            // Selectable, and the whole path: the next thing
                            // the user does with it is attach it to a bug
                            // report, and a path they have to retype is a path
                            // they will get wrong.
                            code { class: "mono", "{path.display()}" }
                        }
                    },
                    Err(problem) => rsx! {
                        p { class: "saved bad", role: "status", "The export was not written: {problem}" }
                    },
                }
            }

            if empty {
                p { class: "empty",
                    "Every JSON-RPC frame, both directions, exactly as it crossed the wire."
                }
            } else {
                ol { class: "wire-rows", "data-tail": FRAMES,
                    // Newest first in the DOM, oldest first on screen: the list
                    // is laid out bottom-up, which is what keeps a live trace
                    // showing its latest frame without a scroll script — and
                    // leaves it where the reader put it once they scroll back.
                    //
                    // **Keyed by the ordinal the frame was captured under, not
                    // by where it now sits in the list.** The trace is a ring:
                    // once it is full, every frame recorded drops the oldest and
                    // shifts every position by one, and a row keyed by position
                    // would hand whatever the reader had selected to whichever
                    // frame landed there next.
                    for position in visible[range].iter().copied().rev() {
                        {frame_row(
                            &rows[position],
                            (dropped + position) as u64,
                            revealed,
                            decoded,
                            selected,
                            on_choose,
                            on_seek,
                        )}
                    }
                }
                {crate::tail::to_latest(FRAMES, "frame")}

                // And the one the reader is reading. Always drawn while there
                // are frames, because a pane that came and went would move the
                // list under the click that selected a row in it.
                {reading(&rows, &visible, dropped, selected)}
            }
        }
    }
}

/// One frame, as the row that selects it.
fn frame_row(
    row: &Row,
    ordinal: u64,
    revealed: ReadSignal<Vec<u64>>,
    decoded: ReadSignal<HashSet<u64>>,
    selected: Option<u64>,
    on_choose: EventHandler<u64>,
    on_seek: EventHandler<u64>,
) -> Element {
    let entry = &row.entry;
    let way = way(entry.direction);
    let summary = entry.frame.summary();
    let kind = summary.kind;
    let marked = revealed().contains(&ordinal);
    let chosen = selected == Some(ordinal);

    rsx! {
        li {
            key: "{ordinal}",
            "data-frame": "{ordinal}",
            // The row where an agent gave way to the next one draws the
            // boundary: the ordinal on every row says whose traffic each is,
            // and this says where the conversation changed.
            "data-switched": "{row.starts}",
            div { class: if row.starts { "frame-line switched" } else { "frame-line" },
                button {
                    class: if marked { "frame-row revealed" } else { "frame-row" },
                    // `aria-current`, not `aria-selected`: this is the row the
                    // pane below is reading, which is the one thing a reader can
                    // point at rather than a selection in a listbox.
                    aria_current: chosen.then_some("true"),
                    "data-revealed": "{marked}",
                    aria_label: frame_name(ordinal, entry),
                    title: "Read frame {ordinal} in the pane below",
                    onclick: move |_| on_choose.call(ordinal),
                    // Which pipe it crossed, on every frame and exactly as the
                    // trace recorded it — the same ordinal the JSONL export
                    // writes, so the screen and a file taken from it agree.
                    span {
                        class: "frame-crossing",
                        "data-slot": "crossing",
                        title: "connection {entry.connection}, counting from the first agent this window launched",
                        "#{entry.connection}"
                    }
                    span { class: "frame-at", "{stamp(entry.at)}" }
                    span { class: "frame-arrow {way.tone}", title: "{way.whence}", "{way.arrow}" }
                    // What the envelope says this frame is, in front of the
                    // bytes and never instead of them (`Frame::summary`). A
                    // column of identical prefixes is not a column: every row
                    // began `{"jsonrpc":"2.0","id":` — forty characters of the
                    // same nine bytes before the one word that says which call
                    // this is.
                    {summarised(&entry.frame)}
                    // One line of the frame, as the frame has it. The wire's own
                    // line whatever the indentation switch says: a preview is one
                    // line by construction, and the first line of an indented
                    // document is `{`.
                    span { class: "frame-peek", "{crate::page::prefix(entry.frame.as_str(), 512)}" }
                    span { class: "frame-kind {kind_tone(kind)}", "{named_kind(kind)}" }
                }
                // The row's own control sits beside it and never inside it, so
                // either has one unambiguous job.
                //
                // **Copying is the pane's**, which is where the frame is: what a
                // row draws is the head of a payload with the rest cut off, and
                // a copy beside it either takes bytes the reader cannot see or
                // takes what they can and is not the frame. Pressing the row
                // reads it whole below, and that is where it is taken from.
                span { class: "controls",
                    if decoded().contains(&ordinal) {
                        button {
                            class: "btn btn-xs btn-quiet",
                            "data-slot": "reach",
                            r#type: "button",
                            title: "Show what the timeline made of this frame",
                            aria_label: "Show Timeline entry for Frame {ordinal}",
                            onclick: move |_| on_seek.call(ordinal),
                            "turn"
                        }
                    }
                }
            }
        }
    }
}

/// The frame the pane is reading: the one that was chosen, or the newest.
///
/// **The newest rather than nothing**, because that is what a reader watching a
/// live trace is looking at anyway — and because a pane that is empty until
/// something is clicked spends two fifths of the Console saying *click
/// something*.
fn reading(rows: &[Row], visible: &[usize], dropped: usize, selected: Option<u64>) -> Element {
    let found = selected
        .and_then(|ordinal| {
            let position = usize::try_from(ordinal).ok()?.checked_sub(dropped)?;
            rows.get(position).map(|row| (ordinal, row))
        })
        .or_else(|| {
            // The newest frame the reader can *see*: a pane reading one they
            // narrowed away would be the surface answering the chip with the
            // row it just hid.
            let position = visible.last().copied()?;
            Some(((dropped + position) as u64, rows.get(position)?))
        });

    let Some((ordinal, row)) = found else {
        return rsx! {};
    };
    let entry = &row.entry;
    let way = way(entry.direction);
    let summary = entry.frame.summary();
    let label = summary.label().unwrap_or("unreadable envelope").to_owned();
    // Laid out, always, and this is the surface that says why (see the module
    // note): one selected document, on the one screen whose subject it is.
    let large = entry.frame.as_str().len() > 16 * 1024;
    let payload = if large {
        String::new()
    } else {
        Indentation::Indented
            .draw(entry.frame.as_str())
            .into_owned()
    };

    rsx! {
        aside {
            class: "frame-pane",
            "data-slot": "frame-detail",
            aria_label: "The selected frame",
            header { class: "frame-pane-head",
                span { class: "frame-arrow {way.tone}", title: "{way.whence}", "{way.arrow}" }
                span { class: "frame-method", "{label}" }
                span { class: "frame-pane-meta",
                    "{named_kind(summary.kind)} · #{ordinal} · {stamp(entry.at)}"
                }
                Copy { frame: entry.frame.clone(), what: "this frame" }
            }
            if large {
                crate::page::RawFrame { key: "{ordinal}", frame: entry.frame.clone() }
            } else { pre { class: "frame-json", "data-slot": "frame-payload",
                for (position, (token, text)) in json::tokens(&payload).into_iter().enumerate() {
                    span { key: "{position}", class: token.class(), "{text}" }
                }
            } }
        }
    }
}

/// What a frame says it is, drawn in front of what it says.
///
/// Core reads the envelope ([`Frame::summary`]); this decides what a row does
/// with the answer, which is one word and, where there is one, the id under
/// which its answer will come back. Nothing is drawn for a frame the envelope
/// could not be read out of — the row is the bytes then, which is what every
/// row was before this existed.
fn summarised(frame: &Frame) -> Element {
    let summary = frame.summary();
    let Some(label) = summary.label() else {
        return rsx! {};
    };

    rsx! {
        span { class: "frame-method", "data-slot": "envelope", "{label}" }
        if let Some(id) = &summary.id {
            span { class: "frame-id", title: "JSON-RPC id {id}", "#{id}" }
        }
    }
}

/// One frame as the view draws it: the frame, and whether the connection
/// changed at it.
struct Row {
    entry: TracedFrame,
    /// Whether the frame before it crossed a *different* connection — the
    /// handover from one agent to the next, seen.
    ///
    /// False for the oldest frame the trace still holds, which has no frame
    /// before it to have changed from: the trace is a ring that drops its
    /// oldest, so the top of the list is where the record begins and not
    /// necessarily where the connection did.
    starts: bool,
}

/// Marks where one connection's traffic gave way to the next.
///
/// The trace outlives any one agent (`Trace::tap`), so a trace can hold two
/// agents' frames and every agent's JSON-RPC ids start again at 1. The ordinal
/// on each row says whose a frame is; this says where one agent stopped
/// speaking and the next started.
fn rows(frames: Vec<TracedFrame>) -> Vec<Row> {
    let mut previous = None;
    frames
        .into_iter()
        .map(|entry| {
            let starts = previous
                .replace(entry.connection)
                .is_some_and(|before| before != entry.connection);
            Row { entry, starts }
        })
        .collect()
}

/// How a direction reads: the inspector is always one end of it (`CONTEXT.md`,
/// *Connection*), so both of them are its own.
struct Way {
    arrow: &'static str,
    whence: &'static str,
    tone: &'static str,
}

fn way(direction: Direction) -> Way {
    match direction {
        Direction::ToAgent => Way {
            arrow: "→",
            whence: "sent to the agent",
            tone: "out",
        },
        Direction::FromAgent => Way {
            arrow: "←",
            whence: "received from the agent",
            tone: "in",
        },
    }
}

fn frame_name(ordinal: u64, frame: &TracedFrame) -> String {
    format!(
        "Frame {ordinal}, {}, connection {}, {}. Read it in the pane below",
        way(frame.direction).whence,
        frame.connection,
        stamp(frame.at),
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

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
    }

    fn crossed(connection: u64, direction: Direction, json: &str) -> Crossed {
        Crossed {
            connection,
            direction,
            json: json.to_owned(),
        }
    }

    impl Crossed {
        fn traced(&self) -> TracedFrame {
            TracedFrame {
                at: at(self.connection),
                connection: self.connection,
                direction: self.direction,
                frame: Frame::new(self.json.as_str()),
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
        revealed: Vec<u64>,
        decoded: Vec<u64>,
        selected: Option<u64>,
    }

    impl Default for Screen {
        fn default() -> Self {
            Self {
                frames: traffic(),
                dropped: 0,
                filter: String::new(),
                kinds: Kinds::default(),
                revealed: Vec::new(),
                decoded: Vec::new(),
                selected: None,
            }
        }
    }

    fn shown(screen: Screen) -> String {
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
            rsx! {
                TraceView {
                    frames,
                    dropped: screen.dropped,
                    saved: None,
                    filter: screen.filter.clone(),
                    kinds: screen.kinds,
                    revealed,
                    decoded,
                    selected: screen.selected,
                    on_choose: move |_: u64| {},
                    on_seek: move |_: u64| {},
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { screen });
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
        assert!(
            html.contains(r#"data-slot="crossing""#) && html.contains("#1"),
            "and every row says whose traffic it is: {html}"
        );
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
        let first = rows_of(&html).last().cloned().unwrap_or_default();
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
    fn cross_screen_navigation_scrolls_and_focuses_the_row_itself() {
        // The row is a button, so it is the useful destination rather than the
        // list item around it — and the ordinals arrive over Dioxus's channel,
        // so nothing an agent said becomes part of a program this window runs.
        assert!(
            FOCUS_FRAME.contains("dioxus.recv()")
                && FOCUS_FRAME.contains(r#"[data-frame="${ordinal}"] .frame-row"#)
                && FOCUS_FRAME.contains("scrollIntoView")
                && FOCUS_FRAME.contains("focus({ preventScroll: true })"),
            "{FOCUS_FRAME}"
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
