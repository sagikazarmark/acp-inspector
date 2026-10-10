//! The Connect dialog, where an agent is launched. See `docs/architecture.md` §9.
//!
//! A native `<dialog>` opened with `showModal`: the platform traps focus, dims
//! the window, closes on Escape and restores focus. The backdrop is a
//! `<form method="dialog">`, so a click outside closes it too.

use acp_inspector_core::AgentCommand;
use dioxus::prelude::*;

use crate::spawn::{LaunchFailure, SpawnForm};

/// The dialog's element id.
const DIALOG: &str = "connect-dialog";

/// Opens the dialog modally.
///
/// `showModal`, not an `open` attribute: only the call makes it modal. No
/// signal mirrors the open state, because Escape and the backdrop close it
/// without telling Rust, and a stale signal would swallow the next open. Only a
/// crate `const` is interpolated into the script.
pub fn open() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.showModal()"));
}

/// Closes it; called when an agent answers a launch.
pub fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// Shows the dialog and focuses the launch failure alert once drawn, or
/// Command if it never comes. The alert settles a few frames after the launch
/// returns, so the script waits for it.
pub fn show_failure() {
    document::eval(&failure_focus());
}

fn failure_focus() -> String {
    format!(
        r#"const dialog = document.getElementById({DIALOG:?});
if (dialog && !dialog.open) dialog.showModal();
for (let attempt = 0; attempt < 30; attempt += 1) {{
  await new Promise(requestAnimationFrame);
  const alert = document.getElementById('launch-failure');
  if (alert) {{ alert.focus(); return; }}
}}
document.getElementById({COMMAND:?})?.focus();"#,
        COMMAND = crate::spawn::COMMAND,
    )
}

/// The launch form, in a dialog over the window.
#[component]
pub fn Connect(
    running: bool,
    launching: bool,
    recent: Vec<AgentCommand>,
    unsaved: Option<String>,
    /// Why the last launch did not reach an agent, said inside the dialog.
    #[props(default)]
    failure: Option<LaunchFailure>,
    /// The invocation the window last launched, which the fields follow.
    #[props(default)]
    launched: Option<AgentCommand>,
    /// A remembered invocation to fill the fields with (`SpawnForm::prefill`).
    #[props(default)]
    prefill: Option<(u64, AgentCommand)>,
    on_launch: EventHandler<AgentCommand>,
    #[props(default)] on_remove: Option<EventHandler<AgentCommand>>,
    #[props(default)] on_forget: Option<EventHandler<AgentCommand>>,
) -> Element {
    rsx! {
        dialog {
            id: DIALOG,
            class: "modal connect-dialog",
            "data-slot": "connect",
            aria_labelledby: "connect-title",
            div { class: "modal-box dialog-box connect-box",
                header { class: "dialog-head",
                    h2 { id: "connect-title", if running { "Agent command" } else { "Connect to an agent" } }
                    span { class: "grow" }
                    button {
                        class: "btn btn-xs btn-quiet",
                        "data-slot": "close-connect",
                        r#type: "button",
                        aria_label: "Close",
                        title: "Close",
                        onclick: move |_| close(),
                        {crate::disclosure::close()}
                    }
                }

                SpawnForm { running, launching, recent, unsaved, failure, launched, prefill, on_launch, on_remove, on_forget }
            }

            // The click outside. Out of the Tab order: the keyboard has Escape
            // and Close.
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", tabindex: "-1", "close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_renders_closed_with_a_native_backdrop_form() {
        let html = shown();
        let dialog = html
            .split("<dialog")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(
            !dialog
                .split_whitespace()
                .any(|attribute| attribute == "open" || attribute.starts_with("open=")),
            "the dialog starts closed: {html}"
        );
        assert!(
            html.contains(r#"method="dialog""#) && html.contains(r#"class="modal-backdrop""#),
            "{html}"
        );
    }

    fn shown() -> String {
        #[component]
        fn Host() -> Element {
            rsx! {
                Connect {
                    running: false,
                    launching: false,
                    recent: Vec::new(),
                    unsaved: None,
                    on_launch: move |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_dialog_is_in_the_document_whether_or_not_it_is_showing() {
        let html = shown();

        assert!(
            html.contains(r#"data-slot="connect""#) && html.contains("Connect to an agent"),
            "the dialog is in the document whether or not it is showing: {html}"
        );
        assert!(
            html.contains(r#"data-slot="close-connect""#),
            "and a way out that is not the keyboard: {html}"
        );
        assert!(
            html.contains(r#"aria-labelledby="connect-title""#)
                && html.contains(r#"<h2 id="connect-title">Connect to an agent</h2>"#)
                && html.contains(r#"data-slot="dismiss" tabindex="-1""#),
            "the dialog is named by its heading and the backdrop is not a Tab stop: {html}"
        );
        assert!(
            failure_focus().contains("launch-failure")
                && failure_focus().contains("launch-command"),
            "a failed launch takes focus to its alert, or to Command"
        );
    }
}
