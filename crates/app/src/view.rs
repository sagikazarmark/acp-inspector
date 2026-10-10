//! A trace opened from a file: the window, read-only, showing an export instead
//! of a connection.
//!
//! Core reads and replays the file (`acp_inspector_core::Imported`) into the
//! same kinds of values the live stores hold, and [`View`] answers the
//! window's sources in their place while it is open. A trace opens only with
//! nothing connected, and launching closes it: a file's frames never mix into
//! a live trace.

use std::path::PathBuf;
use std::time::SystemTime;

use acp_inspector_core::{
    ConnectionStatus, Diagnostic, Imported, Retention, TimelineEntry, Trace, TracedFrame, Turn,
};
use dioxus::prelude::*;

/// What is open, and what the file said about itself.
pub struct Opened {
    pub path: PathBuf,
    pub trace: Trace,
    /// The ordinal the file's frames are numbered from.
    pub dropped: usize,
    /// The agent's own name, where the file holds its `initialize` answer.
    pub agent: Option<String>,
    pub taken: SystemTime,
    pub warnings: Vec<String>,
}

impl Opened {
    /// The file's name, which is what the banner calls it.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

/// The opened trace, as the window's sources. Copy: every field is a signal.
#[derive(Clone, Copy, PartialEq)]
pub struct View {
    opened: Signal<Option<Opened>>,
    frames: Signal<Vec<TracedFrame>>,
    lines: Signal<Vec<Diagnostic>>,
    entries: Signal<Vec<TimelineEntry>>,
    turns: Signal<Vec<Turn>>,
    opening: Signal<bool>,
    problem: Signal<Option<String>>,
}

/// The view a window has, closed whenever an agent connects.
pub fn use_view(status: ReadSignal<ConnectionStatus>) -> View {
    let view = use_hook(View::new);
    use_effect(move || {
        if status() == ConnectionStatus::Connected && view.opened.peek().is_some() {
            view.close();
        }
    });
    view
}

impl View {
    /// Nothing open. Signals of the scope it is made in.
    pub fn new() -> Self {
        Self {
            opened: Signal::new(None),
            frames: Signal::new(Vec::new()),
            lines: Signal::new(Vec::new()),
            entries: Signal::new(Vec::new()),
            turns: Signal::new(Vec::new()),
            opening: Signal::new(false),
            problem: Signal::new(None),
        }
    }

    /// Whether a file is what the window is showing.
    pub fn is_open(&self) -> bool {
        self.opened.read().is_some()
    }

    /// The opened trace's own handle, for pairing and for exporting it again.
    pub fn trace(&self) -> Option<Trace> {
        self.opened
            .read()
            .as_ref()
            .map(|opened| opened.trace.clone())
    }

    /// The opened file's stderr lines, for exporting it again.
    pub fn stderr(&self) -> Vec<Diagnostic> {
        self.lines.peek().clone()
    }

    /// The agent the opened trace is of, by name, where the file says.
    pub fn agent(&self) -> Option<String> {
        self.opened
            .peek()
            .as_ref()
            .and_then(|opened| opened.agent.clone())
    }

    /// The frames to draw: the file's while one is open, else the live ones.
    pub fn frames_or(&self, live: Signal<Vec<TracedFrame>>) -> ReadSignal<Vec<TracedFrame>> {
        if self.is_open() {
            self.frames.into()
        } else {
            live.into()
        }
    }

    pub fn dropped_or(&self, live: usize) -> usize {
        self.opened
            .read()
            .as_ref()
            .map_or(live, |opened| opened.dropped)
    }

    pub fn lines_or(&self, live: Signal<Vec<Diagnostic>>) -> ReadSignal<Vec<Diagnostic>> {
        if self.is_open() {
            self.lines.into()
        } else {
            live.into()
        }
    }

    pub fn dropped_lines_or(&self, live: usize) -> usize {
        if self.is_open() { 0 } else { live }
    }

    pub fn entries_or(&self, live: Signal<Vec<TimelineEntry>>) -> ReadSignal<Vec<TimelineEntry>> {
        if self.is_open() {
            self.entries.into()
        } else {
            live.into()
        }
    }

    pub fn turns_or(&self, live: Vec<Turn>) -> Vec<Turn> {
        if self.is_open() {
            self.turns.read().clone()
        } else {
            live
        }
    }

    pub fn retention_or(&self, live: Retention) -> Retention {
        if self.is_open() {
            Retention::default()
        } else {
            live
        }
    }

    /// The ordinals the shown trace holds, for the Timeline's way back to it.
    pub fn held_or(&self, live: std::ops::Range<u64>) -> std::ops::Range<u64> {
        match self.opened.read().as_ref() {
            Some(opened) => {
                let start = opened.dropped as u64;
                start..start + self.frames.read().len() as u64
            }
            None => live,
        }
    }

    /// A live value, or nothing while a file is shown (the session it would
    /// name is not the file's).
    pub fn live<T>(&self, live: Option<T>) -> Option<T> {
        if self.is_open() { None } else { live }
    }

    /// Asks for a trace file and opens it — refused, with the reason, while an
    /// agent is connected.
    pub fn choose(mut self, connected: bool) {
        if connected {
            self.problem.set(Some(
                "Disconnect the agent before opening a trace.".to_owned(),
            ));
            return;
        }
        if let Some(path) = choose_file() {
            self.open(path);
        }
    }

    /// Reads and replays `path` off the UI thread, then shows it.
    pub fn open(mut self, path: PathBuf) {
        if *self.opening.peek() {
            return;
        }
        self.opening.set(true);
        self.problem.set(None);
        spawn(async move {
            let reading = path.clone();
            let read = tokio::task::spawn_blocking(move || Imported::open(reading))
                .await
                .map_err(|error| error.to_string())
                .and_then(|read| read.map_err(|error| error.to_string()));
            self.opening.set(false);
            match read {
                Ok(imported) => self.show(path, &imported),
                Err(problem) => self.problem.set(Some(format!(
                    "Could not open {}: {problem}",
                    path.display()
                ))),
            }
        });
    }

    /// Shows what was read.
    pub fn show(mut self, path: PathBuf, imported: &Imported) {
        let (frames, dropped) = imported.trace().snapshot();
        let (entries, turns, _) = imported.timeline().snapshot();
        self.frames.set(frames);
        self.lines.set(imported.stderr().to_vec());
        self.entries.set(entries);
        self.turns.set(turns);
        self.opened.set(Some(Opened {
            path,
            trace: imported.trace().clone(),
            dropped,
            agent: imported
                .agent()
                .and_then(|agent| agent.agent_info.as_ref())
                .map(|info| info.name.to_string()),
            taken: imported.taken(),
            warnings: imported.warnings().to_vec(),
        }));
    }

    /// Back to the live window.
    pub fn close(mut self) {
        self.opened.set(None);
        self.frames.set(Vec::new());
        self.lines.set(Vec::new());
        self.entries.set(Vec::new());
        self.turns.set(Vec::new());
    }
}

/// The platform's open dialog, where there is one.
#[cfg(feature = "desktop")]
fn choose_file() -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Open trace")
        .add_filter("ACP Inspector trace", &["jsonl"])
        .add_filter("All files", &["*"]);
    if let Some(downloads) = dirs::download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    dialog.pick_file()
}

#[cfg(not(feature = "desktop"))]
fn choose_file() -> Option<PathBuf> {
    None
}

/// Whether this build can ask for a file at all.
pub const CAN_OPEN: bool = cfg!(feature = "desktop");

/// What the window is showing when it is not a connection, and the way back.
#[component]
pub fn Banner(view: View) -> Element {
    let mut problem = view.problem;
    let opening = (view.opening)();
    let opened = view.opened.read();
    rsx! {
        if opening {
            div { class: "view-banner", "data-slot": "view-banner", role: "status",
                "Opening trace…"
            }
        }
        if let Some(said) = problem() {
            div { class: "view-banner bad", "data-slot": "view-problem", role: "alert",
                span { "{said}" }
                button {
                    class: "btn btn-xs btn-quiet",
                    r#type: "button",
                    "data-slot": "dismiss",
                    aria_label: "Dismiss",
                    onclick: move |_| problem.set(None),
                    "Dismiss"
                }
            }
        }
        if let Some(opened) = opened.as_ref() {
            div { class: "view-banner", "data-slot": "view-banner", role: "status",
                span { class: "view-title",
                    "Viewing "
                    strong { "{opened.name()}" }
                }
                span { class: "hint",
                    " · read-only · {(view.frames)().len()} frames"
                    if opened.dropped > 0 { " · {opened.dropped} not in the file" }
                    " · exported {crate::time::stamp(opened.taken)}"
                }
                for warning in opened.warnings.iter() {
                    p { class: "hint", "data-slot": "view-warning", "{warning}" }
                }
                button {
                    class: "btn btn-xs btn-quiet",
                    r#type: "button",
                    "data-slot": "close-view",
                    title: "Stop viewing the file and go back to the live window",
                    onclick: move |_| view.close(),
                    "Close"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = concat!(
        r#"{"type":"header","format":"acp-inspector-trace","version":1,"at":0,"frames":2,"dropped":1,"warning":"w"}"#,
        "\n",
        r#"{"type":"frame","at":1,"connection":1,"direction":"client -> agent","encoding":"utf8","frame":"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}"}"#,
        "\n",
        r#"{"type":"frame","at":2,"connection":1,"direction":"agent -> client","encoding":"utf8","frame":"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"filed\",\"version\":\"1\"}}}"}"#,
        "\n",
    );

    #[test]
    fn an_opened_trace_answers_the_windows_sources_until_it_is_closed() {
        fn host() -> Element {
            let live = use_signal(Vec::<TracedFrame>::new);
            let view = use_hook(|| {
                let view = View::new();
                view.show(
                    PathBuf::from("/tmp/a trace.jsonl"),
                    &Imported::parse(FILE).unwrap(),
                );
                view
            });
            let frames = view.frames_or(live);
            rsx! {
                Banner { view }
                span { "data-slot": "count", "{frames.read().len()}" }
                span { "data-slot": "dropped", "{view.dropped_or(7)}" }
                span { "data-slot": "held", "{view.held_or(0..0):?}" }
                span { "data-slot": "agent", "{view.agent().unwrap_or_default()}" }
            }
        }
        let mut dom = VirtualDom::new(host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("Viewing <strong>a trace.jsonl</strong>"),
            "{html}"
        );
        assert!(
            html.contains("read-only · 2 frames · 1 not in the file"),
            "{html}"
        );
        assert!(html.contains(r#"data-slot="count">2<"#), "{html}");
        assert!(html.contains(r#"data-slot="dropped">1<"#), "{html}");
        assert!(html.contains(r#"data-slot="held">1..3<"#), "{html}");
        assert!(html.contains(r#"data-slot="agent">filed<"#), "{html}");
        assert!(html.contains(r#"data-slot="close-view""#), "{html}");
    }
}
