//! The roots a session is opened with, and the reopen control that carries them.

use acp_inspector_core::{Restore, Roots, v1};
use dioxus::prelude::*;

use super::Opening;
use super::sessions::{OPENING, described_unless};

/// The reopen button for one listed session, and the roots it reopens with
/// (§7.7).
///
/// A component because each row keeps its own roots, so typing survives the
/// listing being refreshed after a close or delete. Prefilled with the reported
/// roots; an unchanged control sends `None` so core sends the listing's own
/// roots byte for byte. Only the control is gated on `additionalDirectories`;
/// the reopen is always offered.
#[component]
pub(super) fn Reopen(
    /// The session this row is about, as the agent described it.
    session: v1::SessionInfo,
    /// Which call reopens it — core's answer, preference included (§7.5).
    how: Restore,
    /// Whether the agent advertised `additionalDirectories`.
    roots: bool,
    connected: bool,
    on_open: EventHandler<Opening>,
) -> Element {
    let reported = Roots::reported(&session.additional_directories);
    let supplying = use_signal(|| reported.text().to_owned());
    let session_id = session.session_id.to_string();
    let verb = match how.method() {
        "session/load" => "Load",
        "session/resume" => "Resume",
        _ => "Open",
    };
    let open_label = format!("{verb} session {session_id}");
    let roots_label = format!("Edit additionalDirectories for session {session_id}");
    let field_label = format!("additionalDirectories for session {session_id}");
    let opened = session.clone();

    // The reported roots on one line, so the shut summary still shows them.
    let announced = reported.text().trim().replace('\n', " · ");

    rsx! {
        button {
            class: "btn btn-xs btn-quiet btn-go reopen",
            r#type: "button",
            title: "{how.method()}",
            aria_label: "{open_label}",
            aria_describedby: described_unless(connected, OPENING),
            aria_disabled: (!connected).then_some("true"),
            onclick: move |_| {
                if connected {
                    on_open.call(Opening {
                        session: opened.clone(),
                        roots: asking(roots, &supplying(), &reported),
                    })
                }
            },
            "{verb}"
        }
        if roots {
            // After the button it belongs to, and shut by default so a long
            // listing is not a column of textareas.
            details { class: "disclose bare roots-supplied", "data-slot": "roots-supplied",
                summary { aria_label: "{roots_label}", title: "{roots_label}",
                    {crate::disclosure::chevron()}
                    code { "additionalDirectories" }
                    // The agent's words are content; "none reported" is a hint.
                    if announced.is_empty() {
                        span { class: "hint", " none reported" }
                    } else {
                        span { class: "mono", " {announced}" }
                    }
                }
                div { class: "disclose-body",
                    {roots_field(supplying, false, field_label)}
                }
            }
        }
    }
}

/// The roots a reopen asks for: `None` (the reported roots) when nothing was
/// advertised or the control is unchanged, otherwise what the user supplied.
/// An emptied control asks for no roots, not for the reported ones.
fn asking(advertised: bool, supplied: &str, reported: &Roots) -> Option<Roots> {
    let supplied = Roots::supplied(supplied);
    (advertised && &supplied != reported).then_some(supplied)
}

/// The roots field, one absolute path per line, for `session/new`,
/// `session/load` and `session/resume` (§7.7).
///
/// It stays editable while disconnected: it sends nothing, and what was typed
/// is kept for the next agent.
pub(super) fn roots_field(mut roots: Signal<String>, named: bool, field_label: String) -> Element {
    rsx! {
        label { class: "form-field", "data-slot": "roots",
            // The field name is left out, not hidden, where a summary names it.
            div { class: "form-label",
                if named {
                    code { class: "form-key", "additionalDirectories" }
                } else {
                    span { class: "form-key", "Open with these roots" }
                }
                span { class: "form-note", "one absolute path per line" }
            }
            textarea {
                class: "textarea",
                aria_label: "{field_label}",
                rows: 2,
                value: "{roots}",
                spellcheck: false,
                placeholder: "/srv/data",
                oninput: move |event| roots.set(event.value()),
            }
        }
    }
}

/// The rule every roots control is held to, said once above them.
pub(super) fn roots_rule() -> Element {
    rsx! {
        p { class: "hint", "data-slot": "roots-rule",
            "Each path must be absolute. A relative one is resolved against the session's "
            code { "cwd" }
            "."
        }
    }
}

#[cfg(test)]
mod tests {
    use acp_inspector_core::SessionListing;

    use super::*;
    use crate::agent::fixtures::*;

    #[test]
    fn a_long_reported_root_remains_complete_in_the_summary_and_field() {
        let path = format!("/{}", "a".repeat(2_000));
        let mut listing = SessionListing::default();
        listing.sessions = vec![
            v1::SessionInfo::new("sess-1", "/tmp")
                .additional_directories(vec![path.clone().into()]),
        ];
        let panel = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(listing),
        ));

        assert!(
            panel.matches(&path).count() >= 2,
            "the complete path remains in both the closed summary and textarea: {panel}"
        );
        assert!(
            !panel.contains('…'),
            "protocol paths are not replaced with inaccessible ellipses: {panel}"
        );
        let row = rows_shown(&panel)[0];
        for affordance in ["session/load", "session/close", "session/delete"] {
            let name = format!(r#"aria-label="{} session sess-1""#, verb(affordance));
            assert!(
                row.contains(&name),
                "long content does not cost the target-specific {affordance} name: {row}"
            );
        }
    }

    const ROOTS: &str = r#"data-slot="roots""#;

    /// Every roots control in a fragment, cut at its label.
    fn roots_controls(html: &str) -> Vec<&str> {
        html.split(ROOTS)
            .skip(1)
            .map(|control| {
                control
                    .split_once("</label>")
                    .expect("a control is closed")
                    .0
            })
            .collect()
    }

    #[test]
    fn the_roots_a_session_opens_with_are_asked_for_where_the_agent_advertised_them() {
        let offered = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        let controls = roots_controls(&offered);
        assert_eq!(
            controls.len(),
            3,
            "one beside `session/new` and one per listed session: {offered}"
        );
        for control in controls {
            assert!(
                control.contains("<textarea"),
                "each is somewhere to type one: {control}"
            );
        }
        assert_eq!(
            offered
                .matches("<code>additionalDirectories</code>")
                .count(),
            3,
            "named once per place there is to supply one: {offered}"
        );
        assert!(
            offered.contains(r#"aria-label="Edit additionalDirectories for a new session""#)
                && offered.contains(r#"title="Edit additionalDirectories for a new session""#)
                && offered.contains(r#"aria-label="additionalDirectories for a new session""#),
            "the new Session disclosure and field retain complete names and a tooltip: {offered}"
        );
    }

    #[test]
    fn an_agent_that_advertised_no_additional_directories_is_offered_nowhere_to_supply_any() {
        // The control is withheld, not the reopen.
        let listed = affordances(&rendered(
            only_listing(),
            true,
            Some(Restore::Load),
            Some(reporting_roots()),
        ));

        assert!(
            roots_controls(&listed).is_empty(),
            "no claim, no control anywhere: {listed}"
        );
        for row in rows_shown(&listed) {
            assert_eq!(ways_in(row), 1, "and the way in is still here: {row}");
        }
    }

    #[test]
    fn a_reopen_is_prefilled_with_the_roots_the_listing_reported_for_that_session() {
        // Per row, because the roots are the session's.
        let listed = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(reporting_roots()),
        ));

        let rows = rows_shown(&listed);
        assert_eq!(rows.len(), 2, "one row per listed session: {listed}");
        assert!(
            rows[0].contains("value=\"/srv/data\n/srv/models\""),
            "the roots this session reported, one per line: {}",
            rows[0]
        );
        assert!(
            rows[1].contains(r#"value="""#),
            "and a session that reported none is prefilled with none: {}",
            rows[1]
        );
    }

    #[test]
    fn a_reopen_asks_for_the_reported_roots_unless_the_user_changed_them() {
        let reported = Roots::reported(&["/srv/data".into()]);

        assert_eq!(
            asking(false, "", &reported),
            None,
            "an agent that advertised nothing drew no control to change"
        );
        assert_eq!(
            asking(true, "/srv/data", &reported),
            None,
            "and a control nobody changed is not a change"
        );
        assert_eq!(
            asking(true, "/srv/data\n/srv/models", &reported),
            Some(Roots::supplied("/srv/data\n/srv/models")),
            "a list they added to is what is asked for"
        );
        assert_eq!(
            asking(true, "", &reported),
            Some(Roots::supplied("")),
            "and one they cleared asks for no roots, rather than for the reported ones again"
        );
    }

    #[test]
    fn the_rule_a_root_must_keep_is_stated_once_above_every_control_it_binds() {
        let offered = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        assert_eq!(
            offered.matches("Each path must be absolute").count(),
            1,
            "one sentence about this tool, said once: {offered}"
        );
        assert!(
            offered.contains("resolved against the session"),
            "and what becomes of a path that is not: {offered}"
        );

        let controls = roots_controls(&offered);
        assert!(controls.len() > 1, "more than one control to bind");
        for control in controls {
            assert!(
                control.contains("one absolute path per line"),
                "each control still says what goes in it: {control}"
            );
        }
    }

    #[test]
    fn a_listed_sessions_roots_are_legible_without_opening_its_control() {
        // A shut control's summary still shows the reported roots.
        let offered = affordances(&rendered(
            everything(),
            true,
            Some(Restore::Load),
            Some(two_sessions()),
        ));

        for row in rows_shown(&offered) {
            let summary = row
                .split_once("<summary")
                .expect("a shut control names itself")
                .1
                .split_once("</summary>")
                .expect("a summary that ends")
                .0;
            assert!(
                summary.contains("additionalDirectories"),
                "named by the field it supplies: {summary}"
            );
            assert!(
                summary.contains("/srv") || summary.contains("none reported"),
                "and saying what the agent reported: {summary}"
            );
        }
    }
}
