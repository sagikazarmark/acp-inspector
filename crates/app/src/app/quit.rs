//! The quit guard: quitting stops the agent first, and asks before losing
//! frames nobody exported.

use acp_inspector_core::Inspector;
use dioxus::prelude::*;

use super::launch::watch_stop;
use super::stores::Stores;
use crate::export;
#[cfg(feature = "desktop")]
use crate::shell;

/// How quitting is asked for.
#[derive(Clone, Copy)]
pub(crate) struct Quit {
    pub quit_now: EventHandler<()>,
    #[cfg(feature = "desktop")]
    pub on_quit: EventHandler<()>,
}

#[cfg_attr(not(feature = "desktop"), allow(unused_variables))]
pub(crate) fn use_quit(
    inspector: &Inspector,
    stores: Stores,
    export_work: Signal<export::Work>,
    stopping: Signal<bool>,
) -> Quit {
    // Frames recorded since the last finished export that the trace still holds.
    #[cfg(feature = "desktop")]
    let unexported = {
        let Stores {
            frames, dropped, ..
        } = stores;
        use_memo(move || {
            !frames.read().is_empty()
                && export_work
                    .read()
                    .unexported(dropped() + frames.read().len())
        })
    };
    // Ask the agent to stop, give it `QUIT_GRACE`, then close; the window's
    // teardown (`shell::use_shutdown`) kills whatever is left.
    let quitting = inspector.clone();
    let quit_now = EventHandler::new(move |()| {
        quitting.disconnect_gracefully();
        watch_stop(quitting.clone(), stopping);
        let inspector = quitting.clone();
        spawn(async move {
            let _ = tokio::time::timeout(QUIT_GRACE, inspector.stopped()).await;
            inspector.disconnect();
            #[cfg(feature = "desktop")]
            shell::quit();
        });
    });
    #[cfg(feature = "desktop")]
    let on_quit = EventHandler::new(move |()| {
        if unexported() {
            open();
        } else {
            quit_now.call(());
        }
    });
    #[cfg(feature = "desktop")]
    shell::use_shutdown(
        inspector.clone(),
        unexported.into(),
        EventHandler::new(move |()| open()),
    );

    Quit {
        quit_now,
        #[cfg(feature = "desktop")]
        on_quit,
    }
}

/// Asks before quitting while the trace holds frames nobody exported: export,
/// quit anyway, or stay.
#[component]
pub(crate) fn QuitDialog(on_export: EventHandler<()>, on_quit: EventHandler<()>) -> Element {
    rsx! {
        dialog {
            id: DIALOG,
            class: "modal quit-dialog",
            "data-slot": "quit",
            aria_labelledby: "quit-title",
            aria_describedby: "quit-body",
            div { class: "modal-box dialog-box quit-box",
                header { class: "dialog-head",
                    h2 { id: "quit-title", "Quit without exporting?" }
                }
                div { class: "dialog-body",
                    p { id: "quit-body",
                        "The trace holds frames that haven't been exported. They are lost when the window closes."
                    }
                }
                div { class: "dialog-foot",
                    span { class: "grow" }
                    button {
                        class: "btn btn-md btn-quiet",
                        r#type: "button",
                        "autofocus": true,
                        onclick: move |_| close(),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-md btn-quiet",
                        r#type: "button",
                        onclick: move |_| {
                            close();
                            on_export.call(());
                        },
                        "Export trace"
                    }
                    button {
                        class: "btn btn-md btn-filled btn-bad",
                        "data-slot": "quit-anyway",
                        r#type: "button",
                        onclick: move |_| {
                            close();
                            on_quit.call(());
                        },
                        "Quit anyway"
                    }
                }
            }
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", tabindex: "-1", "close" }
            }
        }
    }
}

const DIALOG: &str = "quit-dialog";

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
fn open() {
    document::eval(&format!(
        "const dialog = document.getElementById({DIALOG:?}); if (dialog && !dialog.open) dialog.showModal();"
    ));
}

fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// How long quitting waits for a graceful stop before the window's teardown
/// kills the agent.
const QUIT_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quit_question_is_named_and_offers_export_first() {
        #[component]
        fn Host() -> Element {
            rsx! { QuitDialog { on_export: move |()| {}, on_quit: move |()| {} } }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"id="quit-dialog""#)
                && html.contains(r#"aria-labelledby="quit-title""#)
                && html.contains(">Export trace</button>")
                && html.contains(">Quit anyway</button>"),
            "{html}"
        );
    }
}
