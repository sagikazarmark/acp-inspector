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
use std::path::PathBuf;

use acp_inspector_core::{ConnectionStatus, Diagnostic, DiagnosticKind, FrameKind, TracedFrame};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::hi_outline_icons::{HiDownload, HiTrash},
};

use crate::copy::Copy;
use crate::time::stamp;
use crate::trace::{Kinds, TraceView, named_kind};

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
    saved: Option<Result<PathBuf, String>>,
    on_export: EventHandler<()>,
    on_clear: EventHandler<()>,
    lines: ReadSignal<Vec<Diagnostic>>,
    /// How many lines the diagnostic log captured and no longer holds — `0`
    /// while it is unbounded, and half of what a row's key is made of either
    /// way.
    dropped_lines: usize,
    tab: Tab,
    on_select: EventHandler<Tab>,
    /// The frames a Timeline entry was made of, sent here by somebody reading
    /// that entry — and which of the trace's frames became one, so a row can
    /// offer the way back.
    revealed: ReadSignal<Vec<u64>>,
    decoded: ReadSignal<HashSet<u64>>,
    on_seek: EventHandler<u64>,
) -> Element {
    // Whether there is anything to export or clear. Read here rather than in
    // the Trace because the controls are here, and it is the same question the
    // tab's own count answers.
    let nothing_traced = frames().is_empty();
    // And whether the surface in front of the reader has captured anything at
    // all, which is what decides whether there is anything to narrow.
    let empty = match tab {
        Tab::Trace => nothing_traced,
        Tab::Diagnostics => lines().is_empty(),
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
            selected.set(Some(first));
        }
    });

    // Whether any frame is in none of the envelope's four shapes, which is what
    // decides whether the fifth chip is worth drawing: a chip that can hide
    // nothing is chrome, and one missing for traffic that exists would be a
    // surface able to hide frames with nothing to press to bring them back.
    let unreadable = frames().iter().any(|traced| {
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
                        held: frames().len(),
                        gone: dropped_frames,
                        on_select,
                    }
                    TabControl {
                        tab: Tab::Diagnostics,
                        selected: tab,
                        held: lines().len(),
                        // `0` for as long as the diagnostic log is unbounded,
                        // so nothing is drawn — passed the way the trace's is
                        // so that the day it is bounded, the strip already
                        // says so.
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
                    button {
                        class: "btn btn-xs btn-quiet",
                        "data-slot": "export",
                        r#type: "button",
                        aria_label: "Export the trace",
                        aria_disabled: nothing_traced.then_some("true"),
                        tabindex: nothing_traced.then_some("-1"),
                        title: "Write every frame to a JSONL file, nothing redacted",
                        onclick: move |_| {
                            if !nothing_traced {
                                on_export.call(());
                            }
                        },
                        Icon { class: "icon", icon: HiDownload }
                        "export"
                    }
                    button {
                        class: "btn btn-xs btn-quiet btn-bad",
                        "data-slot": "clear",
                        r#type: "button",
                        aria_label: "Clear the trace",
                        aria_disabled: nothing_traced.then_some("true"),
                        tabindex: nothing_traced.then_some("-1"),
                        // Said on the control, because it is the one thing
                        // about clearing that surprises people: a file already
                        // written is not this trace's any more.
                        title: "Drop the frames captured so far; exports already written are untouched",
                        onclick: move |_| {
                            if !nothing_traced {
                                on_clear.call(());
                            }
                        },
                        Icon { class: "icon", icon: HiTrash }
                        "clear"
                    }
                }
            }

            // Only where there is something to narrow. A surface that has
            // captured nothing says so in its own empty state, and a filter box
            // over it is a control that can do nothing.
            if !empty {
                div { class: "wire-filters",
                    if tab == Tab::Trace {
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
                    }
                    input {
                        class: "input",
                        "data-slot": "filter",
                        r#type: "search",
                        value: "{filter}",
                        spellcheck: false,
                        aria_label: "Filter the console",
                        title: "Show only rows containing this text",
                        placeholder: "filter method…",
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
                    TraceView {
                        frames,
                        dropped: dropped_frames,
                        saved,
                        filter: filter(),
                        kinds: kinds(),
                        revealed,
                        decoded,
                        selected: selected(),
                        on_choose: move |ordinal| selected.set(Some(ordinal)),
                        on_seek,
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
                    title: "captured, and no longer held — cleared, or aged out of the surface's cap",
                    "−{gone}"
                }
            }
        }
    }
}

fn tab_for_key(selected: Tab, key: &Key) -> Option<Tab> {
    match key {
        Key::ArrowLeft | Key::ArrowRight => Some(match selected {
            Tab::Trace => Tab::Diagnostics,
            Tab::Diagnostics => Tab::Trace,
        }),
        Key::Home => Some(Tab::Trace),
        Key::End => Some(Tab::Diagnostics),
        _ => None,
    }
}

fn focus_tab(tab: Tab) {
    // The id is one of the two literals above, never content from the Agent.
    let _ = document::eval(&format!(
        "document.getElementById({:?})?.focus();",
        tab.id()
    ));
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
    filter.is_empty() || text.to_lowercase().contains(&filter.to_lowercase())
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

    rsx! {
        p { class: "narrowed", "data-slot": "narrowed", role: "status", aria_live: "polite",
            if shown == 0 {
                "No {what} match. "
            } else {
                "{shown} of {held} {what}. "
            }
            span { class: "hint", "The whole surface is still captured, exported and counted." }
        }
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
    let held = lines().len();
    // The line as it is *rendered*, which is what the reader is looking at and
    // therefore what they are searching: the transport's own remarks are part
    // of this channel, and a filter that only read the agent's stderr would
    // hide the line saying the agent never started.
    let shown = lines()
        .iter()
        .filter(|diagnostic| matching(&diagnostic.to_string(), &filter))
        .count();

    rsx! {
        {narrowed(shown, held, "lines", !filter.trim().is_empty())}

        if held == 0 {
            p { class: "empty",
                "Everything the transport said about this connection that was not a frame."
            }
        } else {
            ul { class: "lines", "data-slot": "diagnostics", "data-tail": LINES,
                // Newest first in the DOM, oldest first on screen: the list is
                // laid out bottom-up, which is what keeps a streaming console
                // pinned to its latest line without a scroll script.
                //
                // Keyed by the ordinal the line was captured under, the way the
                // trace's rows are: `dropped` is `0` while this log is
                // unbounded, and the sum is what keeps that a fact about today
                // rather than something the rows depend on.
                for (position, diagnostic) in lines()
                    .into_iter()
                    .enumerate()
                    .rev()
                    .filter(|(_, diagnostic)| matching(&diagnostic.to_string(), &filter))
                {
                    li { key: "{dropped + position}", class: "line {tone(&diagnostic.kind).line_class()}",
                        span { class: "frame-at", "{stamp(diagnostic.at)}" }
                        span { class: "dot dot-{tone(&diagnostic.kind).dot()}", aria_hidden: "true" }
                        span { class: "sr-only", "{diagnostic_source(&diagnostic.kind)}: " }
                        span { class: "line-text", "{diagnostic}" }
                        // The agent's own words about why it would not start are
                        // the thing a reader pastes into an issue, and this is
                        // the surface they are on (§9).
                        Copy { text: diagnostic.to_string(), what: "this line" }
                    }
                }
            }
            {crate::tail::to_latest(LINES, "line")}
        }
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

fn tone(kind: &DiagnosticKind) -> Tone {
    match kind {
        DiagnosticKind::Stderr(_) => Tone::Agent,
        DiagnosticKind::SpawnFailed { .. } | DiagnosticKind::TransportFailed(_) => Tone::Bad,
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
            html.contains(r#"data-slot="gone""#) && html.contains("−12"),
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
            4,
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
