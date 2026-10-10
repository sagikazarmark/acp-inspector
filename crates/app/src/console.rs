//! The Console (`docs/architecture.md` §9): the wire, beside the turn rather
//! than beneath it, holding the Trace and the Diagnostic channel as tabs and
//! showing one at a time.
//!
//! Both of its surfaces are what was captured *below* the typed layer, which is
//! what makes them one panel and the timeline not part of it (`CONTEXT.md`,
//! *Console*). What changed with the three-column instrument (ADR 0009) is
//! where the panel is: it spanned the window because a frame is a long line,
//! and it is a column now because a frame is read *whole* in the pane at the
//! foot of the list rather than unfolded into the row it is in. The row keeps
//! the ordinal, the direction, the envelope and the first line; the pane has
//! the room.
//!
//! The diagnostic channel carries the agent's own stderr and the transport's
//! remarks about the connection in one list, because they are one channel and
//! reading them apart is how "the agent explained itself and then died" stops
//! being legible.
//!
//! **One escalation into it** — the connection status, through
//! [`surfaces_diagnostics`], applied by [`App`](crate::App) so that the console
//! is a view of a store here too. Nothing else on this panel escalates: a tab
//! says how much of it there is and never that the reader should look.

use std::collections::HashSet;

use acp_inspector_core::{
    ConnectionStatus, Diagnostic, DiagnosticKind, Direction, FrameKind, Stop, TracedFrame,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdDownload, LdTrash2},
};

use crate::copy::Copy;
use crate::time::stamp;
use crate::trace::{Kinds, TraceView, named_kind};
use crate::wire::Wire;

/// Which of the Console's two surfaces is on screen.
///
/// Window-session state and nothing else: it is never written to
/// `settings.json`, because a window that opened onto an empty Diagnostic tab
/// because of what was being debugged yesterday would be a window whose first
/// screen is about the last session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    /// Every frame that crossed the wire. The default: it is the surface that
    /// has something to say about every agent, including one that is behaving.
    #[default]
    Trace,
    /// Everything the transport said about the Connection that was not a Frame.
    Diagnostics,
}

impl Tab {
    /// What the tab is called in the document, so the body on screen can name
    /// the tab that selected it.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Trace => "console-tab-trace",
            Self::Diagnostics => "console-tab-diagnostics",
        }
    }

    fn panel_id(self) -> &'static str {
        match self {
            Self::Trace => "console-panel-trace",
            Self::Diagnostics => "console-panel-diagnostics",
        }
    }

    /// What it is called on screen.
    fn label(self) -> &'static str {
        match self {
            Self::Trace => "Trace",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

/// The panel: two tabs, one filter bar, one body.
#[component]
pub fn Console(
    frames: ReadSignal<Vec<TracedFrame>>,
    /// How many frames the trace captured and no longer holds — cleared, or
    /// aged out of its cap. Named for what it counts, because the other surface
    /// has one too.
    dropped_frames: usize,
    /// What became of the last export: the file it wrote, or why it did not.
    saved: Option<crate::export::Outcome>,
    #[props(default)] export_pending: bool,
    on_export: EventHandler<()>,
    on_clear: EventHandler<()>,
    lines: ReadSignal<Vec<Diagnostic>>,
    /// How many Diagnostic entries aged out under either retention budget.
    dropped_lines: usize,
    tab: Tab,
    on_select: EventHandler<Tab>,
    /// The frames a Timeline entry was made of, sent here by somebody reading
    /// that entry — and which of the trace's frames became one, so a row can
    /// offer the way back.
    revealed: ReadSignal<Vec<u64>>,
    decoded: ReadSignal<HashSet<u64>>,
    on_seek: EventHandler<u64>,
    /// The connection's tools beyond the frames: pairing, pending calls,
    /// sending by hand, and the trace opened from a file (`crate::wire`).
    #[props(default)]
    wire: Option<Wire>,
) -> Element {
    // Whether there is anything to export or clear. Read here rather than in
    // the Trace because the controls are here, and it is the same question the
    // tab's own count answers.
    let nothing_traced = frames.read().is_empty();
    let cannot_export = nothing_traced || export_pending;
    let viewing = wire.is_some_and(|wire| wire.view.is_open());
    // The export notice, until the reader puts it away.
    let mut dismissed = use_signal(|| None::<u64>);
    let saved = saved.filter(|outcome| dismissed() != Some(outcome.id));
    let saved_id = saved.as_ref().map(|outcome| outcome.id);
    let composing = wire.is_some_and(|wire| (wire.composing)()) && !viewing;
    // And whether the surface in front of the reader has captured anything at
    // all, which is what decides whether there is anything to narrow.
    let empty = match tab {
        Tab::Trace => nothing_traced,
        Tab::Diagnostics => lines.read().is_empty(),
    };

    // What the reader is looking for, narrowing whichever surface is on screen.
    //
    // **Presentation and nothing else** (§5): both stores keep everything, both
    // counts on the tabs go on saying how much there is, and an export writes
    // the whole trace whatever is typed here.
    //
    // One box for both surfaces rather than one each. They are two views of one
    // connection behind two tabs in one panel, and a reader who narrows to a
    // session id and switches tabs is asking the same question of the other
    // account of it.
    let mut filter = use_signal(String::new);
    // And which envelope shapes the Trace is showing, which is the same kind of
    // narrowing said as chips because the answers are a closed set (`trace.rs`).
    let mut kinds = use_signal(Kinds::default);
    // And which way they crossed (F8).
    let mut directions = use_signal(Directions::default);
    // Which frame the pane under the list is reading. The Console's rather than
    // the Trace's, because arriving from the Timeline changes it and that
    // request is delivered to the panel.
    let mut selected = use_signal(|| None::<u64>);

    // Except when somebody arrives from the Timeline. A row asked for by
    // ordinal is a row this panel has been told to *show*, and a narrowing it
    // does not match would answer that by showing nothing at all — so both
    // narrowings give way to the request, and the frame it named is the one the
    // pane reads.
    use_effect(move || {
        if let Some(first) = revealed().first().copied() {
            filter.set(String::new());
            kinds.set(Kinds::default());
            directions.set(Directions::default());
            selected.set(Some(first));
        }
    });

    // Whether any frame is in none of the envelope's four shapes, which is what
    // decides whether the fifth chip is worth drawing: a chip that can hide
    // nothing is chrome, and one missing for traffic that exists would be a
    // surface able to hide frames with nothing to press to bring them back.
    let unreadable = frames.read().iter().any(|traced| {
        !matches!(
            traced.frame.summary().kind,
            FrameKind::Call | FrameKind::Result | FrameKind::Notification | FrameKind::Error
        )
    });

    rsx! {
        section { class: "wire", aria_label: "Console",
            header { class: "pane-head",
                div { class: "wire-tabs", role: "tablist", aria_label: "Console",
                    TabControl {
                        tab: Tab::Trace,
                        selected: tab,
                        held: frames.read().len(),
                        gone: dropped_frames,
                        on_select,
                    }
                    TabControl {
                        tab: Tab::Diagnostics,
                        selected: tab,
                        held: lines.read().len(),
                        gone: dropped_lines,
                        on_select,
                    }
                }
                span { class: "grow" }

                // What a reader does with a trace once they have one, on the bar
                // that already names it — and only on the surface they act on:
                // the diagnostic channel is neither exported nor cleared, and a
                // control that did nothing on the tab in front of you is worse
                // than one that is not there.
                if tab == Tab::Trace {
                    if let Some(wire) = wire {
                        if !viewing {
                            button {
                                class: "btn btn-xs btn-quiet",
                                "data-slot": "compose",
                                r#type: "button",
                                aria_expanded: composing,
                                title: "Write a request or notification by hand and send it to the agent",
                                onclick: move |_| {
                                    let mut composing = wire.composing;
                                    let now = !composing();
                                    composing.set(now);
                                },
                                "Send…"
                            }
                        }
                        if crate::view::CAN_OPEN {
                            button {
                                class: "btn btn-xs btn-quiet",
                                "data-slot": "open-trace",
                                r#type: "button",
                                title: "Open an exported trace, read-only",
                                onclick: move |_| wire.view.choose(wire.live()),
                                "Open…"
                            }
                        }
                    }
                    button {
                        class: "btn btn-xs btn-quiet",
                        "data-slot": "export",
                        r#type: "button",
                        aria_label: "Export the trace",
                        aria_disabled: cannot_export.then_some("true"),
                        tabindex: cannot_export.then_some("-1"),
                        title: "Write every frame to a JSONL file, nothing redacted",
                        onclick: move |_| {
                            if !cannot_export {
                                on_export.call(());
                            }
                        },
                        Icon { class: "icon", width: 16, height: 16, icon: LdDownload }
                        "Export…"
                    }
                    if let Some(wire) = wire {
                        label {
                            class: "export-stderr",
                            "data-slot": "export-stderr",
                            title: "Also write the agent's stderr lines into the export, interleaved by time",
                            input {
                                r#type: "checkbox",
                                class: "checkbox checkbox-xs",
                                checked: (wire.include_stderr)(),
                                onchange: move |event| {
                                    let mut include = wire.include_stderr;
                                    include.set(event.checked());
                                },
                            }
                            "with stderr"
                        }
                    }
                    if !viewing {
                        ClearTrace { nothing_traced, on_clear }
                    }
                }
            }

            if export_pending {
                p { class: "saved", role: "status", "Writing the trace…" }
            }

            // Only where there is something to narrow. A surface that has
            // captured nothing says so in its own empty state, and a filter box
            // over it is a control that can do nothing.
            if !empty {
                div { class: "wire-filters",
                    if tab == Tab::Trace {
                        for direction in Directions::ALL {
                            button {
                                key: "{Directions::named(direction)}",
                                class: "chip",
                                "data-slot": "direction",
                                "data-direction": Directions::named(direction),
                                r#type: "button",
                                aria_pressed: directions().shows(direction),
                                title: if direction == Direction::ToAgent { "Show or hide frames sent to the agent" } else { "Show or hide frames received from the agent" },
                                onclick: move |_| directions.set(directions().toggled(direction)),
                                {crate::disclosure::icon(if direction == Direction::ToAgent { crate::disclosure::Role::Outgoing } else { crate::disclosure::Role::Incoming })}
                                "{Directions::named(direction)}"
                            }
                        }
                        for kind in Kinds::ALL {
                            if kind != FrameKind::Unreadable || unreadable {
                                button {
                                    key: "{named_kind(kind)}",
                                    class: "chip",
                                    "data-slot": "kind",
                                    "data-kind": named_kind(kind),
                                    r#type: "button",
                                    aria_pressed: kinds().shows(kind),
                                    title: "Show or hide frames the envelope says are of this shape",
                                    onclick: move |_| kinds.set(kinds().toggled(kind)),
                                    "{named_kind(kind)}"
                                }
                            }
                        }
                        if let Some(wire) = wire {
                            button {
                                class: "chip",
                                "data-slot": "pending",
                                r#type: "button",
                                aria_pressed: (wire.pending_only)(),
                                title: "Show only requests nothing has answered yet",
                                onclick: move |_| {
                                    let mut pending = wire.pending_only;
                                    let now = !pending();
                                    pending.set(now);
                                },
                                "pending"
                            }
                            button {
                                class: "chip",
                                "data-slot": "merged",
                                r#type: "button",
                                aria_pressed: (wire.merged)(),
                                title: "Show the agent's stderr lines among the frames, by when each was read",
                                onclick: move |_| {
                                    let mut merged = wire.merged;
                                    let now = !merged();
                                    merged.set(now);
                                },
                                "stderr"
                            }
                        }
                    }
                    input {
                        class: "input",
                        "data-slot": "filter",
                        r#type: "search",
                        value: "{filter}",
                        spellcheck: false,
                        aria_label: "Filter the console",
                        title: if tab == Tab::Trace {
                            "Show only rows containing this text. method:NAME and id:N match those fields of the envelope."
                        } else {
                            "Show only rows containing this text"
                        },
                        placeholder: "Filter rows…",
                        oninput: move |event| filter.set(event.value()),
                    }
                }
            }

            // Both panel identities stay in the document, so each tab's
            // `aria-controls` always resolves. Only the selected one renders
            // its live list.
            // Both panel identities stay in the document, so each tab's
            // `aria-controls` always resolves; only the selected one renders a
            // live list.
            section {
                id: Tab::Trace.panel_id(),
                class: "tailing",
                role: "tabpanel",
                aria_labelledby: Tab::Trace.id(),
                hidden: tab != Tab::Trace,
                if tab == Tab::Trace {
                    if let Some(wire) = wire.filter(|_| composing) {
                        crate::raw::Composer {
                            wire,
                            frames,
                            dropped: dropped_frames,
                            on_show: move |ordinal| selected.set(Some(ordinal)),
                        }
                    }
                    TraceView {
                        frames,
                        dropped: dropped_frames,
                        saved,
                        on_dismiss_saved: move |()| dismissed.set(saved_id),
                        wire,
                        lines: Some(lines),
                        dropped_lines,
                        filter: filter(),
                        kinds: kinds(),
                        directions: directions(),
                        revealed,
                        decoded,
                        selected: selected(),
                        on_choose: move |ordinal| selected.set(Some(ordinal)),
                        on_seek,
                        on_filter: move |query: String| filter.set(query),
                    }
                }
            }
            section {
                id: Tab::Diagnostics.panel_id(),
                class: "tailing",
                role: "tabpanel",
                aria_labelledby: Tab::Diagnostics.id(),
                hidden: tab != Tab::Diagnostics,
                if tab == Tab::Diagnostics {
                    Diagnostics { lines, dropped: dropped_lines, filter: filter() }
                }
            }
        }
    }
}

/// How long an armed Clear waits for its confirmation before it disarms.
const CONFIRM_FOR: std::time::Duration = std::time::Duration::from_secs(6);

/// Clear, confirmed in two steps (F9).
///
/// **A confirm rather than an Undo**, because the trace store has no way to
/// put cleared frames back, and the frames it drops are the evidence a reader
/// may not have exported yet. The first press arms it and the second clears;
/// Cancel, Escape, or a few seconds of nothing disarm it. The label changes on
/// the same button, so focus stays where it was pressed.
#[component]
fn ClearTrace(nothing_traced: bool, on_clear: EventHandler<()>) -> Element {
    let mut armed = use_signal(|| false);
    let mut arming = use_signal(|| 0_u64);
    let armed_now = armed() && !nothing_traced;

    rsx! {
        span { class: "clear-trace", "data-armed": "{armed_now}",
            onkeydown: move |event| {
                if event.key() == Key::Escape && armed() {
                    event.stop_propagation();
                    armed.set(false);
                }
            },
            button {
                class: if armed_now { "btn btn-xs btn-filled btn-bad confirm" } else { "btn btn-xs btn-quiet btn-bad" },
                "data-slot": "clear",
                r#type: "button",
                aria_label: if armed_now { "Confirm clearing the trace" } else { "Clear the trace" },
                aria_disabled: nothing_traced.then_some("true"),
                tabindex: nothing_traced.then_some("-1"),
                // Said on the control, because it is the one thing about
                // clearing that surprises people: a file already written is
                // not this trace's any more.
                title: "Drop the frames captured so far; exports already written are untouched",
                onclick: move |_| {
                    if nothing_traced {
                        return;
                    }
                    if armed() {
                        armed.set(false);
                        on_clear.call(());
                        return;
                    }
                    armed.set(true);
                    let this = arming() + 1;
                    arming.set(this);
                    spawn(async move {
                        tokio::time::sleep(CONFIRM_FOR).await;
                        if arming() == this {
                            armed.set(false);
                        }
                    });
                },
                Icon { class: "icon", width: 16, height: 16, icon: LdTrash2 }
                if armed_now { "Confirm clear" } else { "Clear" }
            }
            if armed_now {
                button {
                    class: "btn btn-xs btn-quiet",
                    "data-slot": "cancel-clear",
                    r#type: "button",
                    aria_label: "Keep the trace",
                    onclick: move |_| armed.set(false),
                    "Cancel"
                }
            }
        }
    }
}

/// Which way a frame crossed, as the two chips that hide one direction (F8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Directions {
    pub sent: bool,
    pub received: bool,
}

impl Default for Directions {
    fn default() -> Self {
        Self {
            sent: true,
            received: true,
        }
    }
}

impl Directions {
    pub const ALL: [Direction; 2] = [Direction::ToAgent, Direction::FromAgent];

    pub fn shows(self, direction: Direction) -> bool {
        match direction {
            Direction::ToAgent => self.sent,
            Direction::FromAgent => self.received,
        }
    }

    #[must_use]
    pub fn toggled(mut self, direction: Direction) -> Self {
        match direction {
            Direction::ToAgent => self.sent = !self.sent,
            Direction::FromAgent => self.received = !self.received,
        }
        self
    }

    /// What the chip says.
    pub fn named(direction: Direction) -> &'static str {
        match direction {
            Direction::ToAgent => "sent",
            Direction::FromAgent => "received",
        }
    }
}

/// One tab: what it selects, and how much of it there is.
///
/// One control for both, so that "tabs carry counts and nothing else" (§9) is
/// something this file says once rather than twice: what each tab counts is its
/// own, and what a tab is allowed to be is not.
#[component]
fn TabControl(
    tab: Tab,
    /// Which tab the panel is showing — this one, or the other.
    selected: Tab,
    /// How much of the surface there is to read.
    held: usize,
    /// And how much of it was captured and is no longer held. Absent when there
    /// is none, because a count that is not the whole session should never look
    /// like one.
    gone: usize,
    on_select: EventHandler<Tab>,
) -> Element {
    rsx! {
        button {
            id: tab.id(),
            class: "wire-tab",
            r#type: "button",
            role: "tab",
            aria_selected: tab == selected,
            aria_controls: tab.panel_id(),
            tabindex: if tab == selected { "0" } else { "-1" },
            onclick: move |_| on_select.call(tab),
            onkeydown: move |event| {
                let Some(chosen) = tab_for_key(selected, &event.key()) else {
                    return;
                };
                event.prevent_default();
                on_select.call(chosen);
                focus_tab(chosen);
            },
            "{tab.label()}"
            // Nothing where there is nothing. A surface that has captured none
            // of anything says so in the empty state the tab selects, and a
            // badge reading zero is a count of the times it has said it.
            if held > 0 {
                span { class: "count", "data-slot": "count", "{held}" }
            }
            if gone > 0 {
                span {
                    class: "count gone",
                    "data-slot": "gone",
                    title: "Captured, and no longer held: cleared, or aged out of the surface's cap",
                    "{gone} dropped"
                }
            }
        }
    }
}

pub(crate) fn tab_for_key(selected: Tab, key: &Key) -> Option<Tab> {
    const ORDER: [Tab; 2] = [Tab::Trace, Tab::Diagnostics];
    let at = ORDER.iter().position(|tab| *tab == selected).unwrap_or(0);
    step(ORDER.len(), at, key, Axis::Horizontal).map(|next| ORDER[next])
}

fn focus_tab(tab: Tab) {
    focus_id(tab.id());
}

/// Which way a set of roving-focus items is laid out, and so which arrows move
/// through it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    /// A tablist: ←/→ move and wrap, the WAI-ARIA tabs pattern.
    Horizontal,
}

/// Where a key moves the focus in a set of `len` items, from `at` — or `None`
/// for a key that does not move it.
///
/// One answer for the roving sets drawn in Rust (the Console's tabs, the
/// elicitation's Form/Raw tabs, and the rail's tabs if it wants them). The
/// Timeline and Trace lists rove in [`ROVE`], which moves the same way.
pub(crate) fn step(len: usize, at: usize, key: &Key, axis: Axis) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let at = at.min(last);
    match (axis, key) {
        (_, Key::Home) => Some(0),
        (_, Key::End) => Some(last),
        (Axis::Horizontal, Key::ArrowRight) => Some(if at == last { 0 } else { at + 1 }),
        (Axis::Horizontal, Key::ArrowLeft) => Some(if at == 0 { last } else { at - 1 }),
        _ => None,
    }
}

/// One Tab stop for a long list (A4): ↑/↓ move to the row above or below *on
/// screen*, Home/End to the first and last, and Enter presses the row's
/// `activate` control where one is named.
///
/// In the page rather than in Rust, because only the page knows which element
/// a key landed on: a key typed into a field inside a row must stay the
/// field's. Rows are ordered by where they are drawn — which is also their DOM
/// order, oldest first (A3) — so ↓ is always the newer row. Which row
/// holds `tabindex="0"` is the screen's to render; this only moves focus.
const ROVE: &str = r#"
const [listSelector, rowSelector, activateSelector] = await dioxus.recv();
await new Promise(requestAnimationFrame);
const list = document.querySelector(listSelector);
if (!list || list.dataset.roving === "true") return;
list.dataset.roving = "true";
list.addEventListener("keydown", (event) => {
  const row = event.target;
  if (!(row instanceof HTMLElement) || !row.matches(rowSelector)) return;
  if (event.altKey || event.ctrlKey || event.metaKey) return;
  if (event.key === "Enter" && activateSelector) {
    const control = row.querySelector(activateSelector);
    if (control) { event.preventDefault(); control.click(); }
    return;
  }
  const rows = Array.from(list.querySelectorAll(rowSelector))
    .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top);
  const at = rows.indexOf(row);
  let next;
  switch (event.key) {
    case "ArrowDown": next = rows[Math.min(at + 1, rows.length - 1)]; break;
    case "ArrowUp": next = rows[Math.max(at - 1, 0)]; break;
    case "Home": next = rows[0]; break;
    case "End": next = rows[rows.length - 1]; break;
    default: return;
  }
  event.preventDefault();
  if (next && next !== row) {
    next.focus({ preventScroll: true });
    next.scrollIntoView({ block: "nearest" });
  }
});
"#;

/// Makes the list `list` (a selector) rove between its `row`s, once.
pub(crate) fn rove(list: &str, row: &str, activate: Option<&str>) {
    let roving = document::eval(ROVE);
    let _ = roving.send((list.to_owned(), row.to_owned(), activate.map(str::to_owned)));
}

/// Moves focus to the element with this id, once it is in the document.
///
/// The id is sent to the script rather than written into it, so an id built
/// from agent-supplied text cannot become part of a program.
pub(crate) fn focus_id(id: &str) {
    let focusing = document::eval(
        "const id = await dioxus.recv(); requestAnimationFrame(() => document.getElementById(id)?.focus());",
    );
    let _ = focusing.send(id.to_owned());
}

/// Whether a row survives what the reader typed into the Console's bar.
///
/// **Case-insensitive substring, and deliberately nothing cleverer.** A glob or
/// a regular expression is a language to get wrong in a box with no room to say
/// what went wrong in, and what is typed here is a method name, a session id or
/// a word out of a stderr line — all of which are found by containing them. An
/// empty filter matches everything, which is what makes this one expression the
/// whole of the feature rather than a branch at every call site.
pub(crate) fn matching(text: &str, filter: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty() || contains_folded(text, &filter.to_lowercase())
}

/// Whether `text` contains `needle` (already lowercase), ignoring case.
///
/// **Without lowercasing the text** (L1): a retained frame can be megabytes,
/// and copying every one of them to search it was the cost of every keystroke.
/// An ASCII needle — a method, an id, a session id — is compared byte by byte
/// with ASCII folding; anything else falls back to Unicode lowercasing.
pub(crate) fn contains_folded(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if !needle.is_ascii() {
        return text.to_lowercase().contains(needle);
    }
    let needle = needle.as_bytes();
    text.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// What a narrowing is hiding, said where the rows it hid would have been.
///
/// **A surface that is narrowed says so.** The tab's count goes on saying how
/// much the surface holds — it is a fact about the connection and not about the
/// reader — so without this line a filter that matched nothing would be
/// indistinguishable from an agent that had gone quiet, on the one panel whose
/// job is to say what crossed the wire.
///
/// `narrowing` rather than the filter text, because the Trace has two of them:
/// a box that is typed in and a row of chips that are pressed, and a surface
/// missing rows says so whichever took them (`trace.rs`).
pub(crate) fn narrowed(shown: usize, held: usize, what: &str, narrowing: bool) -> Element {
    if !narrowing || held == 0 {
        return rsx! {};
    }

    let count = if shown == 0 {
        format!("No {what} match.")
    } else {
        format!("{shown} of {held} {what}.")
    };

    rsx! {
        p { class: "narrowed", "data-slot": "narrowed",
            "{count} "
            span { class: "hint", "The whole surface is still captured, exported and counted." }
            // Said once the typing settles rather than on every keystroke
            // (F8): a count read aloud per letter is noise.
            Settled { text: count.clone() }
        }
    }
}

/// How long a changing count must hold still before it is announced.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(400);

/// A polite live region that says `text` once it has stopped changing.
#[component]
fn Settled(text: String) -> Element {
    let mut said = use_signal(String::new);
    let mut latest = use_signal(|| 0_u64);
    use_effect(use_reactive!(|text| {
        let this = *latest.peek() + 1;
        latest.set(this);
        spawn(async move {
            tokio::time::sleep(SETTLE).await;
            if *latest.peek() == this {
                said.set(text);
            }
        });
    }));
    rsx! {
        span { class: "sr-only", "data-slot": "announced", role: "status", aria_live: "polite", "{said}" }
    }
}

/// The diagnostic log, by the name the scroll watcher and its control know it
/// under (`tail.rs`).
pub(crate) const LINES: &str = "lines";

/// The diagnostic log, newest at the bottom.
#[component]
fn Diagnostics(
    lines: ReadSignal<Vec<Diagnostic>>,
    dropped: usize,
    /// What the Console's bar is narrowing both surfaces to.
    filter: String,
) -> Element {
    let anchor = use_signal(|| None);
    let held_lines = lines.read();
    let held = held_lines.len();
    // The line as it is *rendered*, which is what the reader is looking at and
    // therefore what they are searching: the transport's own remarks are part
    // of this channel, and a filter that only read the agent's stderr would
    // hide the line saying the agent never started.
    let ordinals: Vec<_> = held_lines
        .iter()
        .enumerate()
        .filter(|(_, diagnostic)| {
            filter.trim().is_empty() || matching(&said(&diagnostic.kind), &filter)
        })
        .map(|(position, _)| (dropped + position) as u64)
        .collect();
    let shown = ordinals.len();
    let range = crate::page::range(&ordinals, anchor());

    rsx! {
        {narrowed(shown, held, "lines", !filter.trim().is_empty())}
        {crate::page::controls(&ordinals, range.clone(), anchor, LINES)}

        if held == 0 {
            p { class: "empty",
                "Everything the transport said about this connection that was not a frame."
            }
        } else {
            div { class: "lines-scroll", "data-tail": LINES,
                "data-page-latest": "{range.end == ordinals.len()}",
            ul { class: "lines", "data-slot": "diagnostics",
                // Oldest first, in the DOM as on screen (A3). The scroller
                // around the list is laid out bottom-up with the list as its
                // one child, which keeps a streaming console pinned to its
                // latest line without a scroll script.
                //
                // Keyed by the ordinal the line was captured under, the way the
                // Trace's rows are, even as the retention budgets rotate it.
                for ordinal in ordinals[range.clone()].iter().copied() {
                    { let diagnostic = &held_lines[ordinal as usize - dropped]; rsx! {
                    li { key: "{ordinal}", class: "line {tone(&diagnostic.kind).line_class()}",
                        span { class: "frame-at", "{stamp(diagnostic.at)}" }
                        span { class: "dot dot-{tone(&diagnostic.kind).dot()}", aria_hidden: "true" }
                        span { class: "sr-only", "{diagnostic_source(&diagnostic.kind)}: " }
                        DiagnosticEvidence { text: std::sync::Arc::<str>::from(said(&diagnostic.kind)) }
                    }
                    } }
                }
            }
            }
            {crate::tail::to_latest(LINES, "line", Some(anchor))}
        }
    }
}

/// Ordinary rows carry only a literal prefix. Opening one mounts a bounded
/// byte-window reader; Copy shares the original text and materializes it only
/// on activation, never the preview or the current window.
#[component]
fn DiagnosticEvidence(text: std::sync::Arc<str>) -> Element {
    let mut open = use_signal(|| false);
    let large = text.len() > 512;
    rsx! {
        div { class: "line-text",
            if large {
                button { class: "btn btn-xs btn-quiet",
                    aria_label: format!("Read complete diagnostic ({} bytes)", text.len()),
                    aria_expanded: open(),
                    onclick: move |_| { let next = !open(); open.set(next); },
                    if open() { "Close complete line" } else { "Read complete line" }
                }
                span { "{crate::page::prefix(&text, 512)}" }
                span { class: "hint", " (preview; {text.len()} bytes total)" }
                if open() {
                    crate::page::RawText { shared: text.clone(), what: "diagnostic" }
                }
            } else {
                "{text}"
            }
        }
        Copy { shared: text, what: "this line" }
    }
}

/// Whether a connection status is one the reader is owed the diagnostic channel
/// for (§9).
///
/// The MCP Inspector flow worth copying verbatim: an agent that is gone without
/// being asked to go puts its own reason on screen, so a launch that failed
/// cannot be mistaken for one that is taking a while. Two states mean that —
/// one for an agent that never ran, one for an agent that ran and died — and
/// core is where they are told apart.
pub fn surfaces_diagnostics(status: ConnectionStatus) -> bool {
    diagnostics_announcement(status).is_some()
}

/// What the persistent live region says when a status automatically surfaces
/// Diagnostics. One policy decides both the surface change and its words so a
/// future status cannot do one without the other.
pub fn diagnostics_announcement(status: ConnectionStatus) -> Option<&'static str> {
    match status {
        ConnectionStatus::FailedToStart => Some("Failed to start. Diagnostics opened."),
        ConnectionStatus::Lost => Some("Connection lost. Diagnostics opened."),
        _ => None,
    }
}

/// The visual tone a line reads in.
#[derive(Clone, Copy)]
enum Tone {
    Agent,
    Bad,
    Note,
}

impl Tone {
    fn line_class(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Bad => "bad",
            Self::Note => "note",
        }
    }

    /// The dot's tone, which repeats what the row already says in words.
    fn dot(self) -> &'static str {
        match self {
            Self::Agent => "idle",
            Self::Bad => "gone",
            Self::Note => "idle",
        }
    }
}

/// What a line says, in plain words: the agent's own stderr verbatim, and the
/// transport's remarks as sentences about what happened to the record.
pub(crate) fn said(kind: &DiagnosticKind) -> String {
    use crate::trace::size;
    match kind {
        DiagnosticKind::Stderr(line) => line.clone(),
        DiagnosticKind::FrameDropped { limit, length } => format!(
            "The agent sent a frame of {}, over the {} limit. It was dropped; a placeholder marks where it was in the trace.",
            size(*length),
            size(*limit)
        ),
        DiagnosticKind::StderrLineCut { limit, length } => format!(
            "The stderr line above was too long: only its first {} of {} are kept.",
            size(*limit),
            size(*length)
        ),
        DiagnosticKind::StderrLineLossy => {
            "The stderr line above wasn't valid UTF-8. Invalid bytes are shown as \u{FFFD}."
                .to_owned()
        }
        DiagnosticKind::Stopped { by, status } => {
            let how = match by {
                Stop::AlreadyExited => "The agent had already exited.",
                Stop::StdinClosed => "The agent stopped when its input was closed.",
                Stop::Terminated => "The agent stopped when asked to terminate (SIGTERM).",
                Stop::Killed => "The agent didn't stop when asked, and was killed (SIGKILL).",
                _ => "The agent stopped.",
            };
            match status {
                Some(status) => format!("{how} ({status})"),
                None => how.to_owned(),
            }
        }
        DiagnosticKind::DrainAbandoned => "Stopped reading the agent's output: the agent exited, but another process still holds its output open.".to_owned(),
        other => other.to_string(),
    }
}

fn tone(kind: &DiagnosticKind) -> Tone {
    match kind {
        DiagnosticKind::Stderr(_) => Tone::Agent,
        DiagnosticKind::SpawnFailed { .. }
        | DiagnosticKind::TransportFailed(_)
        | DiagnosticKind::FrameDropped { .. }
        | DiagnosticKind::Stopped {
            by: Stop::Killed, ..
        } => Tone::Bad,
        // An agent that exits reporting failure is the line the reader is
        // looking for; one that exits cleanly is a remark.
        DiagnosticKind::AgentExited(status) if !status.success() => Tone::Bad,
        _ => Tone::Note,
    }
}

/// Whose voice a Diagnostic line is in. The Agent's stderr stays verbatim; all
/// other variants are the transport's account of the Connection.
fn diagnostic_source(kind: &DiagnosticKind) -> &'static str {
    match kind {
        DiagnosticKind::Stderr(_) => "Agent stderr",
        _ => "Transport",
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Arc;
    use std::time::SystemTime;

    use acp_inspector_core::{Direction, Frame};

    #[test]
    fn newest_line_and_latest_return_from_an_older_page() {
        fn host() -> Element {
            let lines = use_signal(|| {
                (0..1000)
                    .map(|id| Diagnostic {
                        at: SystemTime::UNIX_EPOCH,
                        kind: DiagnosticKind::Stderr(format!("diagnostic-{id}")),
                    })
                    .collect::<Vec<_>>()
            });
            rsx! { Diagnostics { lines, dropped: 0, filter: String::new() } }
        }
        let mut dom = VirtualDom::new(host);
        let edits = dom.rebuild_to_vec().edits;
        let older = crate::page::tests::labelled(&edits, "Older evidence page");
        for label in ["Newest line", "Latest lines page"] {
            let latest = crate::page::tests::labelled(&edits, label);
            crate::page::tests::click(&mut dom, older);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains("601–800 of 1000 retained rows"));
            assert!(html.contains("diagnostic-799") && !html.contains("diagnostic-999"));
            crate::page::tests::click(&mut dom, latest);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains("801–1000 of 1000 retained rows"));
            assert!(html.contains("diagnostic-999") && !html.contains("diagnostic-799"));
        }
    }

    #[test]
    fn multi_megabyte_diagnostic_is_a_bounded_preview_and_explicit_byte_reader() {
        use dioxus::core::{AttributeValue, Mutation};
        use dioxus::html::{PlatformEventData, SerializedMouseData};
        use std::{any::Any, rc::Rc};
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        fn host() -> Element {
            let lines = use_signal(|| {
                vec![Diagnostic {
                    at: SystemTime::UNIX_EPOCH,
                    kind: DiagnosticKind::Stderr(format!(
                        "{}🦀{}END",
                        "x".repeat(16383),
                        "y".repeat(7 * 1024 * 1024)
                    )),
                }]
            });
            rsx! { Diagnostics { lines, dropped: 0, filter: String::new() } }
        }
        let mut dom = VirtualDom::new(host);
        let edits = dom.rebuild_to_vec().edits;
        let target = |edits: &[Mutation], prefix: &str| {
            edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(text),
                        id,
                        ..
                    } if text.starts_with(prefix) => Some(*id),
                    _ => None,
                })
                .unwrap()
        };
        let click = |dom: &mut VirtualDom, id| {
            let data = Rc::new(PlatformEventData::new(Box::new(
                SerializedMouseData::default(),
            ))) as Rc<dyn Any>;
            dom.runtime()
                .handle_event("click", Event::new(data, true), id);
            dom.render_immediate_to_vec().edits
        };
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.len() < 5000,
            "closed row must not place megabytes in the DOM"
        );
        assert!(html.contains("preview;") && html.contains("Copy this line"));
        assert!(!html.contains("literal raw UTF-8"));
        let edits = click(&mut dom, target(&edits, "Read complete diagnostic"));
        let html = dioxus_ssr::render(&dom);
        assert!(html.len() < 22000);
        assert!(html.contains("Bytes 0–16383"));
        assert!(html.contains("Raw diagnostic byte windows"));
        click(&mut dom, target(&edits, "Next raw diagnostic bytes"));
        let html = dioxus_ssr::render(&dom);
        assert!(html.len() < 22000);
        assert!(html.contains("Bytes 16383–32768"));
        assert!(
            html.contains("🦀yyyy"),
            "window boundary must preserve UTF-8"
        );
        assert!(html.contains("Copy this line"));
    }

    use super::*;

    /// One line on the diagnostic channel, in one of the two tones the console
    /// draws: the agent talking, and the connection's bad news.
    #[derive(Clone, PartialEq)]
    enum Said {
        Ordinary,
        Bad,
    }

    fn diagnostic(said: &Said) -> Diagnostic {
        Diagnostic {
            at: SystemTime::UNIX_EPOCH,
            kind: match said {
                Said::Ordinary => DiagnosticKind::Stderr("listening".to_owned()),
                Said::Bad => {
                    DiagnosticKind::TransportFailed(Arc::new(io::Error::other("the pipe broke")))
                }
            },
        }
    }

    /// One frame. Which frame it is matters to nothing below: what the Console
    /// says about the trace is how many of them there are and what shapes they
    /// were in.
    fn frame(json: &str) -> TracedFrame {
        TracedFrame {
            at: SystemTime::UNIX_EPOCH,
            connection: 1,
            direction: Direction::ToAgent,
            frame: Frame::new(json),
        }
    }

    /// What the Console is being asked to show: which tab, and what each of the
    /// two surfaces holds.
    #[derive(Clone, PartialEq, Default)]
    struct Screen {
        tab: Tab,
        /// The frames, by what each one said — which is how a test asks for a
        /// shape without carrying a value that cannot be compared.
        frames: Vec<String>,
        dropped: usize,
        said: Vec<Said>,
    }

    impl Screen {
        /// A trace with one frame of each of the four shapes the envelope has.
        fn traced(count: usize) -> Vec<String> {
            (0..count)
                .map(|nth| format!(r#"{{"jsonrpc":"2.0","id":{nth},"method":"initialize"}}"#))
                .collect()
        }
    }

    fn shown(screen: Screen) -> String {
        #[component]
        fn Host(screen: Screen) -> Element {
            let frames = use_signal(|| {
                screen
                    .frames
                    .iter()
                    .map(|json| frame(json))
                    .collect::<Vec<_>>()
            });
            let lines = use_signal(|| screen.said.iter().map(diagnostic).collect::<Vec<_>>());
            let revealed = use_signal(Vec::<u64>::new);
            let decoded = use_signal(HashSet::<u64>::new);
            rsx! {
                Console {
                    frames,
                    dropped_frames: screen.dropped,
                    saved: None,
                    on_export: move |()| {},
                    on_clear: move |()| {},
                    lines,
                    dropped_lines: 0,
                    tab: screen.tab,
                    on_select: move |_| {},
                    revealed,
                    decoded,
                    on_seek: move |_: u64| {},
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { screen });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// One control on the Console's bar, by the slot it names itself with.
    fn action(html: &str, slot: &str) -> Option<String> {
        html.split("<button")
            .skip(1)
            .filter_map(|rest| rest.split_once("</button>"))
            .map(|(button, _)| button.to_owned())
            .find(|button| button.contains(&format!(r#"data-slot="{slot}""#)))
    }

    #[test]
    fn a_console_opens_on_the_trace_and_both_panels_stay_addressable() {
        let html = shown(Screen {
            frames: Screen::traced(2),
            said: vec![Said::Ordinary],
            ..Screen::default()
        });
        assert!(
            html.contains(r#"id="console-tab-trace""#)
                && html.contains(r#"aria-controls="console-panel-trace""#)
                && html.contains(r#"id="console-panel-diagnostics""#),
            "each tab's `aria-controls` resolves whichever surface is showing: {html}"
        );
        assert!(
            html.contains(r#"data-slot="trace""#) && !html.contains(r#"data-slot="diagnostics""#),
            "and the surface a window opens on is the one with something to say about every agent: {html}"
        );
        // Tabs carry counts and nothing else — no unread badge, no activity
        // dot, no severity mark when a bad diagnostic line arrives.
        assert!(
            html.contains(r#"data-slot="count""#) && html.contains(">2</span>"),
            "the count is the fact: {html}"
        );
    }

    #[test]
    fn a_surface_that_captured_nothing_draws_no_count_and_no_way_to_narrow_it() {
        let html = shown(Screen::default());
        assert!(
            !html.contains(r#"data-slot="count""#),
            "a badge reading zero is a count of the times it has said it: {html}"
        );
        assert!(
            !html.contains(r#"class="console-filters""#),
            "and a filter over an empty surface is a control that can do nothing: {html}"
        );
        assert!(
            html.contains("exactly as it crossed the wire"),
            "what the surface will hold is what it says instead: {html}"
        );
    }

    #[test]
    fn what_a_surface_captured_and_no_longer_holds_is_said_beside_what_it_has() {
        let html = shown(Screen {
            frames: Screen::traced(3),
            dropped: 12,
            ..Screen::default()
        });
        assert!(
            html.contains(r#"data-slot="gone""#) && html.contains("12 dropped"),
            "a count that is not the whole session must never look like one: {html}"
        );
    }

    #[test]
    fn the_traces_verbs_are_on_the_console_bar_and_only_on_its_own_surface() {
        // One strip of chrome for one panel, and the two verbs of one surface:
        // the Diagnostic channel is neither exported nor cleared, and a control
        // that did nothing on the tab in front of you is worse than one that is
        // not there.
        let traced = shown(Screen {
            frames: Screen::traced(1),
            ..Screen::default()
        });
        let export = action(&traced, "export").expect("the export control");
        assert!(
            export.contains(r#"aria-label="Export the trace""#)
                && export.contains("nothing redacted")
                && !export.contains("aria-disabled"),
            "a trace with frames in it can be taken away: {export}"
        );
        let clear = action(&traced, "clear").expect("the clear control");
        assert!(
            clear.contains("exports already written are untouched"),
            "clearing says the one thing about it that surprises people: {clear}"
        );

        // Unavailable rather than absent while there is nothing to act on, and
        // ARIA-disabled rather than natively so, which is the rule every other
        // guarded control here follows.
        let empty = shown(Screen::default());
        let export = action(&empty, "export").expect("the export control stays in the document");
        assert!(
            export.contains(r#"aria-disabled="true""#) && export.contains(r#"tabindex="-1""#),
            "an empty trace has nothing to export: {export}"
        );

        let diagnostics = shown(Screen {
            tab: Tab::Diagnostics,
            frames: Screen::traced(1),
            said: vec![Said::Ordinary],
            ..Screen::default()
        });
        assert!(
            action(&diagnostics, "export").is_none() && action(&diagnostics, "clear").is_none(),
            "neither verb is drawn on the surface it cannot act on: {diagnostics}"
        );
    }

    #[test]
    fn the_shapes_a_reader_can_hide_are_chips_and_the_fifth_is_drawn_only_when_it_can_hide_something()
     {
        let ordinary = shown(Screen {
            frames: Screen::traced(1),
            ..Screen::default()
        });
        for kind in ["request", "response", "notif", "error"] {
            assert!(
                ordinary.contains(&format!(r#"data-kind="{kind}""#)),
                "{kind} is one of the envelope's four shapes: {ordinary}"
            );
        }
        assert!(
            !ordinary.contains(r#"data-kind="other""#),
            "a chip that can hide nothing is chrome: {ordinary}"
        );
        let unreadable = shown(Screen {
            frames: vec!["not json at all".to_owned()],
            ..Screen::default()
        });
        assert!(
            unreadable.contains(r#"data-kind="other""#),
            "and traffic in none of the four shapes has one, so nothing can be hidden with no way back: {unreadable}"
        );
        // Every chip starts pressed: a surface that opened narrowed would be
        // hiding traffic nobody asked it to hide.
        assert_eq!(
            ordinary.matches(r#"aria-pressed=true"#).count(),
            6,
            "a surface opens showing everything: {ordinary}"
        );
    }

    #[test]
    fn the_filter_narrows_the_surface_in_front_of_the_reader_and_nothing_else() {
        // One box for both surfaces: they are two views of one connection
        // behind two tabs in one panel, and a reader who narrows to a session id
        // and switches tabs is asking the same question of the other account of
        // it.
        let html = shown(Screen {
            tab: Tab::Diagnostics,
            said: vec![Said::Ordinary],
            ..Screen::default()
        });
        let filter = html
            .split("<input")
            .nth(1)
            .and_then(|rest| rest.split_once('>'))
            .map(|(field, _)| field.to_owned())
            .expect("the filter is on the bar");
        assert!(
            filter.contains(r#"data-slot="filter""#)
                && filter.contains(r#"type="search""#)
                && filter.contains(r#"aria-label="Filter the console""#),
            "the filter is one accessibly named field on the panel's own bar: {filter}"
        );

        // Presentation and nothing else, which is what `matching` is: an empty
        // filter matches everything, and what is typed is found by containing
        // it rather than by a language to get wrong.
        assert!(matching("session/prompt", ""));
        assert!(matching("session/prompt", "PROMPT"));
        assert!(!matching("session/prompt", "cancel"));
    }

    #[test]
    fn folded_matching_needs_no_lowercased_copy_and_agrees_with_one() {
        for (text, needle) in [
            (r#"{"method":"Session/Prompt"}"#, "session/prompt"),
            ("ÉCOLE élève", "élève"),
            ("abc", ""),
        ] {
            assert!(contains_folded(text, needle), "{text} / {needle}");
            assert_eq!(
                contains_folded(text, needle),
                text.to_lowercase().contains(needle)
            );
        }
        assert!(!contains_folded("short", "much longer needle"));
    }

    #[tokio::test]
    async fn clearing_the_trace_takes_a_second_press() {
        use std::{any::Any, rc::Rc};

        use dioxus::core::{AttributeValue, Mutation, NoOpMutations};
        use dioxus::html::{PlatformEventData, SerializedMouseData};

        #[component]
        fn Host() -> Element {
            let mut cleared = use_signal(|| 0);
            rsx! {
                ClearTrace { nothing_traced: false, on_clear: move |()| cleared += 1 }
                span { "data-slot": "cleared", "{cleared}" }
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
        let clear = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(text),
                    id,
                    ..
                } if text == "Clear the trace" => Some(*id),
                _ => None,
            })
            .expect("the clear control");
        dom.runtime().handle_event("click", click(), clear);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("Confirm clear")
                && html.contains(r#"data-slot="cancel-clear""#)
                && html.contains(r#"data-slot="cleared">0"#),
            "{html}"
        );
        dom.runtime().handle_event("click", click(), clear);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="cleared">1"#) && !html.contains("Confirm clear"),
            "{html}"
        );
    }

    #[test]
    fn a_narrowed_surface_says_what_it_is_hiding() {
        // Without this line a filter that matched nothing would be
        // indistinguishable from an agent that had gone quiet, on the one panel
        // whose job is to say what crossed the wire.
        let none = dioxus_ssr::render_element(narrowed(0, 12, "frames", true));
        assert!(
            none.contains("No frames match")
                && none.contains("still captured, exported and counted"),
            "{none}"
        );
        let some = dioxus_ssr::render_element(narrowed(3, 12, "lines", true));
        assert!(some.contains("3 of 12 lines"), "{some}");
        // And a surface nobody narrowed says nothing at all.
        assert!(dioxus_ssr::render_element(narrowed(12, 12, "frames", false)).is_empty());
        assert!(dioxus_ssr::render_element(narrowed(0, 0, "frames", true)).is_empty());
    }

    #[test]
    fn the_diagnostic_channel_carries_both_voices_in_one_order() {
        let html = shown(Screen {
            tab: Tab::Diagnostics,
            said: vec![Said::Ordinary, Said::Bad],
            ..Screen::default()
        });
        assert!(
            html.contains("Agent stderr: ") && html.contains("Transport: "),
            "each row says whose voice it carries where a screen reader hears it: {html}"
        );
        assert!(
            html.contains("listening") && html.contains("the pipe broke"),
            "the agent's account and the transport's are read against each other: {html}"
        );
        assert!(
            html.contains(r#"class="line agent"#) && html.contains(r#"class="line bad"#),
            "and the tone repeats what the row already says: {html}"
        );
        // The agent's own words about why it would not start are the thing a
        // reader pastes into an issue.
        assert_eq!(
            html.matches(r#"aria-label="Copy this line""#).count(),
            2,
            "every line can be taken away: {html}"
        );
    }

    #[test]
    fn the_transports_remarks_about_the_record_are_said_in_plain_words() {
        let said_of = |kind: DiagnosticKind| said(&kind);
        assert_eq!(
            said_of(DiagnosticKind::FrameDropped {
                limit: 10 * 1024 * 1024,
                length: 20 * 1024 * 1024,
            }),
            "The agent sent a frame of 20.0 MiB, over the 10.0 MiB limit. It was dropped; a placeholder marks where it was in the trace."
        );
        assert!(
            said_of(DiagnosticKind::StderrLineCut {
                limit: 1024 * 1024,
                length: 3 * 1024 * 1024,
            })
            .contains("only its first 1.0 MiB of 3.0 MiB are kept")
        );
        assert!(said_of(DiagnosticKind::StderrLineLossy).contains("wasn't valid UTF-8"));
        assert_eq!(
            said_of(DiagnosticKind::Stopped {
                by: Stop::StdinClosed,
                status: None,
            }),
            "The agent stopped when its input was closed."
        );
        assert!(
            said_of(DiagnosticKind::Stopped {
                by: Stop::Killed,
                status: None,
            })
            .contains("was killed (SIGKILL)")
        );
        assert!(said_of(DiagnosticKind::DrainAbandoned).contains("another process"));
        assert_eq!(
            said_of(DiagnosticKind::Stderr("verbatim".into())),
            "verbatim"
        );
        assert!(matches!(
            tone(&DiagnosticKind::FrameDropped {
                limit: 1,
                length: 2
            }),
            Tone::Bad
        ));
    }

    #[test]
    fn an_automatic_surface_change_is_a_connection_state_and_never_a_volume() {
        // The MCP Inspector flow worth copying verbatim: an agent that is gone
        // without being asked to go puts its own reason on screen. Two states
        // mean that, and core is where they are told apart.
        for status in [ConnectionStatus::FailedToStart, ConnectionStatus::Lost] {
            assert!(surfaces_diagnostics(status));
            assert!(diagnostics_announcement(status).is_some());
        }
        for status in [ConnectionStatus::Connected, ConnectionStatus::Disconnected] {
            assert!(
                !surfaces_diagnostics(status),
                "{status:?} takes no surface the reader may have chosen"
            );
        }
    }

    #[test]
    fn the_keyboard_moves_between_the_two_tabs_and_stops_at_the_ends() {
        for (from, key, to) in [
            (Tab::Trace, Key::ArrowRight, Some(Tab::Diagnostics)),
            (Tab::Diagnostics, Key::ArrowLeft, Some(Tab::Trace)),
            (Tab::Diagnostics, Key::Home, Some(Tab::Trace)),
            (Tab::Trace, Key::End, Some(Tab::Diagnostics)),
            (Tab::Trace, Key::Enter, None),
        ] {
            assert_eq!(tab_for_key(from, &key), to, "{from:?} + {key:?}");
        }
    }
}
