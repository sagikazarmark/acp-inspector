//! Where sessions are opened, switched and ended (`docs/architecture.md` §9).
//!
//! **It is reached from the Timeline's own header, because a session is what
//! the Timeline draws.** Opening one, switching to one and closing one all
//! change what is on the screen the control sits on, which is the one place a
//! reader can see what the click did. It was a group of the Agent screen, where
//! it collided with that screen's own `Sessions` heading — the capability run of
//! the same name, 320px below it — and where every act on a session was a tab
//! away from the session.
//!
//! **A dialog and not a menu.** The sketch this came from was a dropdown, and
//! the rows would not have it: a listed session carries `session/load` or
//! `session/resume` with its own `additionalDirectories` control, `session/close`
//! and `session/delete`, the directory it was opened in, when it was last
//! updated, and a page cursor under all of it (§7.5). A menu holding that is a
//! panel wearing a menu's clothes, and a menu that dropped it would be
//! abridging the affordances §7.5 gates one at a time. What the platform's
//! `<dialog>` gives is what `crate::connect` already relies on it for: a focus
//! trap, a backdrop, Escape, and focus handed back to the button that opened it.

use crate::agent::Claims;
use dioxus::prelude::*;

#[cfg(test)]
use acp_inspector_core::v1;

/// The dialog, by the name the two scripts below reach it under.
const DIALOG: &str = "sessions-dialog";

/// Open it. See [`crate::connect::open`] for why this is a call and not an
/// attribute, and why no signal shadows it.
///
/// Nothing the agent said reaches this string: the only value interpolated is a
/// `const` this crate wrote.
pub fn open() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.showModal()"));
}

/// Shut it, for the one thing that shuts it which is not the reader: a session
/// that opened, which is the answer every control in here was pressed for.
pub fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// The sessions this agent has, and the four calls that act on them.
#[component]
pub fn Sessions(claims: Claims) -> Element {
    // What the roots control beside `session/new` holds (§7.7). A hook, so it
    // is taken before any branch, and it lives here rather than beside the
    // button so that a dialog shut and reopened comes back to what was typed.
    let creating = use_signal(String::new);
    let count = claims
        .listing
        .as_ref()
        .map(|answered| answered.sessions.len());

    rsx! {
        dialog {
            id: DIALOG,
            class: "modal sessions-dialog",
            "data-slot": "session-manager",
            // **Focus lands on the dialog and not on a control.** `showModal`
            // gives the keyboard to the first focusable thing it finds, which
            // here was the close button — so opening the surface lit a ring on
            // *Close*, which is both the least likely thing a reader came for
            // and the one whose accidental Enter undoes the opening. A
            // container that can hold focus and does nothing with it puts the
            // reader at the top of the dialog, where the heading is, with the
            // tab order still ahead of them.
            div {
                class: "modal-box dialog-box sessions-box",
                tabindex: "-1",
                autofocus: true,
                header { class: "dialog-head",
                    h2 { "Sessions" }
                    // The agent's own count, kept at nought: a nought here is
                    // what the agent answered rather than an empty surface
                    // (ADR 0006).
                    if let Some(count) = count {
                        span { class: "dialog-count", "{count}" }
                    }
                    span { class: "grow" }
                    button {
                        class: "btn btn-xs btn-quiet",
                        "data-slot": "close-sessions",
                        r#type: "button",
                        aria_label: "Close",
                        title: "Close",
                        onclick: move |_| close(),
                        {crate::disclosure::close()}
                    }
                }

                {
                    crate::agent::sessions(
                        claims.listing(),
                        claims.connected,
                        claims.failed.as_ref(),
                        creating,
                        claims.on_new_session,
                    )
                }
            }

            // The click outside, which `showModal` does not give (`connect.rs`).
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", "close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent that advertised everything this dialog gates a control on, so
    /// that a control missing from the markup is a control this moved and lost.
    fn described() -> v1::InitializeResponse {
        v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1).agent_capabilities(
            v1::AgentCapabilities::new()
                .load_session(true)
                .session_capabilities(
                v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .close(v1::SessionCloseCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new())
                    .additional_directories(v1::SessionAdditionalDirectoriesCapabilities::new()),
            ),
        )
    }

    /// The dialog, rendered. The fixture is built *inside* a component because
    /// `Claims` carries `EventHandler`s, and one made outside a running
    /// VirtualDom has no runtime to belong to.
    fn shown(agent: Option<v1::InitializeResponse>) -> String {
        #[component]
        fn Host(agent: Option<v1::InitializeResponse>) -> Element {
            rsx! {
                Sessions { claims: Claims::about(agent) }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { agent });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_platform_owns_the_modality_here_too() {
        let source = include_str!("session.rs");
        assert!(
            source.contains("dialog.showModal()") && source.contains("dialog.close()"),
            "the element is opened and shut by the calls that make it modal"
        );
        let html = shown(None);
        assert!(
            !html.contains("<dialog id=\"sessions-dialog\" open"),
            "and never by the attribute that does not: {html}"
        );
    }

    #[test]
    fn every_affordance_the_group_had_survives_the_move() {
        // The move is a move and never an abridgement: a dialog that dropped
        // `session/close` or the roots control would be narrowing what §7.5
        // gates one claim at a time, under cover of a layout change.
        let html = shown(Some(described()));

        for affordance in ["session/new", "session/list", "additionalDirectories"] {
            assert!(html.contains(affordance), "{affordance} is here: {html}");
        }
        assert!(
            html.contains(r#"data-slot="sessions""#),
            "the group itself is what moved: {html}"
        );
    }

    #[test]
    fn the_platform_is_given_the_click_outside_and_the_first_focus() {
        let html = shown(Some(described()));

        // A click on the backdrop closes a dialog in every application anybody
        // has used, and `showModal` does not do it. A form that submits to the
        // dialog is the platform's own way out rather than a listener this
        // window attaches, removes and has to reason about.
        assert!(
            html.contains(r#"<form method="dialog" class="modal-backdrop">"#)
                && html.contains(r#"data-slot="dismiss""#),
            "the click outside is a form that closes the dialog: {html}"
        );

        // And the keyboard arrives at the dialog rather than on its Close
        // button, which is where `showModal` puts it by default: the least
        // likely thing a reader came for, and the one whose accidental Enter
        // undoes the opening.
        let box_ = html
            .split_once(r#"class="modal-box dialog-box sessions-box""#)
            .expect("the dialog's box")
            .1
            .split_once('>')
            .expect("its attributes")
            .0;
        assert!(
            box_.contains("tabindex=\"-1\"") && box_.contains("autofocus"),
            "the box takes the first focus and does nothing with it: {box_}"
        );
    }
}
