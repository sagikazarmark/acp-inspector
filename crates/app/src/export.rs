//! One export job for toolbar, palette and native menu. Snapshot synchronously;
//! serialize and write on the runtime's blocking pool.
use acp_inspector_core::{Export, Trace};
use dioxus::prelude::*;
use std::path::PathBuf;
use std::{future::Future, pin::Pin};

pub type RetrievalFuture = Pin<Box<dyn Future<Output = Result<(), String>>>>;

/// Platform operations supplied by the shell. Without a desktop only Copy is
/// available. The view owns pending/feedback; operations report actual outcomes.
#[derive(Clone, Copy)]
pub struct Retrieval {
    pub copy: fn(String) -> RetrievalFuture,
    pub open_folder: Option<fn(PathBuf) -> RetrievalFuture>,
}

impl Default for Retrieval {
    fn default() -> Self {
        Self {
            copy: |text| {
                Box::pin(async move {
                    if crate::copy::copied(text).await {
                        Ok(())
                    } else {
                        Err(
                            "Could not copy export path. Select and copy the path above manually."
                                .into(),
                        )
                    }
                })
            },
            open_folder: None,
        }
    }
}

/// Where an export went, and whether that is somewhere the system may clear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// Written to the temp directory because there was no dialog to ask with.
    pub temporary: bool,
}

/// One finished export, numbered so a notice dismissed for one is not
/// dismissed for the next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub id: u64,
    pub written: Result<Written, String>,
}

/// What an export said when it finished: where it went, or why it did not,
/// with a way to put the notice away.
#[component]
pub fn Notice(outcome: Outcome, on_dismiss: EventHandler<()>) -> Element {
    match outcome.written {
        Ok(written) => rsx! {
            Saved { key: "{written.path.display()}", path: written.path, temporary: written.temporary, on_dismiss }
        },
        Err(problem) => rsx! {
            div { class: "saved bad", "data-slot": "export-failed",
                p { role: "status", "The export was not written: {problem}" }
                {dismissal(on_dismiss)}
            }
        },
    }
}

fn dismissal(on_dismiss: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "btn btn-xs btn-quiet",
            r#type: "button",
            "data-slot": "dismiss-export",
            aria_label: "Dismiss the export notice",
            onclick: move |_| on_dismiss.call(()),
            "Dismiss"
        }
    }
}

#[component]
pub fn Saved(
    path: PathBuf,
    #[props(default)] temporary: bool,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    let dismiss = on_dismiss.map(dismissal);
    let retrieval = try_consume_context::<Retrieval>().unwrap_or_default();
    let copy_path = path.clone();
    rsx! {
        div { class: "saved", "data-slot": "export-retrieval",
            if temporary {
                p { role: "status", "Exported to temporary storage: "
                    code { class: "mono", "{path.display()}" }
                }
                p { class: "hint", "Copy or move this file elsewhere to keep it; the system may remove temporary files." }
            } else {
                p { role: "status", "Saved the trace to "
                    code { class: "mono", "{path.display()}" }
                }
            }
            if let Some(dismiss) = dismiss {
                {dismiss}
            }
            RetrievalAction {
                label: "Copy export path",
                pending_text: "Copying export path…",
                success: "Copied export path.",
                run: move |_| match copy_path.to_str() {
                    Some(text) => (retrieval.copy)(text.to_owned()),
                    None => Box::pin(async { Err("This path cannot be represented as clipboard text. Use Open containing folder.".into()) }) as RetrievalFuture,
                },
            }
            if let Some(open) = retrieval.open_folder {
                RetrievalAction {
                    label: "Open containing folder",
                    pending_text: "Opening containing folder…",
                    success: "Folder open request sent to the desktop.",
                    run: move |_| open(path.clone()),
                }
            }
        }
    }
}

#[component]
fn RetrievalAction(
    label: String,
    pending_text: String,
    success: String,
    run: Callback<(), RetrievalFuture>,
) -> Element {
    let mut pending = use_signal(|| false);
    let mut feedback = use_signal(|| None::<Result<String, String>>);
    rsx! {
        button {
            class: "btn btn-xs btn-quiet",
            r#type: "button",
            aria_label: "{label}",
            aria_disabled: pending().then_some("true"),
            onclick: move |_| {
                if *pending.peek() { return; }
                pending.set(true);
                feedback.set(None);
                let future = run.call(());
                let success = success.clone();
                spawn(async move {
                    feedback.set(Some(future.await.map(|()| success)));
                    pending.set(false);
                });
            },
            "{label}"
        }
        p { role: "status", aria_live: "polite", aria_atomic: "true",
            if pending() { "{pending_text}" }
            else if let Some(result) = feedback() {
                match result {
                    Ok(message) => rsx! { "{message}" },
                    Err(problem) => rsx! { span { class: "bad", "{problem}" } },
                }
            }
        }
    }
}

#[derive(Default)]
pub struct Work {
    pub pending: bool,
    pub result: Option<Outcome>,
    /// How many frames had been recorded when the export in flight began.
    taking: usize,
    /// How many frames had been recorded when the last export that was
    /// written began: everything up to there is in a file.
    exported: usize,
    finished: u64,
}

impl Work {
    /// Takes a snapshot to export, unless one is already being written.
    /// `recorded` is how many frames the trace has ever recorded, dropped and
    /// cleared ones included, as of this snapshot.
    #[cfg(test)]
    pub fn begin(&mut self, trace: &Trace, recorded: usize) -> Option<Export> {
        if self.pending {
            return None;
        }
        let snapshot = trace.export();
        self.pending = true;
        self.result = None;
        self.taking = recorded;
        Some(snapshot)
    }

    /// What became of the export in flight; `None` when nobody chose where it
    /// goes, which writes nothing and says nothing.
    pub fn finish(&mut self, result: Option<Result<Written, String>>) {
        self.pending = false;
        let Some(result) = result else {
            return;
        };
        if result.is_ok() {
            self.exported = self.exported.max(self.taking);
        }
        self.finished += 1;
        self.result = Some(Outcome {
            id: self.finished,
            written: result,
        });
    }

    /// Whether frames recorded up to `recorded` include some no export has
    /// written (F9).
    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub fn unexported(&self, recorded: usize) -> bool {
        recorded > self.exported
    }
}

/// An activation's snapshot, and where it is going.
pub struct Snapshot {
    export: Export,
    /// The path chosen in the save dialog; `None` where there is no dialog.
    chosen: Option<PathBuf>,
}

/// Takes the snapshot an Export activation owns — the opened file's while one
/// is shown, else the live trace's — and asks where it goes.
///
/// The snapshot is taken first, so what is written is what had crossed when
/// Export was pressed, whatever crosses while the dialog is open. `None` when
/// an export is already being written or the dialog was cancelled.
pub fn start(
    mut work: Signal<Work>,
    live: &Trace,
    recorded: usize,
    wire: Option<crate::wire::Wire>,
) -> Option<Snapshot> {
    if work.peek().pending {
        return None;
    }
    let viewed = wire.and_then(|wire| wire.view.trace());
    let export = viewed.as_ref().unwrap_or(live).export();
    let export = match wire {
        Some(wire) if *wire.include_stderr.peek() => export.with_stderr(&wire.stderr()),
        _ => export,
    };
    let name = default_name(
        wire.and_then(|wire| wire.agent()).as_deref(),
        std::time::SystemTime::now(),
    );
    let chosen = if CAN_CHOOSE {
        Some(choose_destination(&name)?)
    } else {
        None
    };
    let mut work = work.write();
    work.pending = true;
    work.result = None;
    // An opened file exported again is not the live trace exported.
    work.taking = if viewed.is_some() {
        work.exported
    } else {
        recorded
    };
    Some(Snapshot { export, chosen })
}

/// `acp-trace-<agent>-<YYYYMMDD-HHMMSS>.jsonl`, in local time, with the agent's
/// name reduced to what is safe in a file name.
pub fn default_name(agent: Option<&str>, at: std::time::SystemTime) -> String {
    let agent: String = agent
        .unwrap_or("agent")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let agent = agent.trim_matches(|character| matches!(character, '-' | '.'));
    let agent = if agent.is_empty() { "agent" } else { agent };
    let stamp = jiff::Timestamp::try_from(at)
        .map(|moment| {
            moment
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y%m%d-%H%M%S")
                .to_string()
        })
        .unwrap_or_else(|_| "00000000-000000".to_owned());
    format!("acp-trace-{agent}-{stamp}.jsonl")
}

/// Whether this build has a save dialog to ask with.
const CAN_CHOOSE: bool = cfg!(feature = "desktop");

/// The platform's save dialog, starting in Downloads with the default name.
#[cfg(feature = "desktop")]
fn choose_destination(name: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Save trace as")
        .set_file_name(name)
        .add_filter("ACP Inspector trace", &["jsonl"]);
    if let Some(downloads) = dirs::download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    dialog.save_file()
}

#[cfg(not(feature = "desktop"))]
fn choose_destination(_name: &str) -> Option<PathBuf> {
    None
}

/// Writes a snapshot where it was asked to go, off the UI thread.
pub async fn write(snapshot: Snapshot) -> Option<Result<Written, String>> {
    let Snapshot { export, chosen } = snapshot;
    let temporary = chosen.is_none();
    let written = in_background(move || match chosen {
        Some(path) => export.save_to(path),
        None => export.save(),
    })
    .await
    .map(|path| Written { path, temporary });
    Some(written)
}

async fn in_background(
    save: impl FnOnce() -> std::io::Result<PathBuf> + Send + 'static,
) -> Result<PathBuf, String> {
    tokio::task::spawn_blocking(save)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Completion = tokio::sync::oneshot::Sender<Result<(), String>>;
    thread_local! {
        static CALLS: std::cell::RefCell<Vec<(String, Completion)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn operation(value: String) -> RetrievalFuture {
        let (send, receive) = tokio::sync::oneshot::channel();
        CALLS.with(|calls| calls.borrow_mut().push((value, send)));
        Box::pin(async { receive.await.unwrap() })
    }

    #[component]
    fn Host() -> Element {
        use_context_provider(|| Retrieval {
            copy: operation,
            open_folder: Some(|path| operation(format!("folder:{}", path.display()))),
        });
        rsx! { Saved { path: PathBuf::from("/tmp/export folder/trace '🦀'.jsonl"), temporary: true } }
    }

    fn click(dom: &mut VirtualDom, id: dioxus::core::ElementId) {
        use dioxus::html::{PlatformEventData, SerializedMouseData};
        dom.runtime().handle_event(
            "click",
            Event::new(
                std::rc::Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default()))
                    as std::rc::Rc<dyn std::any::Any>,
                true,
            ),
            id,
        );
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
    }

    async fn settle(dom: &mut VirtualDom, expected: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !dioxus_ssr::render(dom).contains(expected) {
                dom.wait_for_work().await;
                dom.render_immediate(&mut dioxus::core::NoOpMutations);
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn retrieval_clicks_pass_exact_path_deduplicate_and_show_failure_then_retry() {
        use dioxus::core::Mutation;
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        let button = |label: &str| {
            edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: dioxus::core::AttributeValue::Text(value),
                        id,
                        ..
                    } if value == label => Some(*id),
                    _ => None,
                })
                .expect("named retrieval button")
        };
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("temporary storage") && html.contains("system may remove"));
        for (button, expected, pending, success) in [
            (
                button("Copy export path"),
                "/tmp/export folder/trace '🦀'.jsonl",
                "Copying export path",
                "Copied export path.",
            ),
            (
                button("Open containing folder"),
                "folder:/tmp/export folder/trace '🦀'.jsonl",
                "Opening containing folder",
                "Folder open request sent to the desktop.",
            ),
        ] {
            click(&mut dom, button);
            click(&mut dom, button);
            let html = dioxus_ssr::render(&dom);
            assert!(
                html.contains(pending) && html.contains("aria-disabled=\"true\""),
                "{html}"
            );
            let finish = CALLS.with(|calls| {
                let mut calls = calls.borrow_mut();
                assert_eq!(calls.len(), 1, "duplicate activation is ignored");
                let (value, finish) = calls.pop().unwrap();
                assert_eq!(value, expected);
                finish
            });
            finish.send(Err("retrieval probe failed".into())).unwrap();
            settle(&mut dom, "retrieval probe failed").await;
            assert!(!dioxus_ssr::render(&dom).contains("aria-disabled=\"true\""));
            click(&mut dom, button);
            assert!(!dioxus_ssr::render(&dom).contains("retrieval probe failed"));
            CALLS.with(|calls| calls.borrow_mut().pop().unwrap().1.send(Ok(())).unwrap());
            settle(&mut dom, success).await;
        }
    }

    #[test]
    fn without_a_native_shell_only_copy_is_offered() {
        let html =
            dioxus_ssr::render_element(rsx! { Saved { path: PathBuf::from("/tmp/trace.jsonl") } });
        assert!(html.contains("Copy export path"));
        assert!(!html.contains("Open containing folder"));
    }

    #[tokio::test]
    async fn pending_job_does_not_block_executor_or_accept_duplicates_and_failure_can_retry() {
        let trace = Trace::default();
        let mut work = Work::default();
        let snapshot = work.begin(&trace, 3).unwrap();
        assert!(work.pending);
        assert!(work.begin(&trace, 3).is_none());
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(in_background(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
            assert_eq!(snapshot.frames(), 0);
            Err(std::io::Error::other("disk probe"))
        }));
        // A single-thread Tokio executor can reach this while the disk worker waits.
        ready.await.unwrap();
        release.send(()).unwrap();
        work.finish(Some(task.await.unwrap().map(|path| Written {
            path,
            temporary: true,
        })));
        assert!(!work.pending);
        assert_eq!(
            work.result.as_ref().map(|outcome| outcome.written.clone()),
            Some(Err("disk probe".into()))
        );
        assert!(work.unexported(3), "a failed export wrote nothing");
        assert!(work.begin(&trace, 3).is_some());
        assert!(work.result.is_none());
        work.finish(Some(Ok(Written {
            path: PathBuf::from("/tmp/trace.jsonl"),
            temporary: false,
        })));
        assert_eq!(work.result.as_ref().map(|outcome| outcome.id), Some(2));
        assert!(
            !work.unexported(3),
            "what the export began with is in a file"
        );
        assert!(work.unexported(4), "a frame recorded since is not");
        // A dialog nobody answered writes nothing and says nothing.
        assert!(work.begin(&trace, 5).is_some());
        work.finish(None);
        assert!(!work.pending && work.result.is_none());
        assert!(work.unexported(5));
    }

    #[test]
    fn the_default_name_says_which_agent_and_when() {
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(86_400 * 400);
        let name = default_name(Some("Claude Code / ACP"), at);
        assert!(name.starts_with("acp-trace-claude-code---acp-"), "{name}");
        assert!(name.ends_with(".jsonl"));
        let stamp = name
            .trim_start_matches("acp-trace-claude-code---acp-")
            .trim_end_matches(".jsonl");
        assert_eq!(stamp.len(), "YYYYMMDD-HHMMSS".len(), "{stamp}");
        assert!(default_name(None, at).starts_with("acp-trace-agent-"));
        assert!(default_name(Some("///"), at).starts_with("acp-trace-agent-"));
    }

    #[test]
    fn an_export_notice_can_be_dismissed_and_says_where_the_file_is() {
        #[component]
        fn Host(outcome: Outcome) -> Element {
            rsx! { Notice { outcome, on_dismiss: move |()| {} } }
        }
        let render = |outcome: Outcome| {
            let mut dom = VirtualDom::new_with_props(Host, HostProps { outcome });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        };
        let saved = render(Outcome {
            id: 1,
            written: Ok(Written {
                path: PathBuf::from("/x/t.jsonl"),
                temporary: false,
            }),
        });
        assert!(
            saved.contains("Saved the trace to") && !saved.contains("temporary"),
            "{saved}"
        );
        assert!(saved.contains(r#"data-slot="dismiss-export""#), "{saved}");
        let failed = render(Outcome {
            id: 2,
            written: Err("no room".into()),
        });
        assert!(
            failed.contains("not written: no room") && failed.contains("Dismiss"),
            "{failed}"
        );
    }
}
