//! Where an agent is started (`docs/architecture.md` §9).
//!
//! **It is a dialog because launching is a thing done once.** The four fields
//! and the list of invocations that worked were a permanent column of the
//! window: measured, the rail they were in was a fifth of its width and forty
//! per cent of it was empty, and nothing in it changed while a reader worked. A
//! task performed once per connection does not earn a column; it earns a way in
//! and a way out.
//!
//! **The platform owns the modality.** A native `<dialog>` opened with
//! `showModal` traps focus, dims what is behind it, closes on Escape and
//! restores focus to whatever opened it — four behaviours this window would
//! otherwise have to write, get subtly wrong, and test. Opening and closing are
//! the two things a Rust signal cannot do to it, because the modal state lives
//! on the element rather than in an attribute, so those go through the one
//! script this module has.
//!
//! **And the fifth behaviour is a form.** A click on the backdrop closes a
//! dialog in every application anybody has used, and `showModal` does not do it
//! — so the backdrop is a `<form method="dialog">` filling the space behind the
//! box, whose submit button is the click. It is the platform's own way out
//! again rather than a listener this window has to attach, remove and reason
//! about: submitting a `dialog` form closes the dialog it is in.

use acp_inspector_core::AgentCommand;
use dioxus::prelude::*;

use crate::spawn::SpawnForm;

/// The dialog, by the name the two scripts below reach it under.
const DIALOG: &str = "connect-dialog";

/// Open the dialog.
///
/// **`showModal` and not an `open` attribute.** The attribute renders the
/// element in the page like any other box: no backdrop, no focus trap, no
/// Escape. Modality is a property of the element that only this call sets, so a
/// component rendering `open: true` would draw the form in a corner of the
/// window with the window still usable behind it and call it a modal.
///
/// **And no `open` signal to go with it.** Whether the dialog is showing lives
/// on the element, because that is where the platform keeps it: Escape and the
/// backdrop close it without asking anybody, and a Rust signal that thought it
/// was still open would answer the next click by setting a value that had not
/// changed — a Connect button that works once. Nothing in this window needs to
/// know, so nothing in it is told.
///
/// Nothing the agent said reaches this string: the only value interpolated is a
/// `const` this crate wrote.
pub fn open() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.showModal()"));
}

/// Shut it, for the one thing that shuts it which is not the reader: an agent
/// that answered.
pub fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// The launch form, in a dialog over the window.
#[component]
pub fn Connect(
    running: bool,
    launching: bool,
    recent: Vec<AgentCommand>,
    unsaved: Option<String>,
    on_launch: EventHandler<AgentCommand>,
) -> Element {
    rsx! {
        dialog {
            id: DIALOG,
            class: "modal connect-dialog",
            "data-slot": "connect",
            div { class: "modal-box dialog-box connect-box",
                header { class: "dialog-head",
                    h2 { "Connect to an agent" }
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

                SpawnForm { running, launching, recent, unsaved, on_launch }
            }

            // The click outside. Nothing in it is drawn — the box above covers
            // the middle and this covers the rest — and its one control is
            // named for the reader who reaches it by keyboard rather than by
            // pointing.
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", "close" }
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
    }
}
