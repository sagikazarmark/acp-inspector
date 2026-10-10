//! The roots a session is opened with, and the reopen control that carries them.

use acp_inspector_core::{Restore, Roots, v1};
use dioxus::prelude::*;

use super::Opening;
use super::sessions::{OPENING, described_unless};

/// The way into one listed session, and the roots it will be reopened with
/// (§7.7).
///
/// **A component rather than more markup in the row above**, because the roots
/// are state: each row's control holds its own list, and a list somebody typed
/// survives the listing being asked for again — which it is, on the tool's own
/// initiative, after every close and delete.
///
/// **Prefilled with what the listing reported**, so that reopening stays a
/// reopen unless the user deliberately changes it (§7.5). Changing it is the
/// point of the control and not a hazard: the schema permits a list that differs
/// from any previously reported one as long as the `cwd` matches, and what the
/// agent does with a list it did not report is the finding.
///
/// **A control nobody changed asks for nothing**, and says so with `None` rather
/// than by handing back a copy of what it was prefilled with. That is what keeps
/// a default reopen the *listing's own* roots however this window rendered them
/// — a path this control could not draw faithfully would otherwise reach the
/// wire as the drawing rather than as the path.
///
/// **The control is gated and the reopen is not.** Where the agent advertised no
/// `additionalDirectories` there is nothing here to type in and the click says
/// `None` for that reason instead, which is core's cue to hand the agent back
/// the roots it reported — because a reopen gated into asking for less would be
/// reopening a different session from the one it named.
///
/// **What the user typed survives the listing being asked for again**, which it
/// is on the tool's own initiative after every close and delete: the row keeps
/// its own state for as long as the session keeps its place in the listing, and
/// a control that emptied itself out from under the typing would be spending the
/// user's work on housekeeping they did not ask for.
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

    // What the agent said this session's roots are, as one line. The summary
    // carries it so that shutting the control does not take away the agent's own
    // words: those are what the field is prefilled from and what will cross the
    // wire if nobody edits them, and a disclosure that hid them would be buying
    // room with the evidence (§7.7).
    let announced = reported.text().trim().replace('\n', " · ");

    rsx! {
        button {
            class: "btn btn-xs btn-quiet btn-go reopen",
            r#type: "button",
            title: "{how.method()}",
            aria_label: "{open_label}",
            // What pressing it costs, on the control rather than above the
            // list of them (`OPENING`).
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
            // **Under the call rather than in front of it.** These are what the
            // reopen will carry, so they belong to that button — and drawn
            // first, at the head of the row's action column, the disclosure was
            // the widest thing on the row: a modifier read as the row's
            // subject, with the session's own working directory squeezed into
            // an ellipsis beside it.
            //
            // **Shut by default, and one per row.** Eleven of these textareas —
            // one for `session/new` and one for each listed session — were two
            // and a half screens of the same control repeated, on a rail already
            // six screens long. What it costs is a click before the roots of a
            // *particular* listed session can be edited; what it does not cost
            // is knowing what they are, which the summary says whether it is
            // open or shut, or that they can be edited at all, which is what the
            // control being named rather than hidden is for.
            details { class: "disclose bare roots-supplied", "data-slot": "roots-supplied",
                summary { aria_label: "{roots_label}", title: "{roots_label}",
                    {crate::disclosure::chevron()}
                    // No `reported` badge in front of it. The badge and the
                    // words after it said one thing twice — *reported ·
                    // additionalDirectories · the agent reported none* — which
                    // on a row is three chips for one fact.
                    code { "additionalDirectories" }
                    // The agent's own words get the row's colour, not a hint's:
                    // what it reported is content here, and the word saying it
                    // reported nothing is this window talking.
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

/// What a click on a listing row asks for: the roots the user supplied, or
/// `None` for the ones the listing reported (§7.7).
///
/// **Two ways to mean the default and one to mean a change.** An agent that
/// advertised no `additionalDirectories` drew no control, and a control nobody
/// changed is not a change — both say `None`, which is core's cue to hand the
/// agent back the roots it reported rather than a rendering of them. A control
/// the user *emptied* is a change like any other and asks for no roots at all,
/// which is the one thing a fallback to the reported list would get wrong.
fn asking(advertised: bool, supplied: &str, reported: &Roots) -> Option<Roots> {
    let supplied = Roots::supplied(supplied);
    (advertised && &supplied != reported).then_some(supplied)
}

/// The roots control: the list of additional workspace roots a session is to be
/// opened with, as the user supplies them (§7.7).
///
/// One control for both places it appears, because it is one thing: the field
/// `session/new`, `session/load` and `session/resume` all carry, and the same
/// rules for reading it either way.
///
/// **It says the paths must be absolute**, and says what happens to one that is
/// not, because that is the rule the user would otherwise learn from the agent's
/// error — and would learn wrongly, since the inspector resolves a relative path
/// against the session's own `cwd` before it sends one rather than letting a
/// malformed request cross ([`Roots`]).
///
/// **It stays live when the agent is gone**, unlike the button beside it. A
/// field is not an affordance — nothing here sends anything — and the spawn
/// form's own fields keep the same rule while its Launch button is disabled: a
/// list typed against an agent that has just died is a list the next one is
/// asked for, and a control that emptied itself out from under the user would be
/// this window throwing their typing away to make a point the button already
/// makes.
pub(super) fn roots_field(mut roots: Signal<String>, named: bool, field_label: String) -> Element {
    rsx! {
        label { class: "form-field", "data-slot": "roots",
            // The field it supplies, except where the control is inside a
            // disclosure whose summary already names it. Left out of the markup
            // rather than hidden by a rule: a fact kept in the document and
            // painted away is the dimness rule broken by another name, which is
            // the reason `hushed` removes the fourth fact rather than dimming it.
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

    /// The roots a session would be opened with.
    const ROOTS: &str = r#"data-slot="roots""#;

    /// Every roots control in a fragment of the screen, one string each — cut at
    /// the label that holds it, which is the whole control.
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
        // **The advertisement gates the control** (§7.7), like every other
        // affordance here: `additionalDirectories` is a field on a request
        // rather than a method, so what an agent that claimed it gets is a place
        // to supply one — beside the create button, and on each row that reopens
        // a session.
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
        // And each is named as the protocol names it — once per control, on the
        // control itself beside `session/new` and on the summary that stands for
        // it on a listing row, where the disclosure is what carries the name.
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
        // Gated on the advertisement and on nothing else — and the rest of the
        // row is untouched, because the reopen goes on being offered and goes on
        // sending the roots the listing reported (core's rule, asserted in
        // `crates/core/tests/roots.rs`). What is withheld is the control, not the call.
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
        // **So that reopening stays a reopen unless the user deliberately
        // changes it** (§7.5). Per row, because the roots are the session's: a
        // control that showed one session's roots on another's row would ask an
        // agent to reopen a session as a different one.
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
        // **What a click means, decided where it can be asserted** (§12). A
        // control the user has not touched is not a change: it says `None`, and
        // core hands the agent back the roots it reported — byte for byte, and
        // not a rendering of them that came back through a textarea. A control
        // they *emptied* is a change like any other and asks for no roots at
        // all, which is the case a fallback to the reported list would get
        // exactly backwards.
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
        // The rule the user would otherwise learn from the agent's error — and
        // learn wrongly, because a relative path never reaches the wire: it is
        // resolved against the session's own `cwd` first (§7.7).
        //
        // It used to be said inside each control, which meant an agent
        // advertising `additionalDirectories` with two sessions listed said it
        // three times and one with ten said it eleven. That is the shape §7.7
        // already refused for the fourth fact — one sentence about the inspector
        // drawn as eleven facts about the agent — so it is hoisted here for the
        // same reason and held to the same rule: said once, not dropped.
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

        // What stays on every control is the half that labels the box it is
        // beside rather than the half that states the rule.
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
        // The control is shut on a listing row (§7.7) — eleven textareas was
        // two and a half screens of one control repeated. What must survive
        // shutting it is the agent's own words: the roots it reported are what
        // the field is prefilled from and what crosses the wire if nobody edits
        // them, so a disclosure that hid them would be buying room with the
        // evidence.
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
