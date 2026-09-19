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

#[component]
pub fn Saved(path: PathBuf) -> Element {
    let retrieval = try_consume_context::<Retrieval>().unwrap_or_default();
    let copy_path = path.clone();
    rsx! {
        div { class: "saved", "data-slot": "export-retrieval",
            p { role: "status", "Exported to temporary storage: "
                code { class: "mono", "{path.display()}" }
            }
            p { class: "hint", "Copy or move this file elsewhere to keep it; the system may remove temporary files." }
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
    pub result: Option<Result<PathBuf, String>>,
}

impl Work {
    pub fn begin(&mut self, trace: &Trace) -> Option<Export> {
        if self.pending {
            return None;
        }
        let snapshot = trace.export();
        self.pending = true;
        self.result = None;
        Some(snapshot)
    }

    pub fn finish(&mut self, result: Result<PathBuf, String>) {
        self.result = Some(result);
        self.pending = false;
    }
}

pub async fn write(snapshot: Export) -> Result<PathBuf, String> {
    in_background(move || snapshot.save()).await
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
        rsx! { Saved { path: PathBuf::from("/tmp/export folder/trace '🦀'.jsonl") } }
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
        let snapshot = work.begin(&trace).unwrap();
        assert!(work.pending);
        assert!(work.begin(&trace).is_none());
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
        work.finish(task.await.unwrap());
        assert!(!work.pending);
        assert_eq!(work.result, Some(Err("disk probe".into())));
        assert!(work.begin(&trace).is_some());
        assert!(work.result.is_none());
    }
}
