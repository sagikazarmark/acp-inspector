//! The Sessions dialog, opened from the Timeline's header: where sessions are
//! opened, switched and ended. A `<dialog>` rather than a menu, because each row
//! carries several calls and a roots control. See `docs/architecture.md` §7.5, §9.

use crate::agent::Claims;
use dioxus::prelude::*;

#[cfg(test)]
use acp_inspector_core::v1;

/// The dialog's element id.
const DIALOG: &str = "sessions-dialog";

/// Opens it modally. See [`crate::connect::open`] for why this is a call and
/// not an attribute. Only a crate `const` is interpolated into the script.
pub fn open() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.showModal()"));
}

/// Closes it; called when a session opens.
pub fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// The sessions this agent has, and the four calls that act on them.
#[component]
pub fn Sessions(claims: Claims) -> Element {
    // The `session/new` roots control's text. Held here so it survives the
    // dialog being shut and reopened.
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
            aria_labelledby: "sessions-title",
            // Focus lands on the box, not on Close, which `showModal` would
            // otherwise pick as the first focusable element.
            div {
                class: "modal-box dialog-box sessions-box",
                tabindex: "-1",
                autofocus: true,
                header { class: "dialog-head",
                    h2 { id: "sessions-title", "Sessions" }
                    // Zero is shown: it is the agent's answer (ADR 0006).
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
                crate::mcp::McpEditor { prefix:"sessions" }
            }

            // The click outside, which `showModal` does not give. Out of the
            // Tab order: Escape and Close are the keyboard's ways out.
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", tabindex: "-1", "close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent that advertised everything this dialog gates a control on.
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

    /// The dialog, rendered. Built inside a component because `Claims` carries
    /// `EventHandler`s, which need a runtime.
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

        // A form submitting to the dialog closes it on a backdrop click.
        assert!(
            html.contains(r#"<form method="dialog" class="modal-backdrop">"#)
                && html.contains(r#"data-slot="dismiss""#),
            "the click outside is a form that closes the dialog: {html}"
        );
        assert!(
            html.contains(r#"aria-labelledby="sessions-title""#)
                && html.contains(r#"id="sessions-title""#)
                && html.contains(r#"data-slot="dismiss" tabindex="-1""#),
            "the dialog is named by its heading and the backdrop is not a Tab stop: {html}"
        );

        // The first focus is the box, not Close.
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
