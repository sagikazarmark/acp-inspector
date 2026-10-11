//! A capability row: the claim, the shape it was made in, and its fourth fact.

use acp_inspector_core::{AgentCapability, AuthState, Driven, DrivenRecord};

/// A capability row's fourth fact: what became of driving it, or why nothing
/// here can (§7.7). Never both: *not driven* on an unreachable row would blame
/// the agent for a limit of this tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Fourth {
    /// What became of driving it: core's record, or the auth state for
    /// `authenticate`.
    Outcome(Driven),
    /// Nothing here can drive it, and this is why.
    Unreached(Unreached),
}

/// Why nothing in this tool drives an advertisement. A fact about where the
/// affordances live, so it is decided beside the rows rather than stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unreached {
    /// `session/resume` beside an advertised `session/load`, which
    /// `Restore::preferred` picks, so a resume is never sent.
    LoadPreferred,
    /// `session/close` or `session/delete` without `session/list`: both are
    /// pressed on a listing row, so there is none to press.
    NoListingRow,
}

impl Unreached {
    /// The reason alone, shared by the row and run forms so they cannot drift.
    fn because(self) -> &'static str {
        match self {
            Self::LoadPreferred => "it prefers session/load, which this agent advertised too",
            Self::NoListingRow => {
                "it is pressed on a listing row, and this agent advertised no session/list"
            }
        }
    }

    /// What the row says instead of an outcome: that this tool cannot drive it,
    /// then why. The leading space separates it from the state when read aloud.
    pub(super) fn said(self) -> String {
        format!(" this tool cannot drive it: {}", self.because())
    }

    /// The same sentence for a whole run, said once above it.
    pub(super) fn said_of_all(self) -> String {
        format!("this tool cannot drive any of these: {}", self.because())
    }
}

/// The shapes ACP v1 states an advertisement in, kept apart rather than
/// flattened (§7.5): `session/load` is a top-level boolean while the rest of
/// the lifecycle is gated by an object's presence. `authenticate` is gated on
/// the `authMethods` list (§7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// A boolean field, where `false` and absent are the same claim.
    Flag,
    /// A capability object, whose presence is the claim, empty or not.
    Object,
    /// A list, where an entry is the claim and an empty list is none.
    List,
}

/// A run of claims made in one shape at one place in the `initialize` result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Claimed {
    pub(super) shape: Shape,
    /// Where the run lives, by the name the protocol gives that object.
    pub(super) at: &'static str,
    pub(super) claims: Vec<Claim>,
}

impl Claimed {
    /// The reason nothing here drives any row of the run, where two or more
    /// rows share the same one, so the run can say it once. A mixed run (the
    /// session lifecycle) keeps the reason on its rows.
    pub(super) fn shared_reason(&self) -> Option<Unreached> {
        let mut reasons = self.claims.iter().map(|claim| match claim.fourth {
            Some(Fourth::Unreached(why)) => Some(why),
            _ => None,
        });
        let first = reasons.next().flatten()?;
        (self.claims.len() > 1 && reasons.all(|why| why == Some(first))).then_some(first)
    }

    /// The run with the fourth fact removed from its rows, for when the run
    /// says it. Removed rather than hidden, so screen readers get it once too.
    pub(super) fn hushed(mut self) -> Self {
        for claim in &mut self.claims {
            claim.fourth = None;
        }
        self
    }
}

/// One advertisement: its name, the field it is claimed in, whether it was
/// advertised, and its fourth fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Claim {
    /// The method it gates, or the field name where it gates a field.
    pub(super) name: &'static str,
    /// The field the claim is made in; differs from `name` for the lifecycle.
    pub(super) field: &'static str,
    pub(super) advertised: bool,
    /// The fourth fact, or `None` on a row that says nothing about driving.
    /// Carried in words at the row's weight, never by colour.
    pub(super) fourth: Option<Fourth>,
}

impl Claim {
    pub(super) fn new(name: &'static str, field: &'static str, advertised: bool) -> Self {
        Self {
            name,
            field,
            advertised,
            fourth: None,
        }
    }

    /// A capability named by the field it is claimed in.
    pub(super) fn self_named(name: &'static str, advertised: bool) -> Self {
        Self::new(name, name, advertised)
    }

    /// The fourth fact from core's record, keyed on [`AgentCapability`] so a
    /// capability upstream adds breaks the build rather than rendering unknown.
    pub(super) fn driven(self, record: &DrivenRecord, capability: AgentCapability) -> Self {
        self.if_advertised(Fourth::Outcome(record.of(capability)))
    }

    /// The `authenticate` row's fourth fact, from the auth state: the record has
    /// no entry for it, and the login block reads the same value.
    pub(super) fn logged_in(self, auth: &AuthState) -> Self {
        self.if_advertised(Fourth::Outcome(auth.driven()))
    }

    /// Marks this agent's claim as unreachable here.
    pub(super) fn unreached(self, why: Unreached) -> Self {
        self.if_advertised(Fourth::Unreached(why))
    }

    /// Sets the fourth fact only on an advertised row: there is nothing to have
    /// driven on a claim the agent never made.
    fn if_advertised(mut self, fourth: Fourth) -> Self {
        self.fourth = self.advertised.then_some(fourth);
        self
    }
}

#[cfg(test)]
mod tests {
    use acp_inspector_core::{CallError, Restore, v1};

    use super::*;
    use crate::agent::fixtures::*;

    const UNREACHED: &str = r#"data-slot="unreached""#;

    /// The run of the agent's block holding the row for `capability`.
    fn run_for(panel: &str, capability: &str) -> String {
        runs(&advertisement(panel, "agent"))
            .into_iter()
            .find(|run| run.contains(capability))
            .unwrap_or_else(|| panic!("the {capability} row is in a run: {panel}"))
            .to_owned()
    }

    /// A refusal in the agent's own words.
    fn turned_down() -> Driven {
        Driven::Refused(CallError::Rejected(v1::Error::method_not_found()))
    }

    /// A record where only `session/load` was driven.
    fn record(outcome: Driven) -> DrivenRecord {
        DrivenRecord::from_iter([(AgentCapability::Load, outcome)])
    }

    /// The rows an affordance reaches on an agent that advertised everything,
    /// with the record key each reads. Stated by name rather than derived, so a
    /// row asking about the wrong key is caught. Resume is absent (load is
    /// preferred), and so is `authenticate` (its record is `AuthState`).
    const REACHED: [(&str, AgentCapability); 6] = [
        ("session/load", AgentCapability::Load),
        ("session/list", AgentCapability::List),
        ("session/close", AgentCapability::Close),
        ("session/delete", AgentCapability::Delete),
        (
            "additionalDirectories",
            AgentCapability::AdditionalDirectories,
        ),
        ("logout", AgentCapability::Logout),
    ];

    /// What a row says where nothing here can drive it.
    const CANNOT: &str = " this tool cannot drive it";

    /// The same, said once above a run.
    const CANNOT_ANY: &str = "this tool cannot drive any of these";

    /// Every capability row nothing here can drive, whether the row or its run
    /// says so.
    fn unreachable_rows(block: &str) -> Vec<&str> {
        runs(block)
            .into_iter()
            .flat_map(|run| {
                let said_of_all = run.contains(CANNOT_ANY);
                capability_rows(run)
                    .into_iter()
                    .filter(move |row| said_of_all || row.contains(CANNOT))
            })
            .collect()
    }

    /// Everything a row says when it reports an outcome, including the
    /// fallback for an outcome this window cannot render.
    const OUTCOMES: [&str; 4] = [
        " not called",
        " called · OK",
        " called · refused",
        " called, with an outcome",
    ];

    fn load_row(panel: &str) -> String {
        row_for(panel, "session/load")
    }

    #[test]
    fn the_load_row_says_whether_the_advertisement_was_ever_driven() {
        // All three outcomes: a row that always said one would pass a test
        // asking about only that one.
        for (driven, says, and_not) in [
            (Driven::Unasked, " not called", " called ·"),
            (Driven::Answered, " called · OK", " not called"),
            (turned_down(), " called · refused", " called · OK"),
        ] {
            let row = load_row(&rendered_driven(record(driven)));

            assert!(
                row.contains(&format!(r#"{DRIVEN}{says}"#)),
                "the row says what became of driving it: {row}"
            );
            assert!(!row.contains(and_not), "and says only that: {row}");
        }

        // A refusal carries the agent's own error.
        let refused = load_row(&rendered_driven(record(turned_down())));
        assert!(
            refused.contains("Method not found"),
            "in the agent's own words: {refused}"
        );
    }

    #[test]
    fn a_refusal_is_a_fourth_element_of_the_same_row_and_no_louder() {
        // Read after the state, in the same row, not painted as a failure.
        // `style.rs` holds the stylesheet half.
        let row = load_row(&rendered_driven(record(turned_down())));

        let state = row.find(STATE).expect("the row says its state");
        let driven = row.find(DRIVEN).expect("and what became of driving it");
        assert!(state < driven, "the fourth fact is the fourth thing: {row}");
        assert!(
            !row.contains("bad"),
            "and it is not painted as a failure: {row}"
        );
    }

    fn reporting(panel: &str) -> Vec<String> {
        capability_rows(&advertisement(panel, "agent"))
            .into_iter()
            .filter(|row| OUTCOMES.iter().any(|outcome| row.contains(outcome)))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_row_an_affordance_reaches_reports_an_outcome_and_no_other_row_does() {
        // Every reachable row reports an outcome, `authenticate` included via
        // `AuthState`, and no other row does.
        let mut expected: Vec<_> = REACHED.iter().map(|(capability, _)| *capability).collect();
        expected.push("authenticate");
        expected.push("image");
        expected.push("audio");
        expected.push("embeddedContext");
        expected.push("http");
        expected.push("sse");
        let panel = rendered_driven(record(Driven::Answered));

        let carrying = reporting(&panel);
        for capability in &expected {
            assert!(
                carrying.iter().any(|row| row.contains(capability)),
                "{capability} reports an outcome: {carrying:?}"
            );
        }
        assert_eq!(
            carrying.len(),
            expected.len(),
            "and nothing else does: {carrying:?}"
        );
    }

    #[test]
    fn a_row_no_affordance_reaches_draws_no_outcome_and_says_why() {
        // The load-preferred resume says why rather than reporting an outcome.
        let panel = rendered_driven(record(Driven::Answered));

        let agent = advertisement(&panel, "agent");
        let unreached = unreachable_rows(&agent);
        assert_eq!(
            unreached
                .iter()
                .filter(|row| OUTCOMES.iter().any(|outcome| row.contains(outcome)))
                .count(),
            0,
            "none of them reports an outcome as well: {unreached:?}"
        );
        assert!(
            unreached.iter().any(|row| row.contains("session/resume")),
            "load-preferred resume is unreachable: {unreached:?}"
        );
        assert_eq!(unreached.len(), 1, "and only those: {unreached:?}");
    }

    #[test]
    fn the_resume_row_beside_an_advertised_load_says_which_one_this_tool_prefers() {
        // A resume is never sent beside load, so the row says which call is.
        let row = row_for(&rendered_driven(record(Driven::Answered)), "session/resume");

        assert!(
            row.contains("prefers session/load"),
            "the row says which call this tool sends instead: {row}"
        );
        assert!(
            !OUTCOMES.iter().any(|outcome| row.contains(outcome)),
            "and reports no outcome at all: {row}"
        );
    }

    fn only_resuming() -> v1::InitializeResponse {
        described(v1::AgentCapabilities::new().session_capabilities(
            v1::SessionCapabilities::new().resume(v1::SessionResumeCapabilities::new()),
        ))
    }

    #[test]
    fn an_agent_that_advertised_resume_alone_records_outcomes_on_the_row() {
        // Where resume is what this tool would send, it records like any row.
        let panel = rendered_with(
            Some(only_resuming()),
            true,
            Some(Restore::Resume),
            None,
            None,
            DrivenRecord::from_iter([(AgentCapability::Resume, turned_down())]),
            AuthState::default(),
        );

        let row = row_for(&panel, "session/resume");
        assert!(
            row.contains(&format!(r#"{DRIVEN} called · refused"#))
                && row.contains("Method not found"),
            "what became of driving it, in the agent's own words: {row}"
        );
        assert!(
            !row.contains(CANNOT),
            "and nothing about this tool being unable to: {row}"
        );
    }

    fn ending_without_listing() -> v1::InitializeResponse {
        described(
            v1::AgentCapabilities::new().session_capabilities(
                v1::SessionCapabilities::new()
                    .close(v1::SessionCloseCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            ),
        )
    }

    fn ending_with_listing() -> v1::InitializeResponse {
        described(
            v1::AgentCapabilities::new().session_capabilities(
                v1::SessionCapabilities::new()
                    .list(v1::SessionListCapabilities::new())
                    .close(v1::SessionCloseCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            ),
        )
    }

    #[test]
    fn close_and_delete_with_no_listing_say_there_is_no_row_to_press_them_on() {
        // Close and delete hang off a listing row; without one, the row says so.
        let panel = rendered(ending_without_listing(), true, None, None);

        for capability in ["session/close", "session/delete"] {
            let row = row_for(&panel, capability);
            assert!(
                row.contains("no listing row") || row.contains("pressed on a listing row"),
                "{capability} says where the affordance is: {row}"
            );
            assert!(
                row.contains("advertised no session/list"),
                "and why this agent has none: {row}"
            );
            assert!(
                !OUTCOMES.iter().any(|outcome| row.contains(outcome)),
                "and reports no outcome: {row}"
            );
        }
    }

    #[test]
    fn close_and_delete_beside_a_listing_record_outcomes_normally() {
        let panel = rendered_with(
            Some(ending_with_listing()),
            true,
            None,
            None,
            None,
            DrivenRecord::from_iter([(AgentCapability::Close, turned_down())]),
            AuthState::default(),
        );

        let closed = row_for(&panel, "session/close");
        assert!(
            closed.contains(&format!(r#"{DRIVEN} called · refused"#)),
            "what became of driving it: {closed}"
        );
        let deleted = row_for(&panel, "session/delete");
        assert!(
            deleted.contains(&format!(r#"{DRIVEN} not called"#)),
            "and the one nobody drove says that: {deleted}"
        );
        for row in [&closed, &deleted] {
            assert!(!row.contains(CANNOT), "neither is unreachable: {row}");
        }
    }

    #[test]
    fn mcp_rows_offer_outcomes_only_for_advertised_transports() {
        // Both fixtures: the MCP rows are reachable whatever was advertised.
        for agent in [everything(), nothing()] {
            let panel = shown(agent, true);

            for capability in ["http", "sse"] {
                let run = run_for(&panel, capability);
                assert!(
                    !run.contains(CANNOT_ANY) && !run.contains(CANNOT),
                    "the MCP run is reachable: {run}"
                );
                let row = row_for(&panel, capability);
                assert!(
                    !row.contains("stdio definitions only"),
                    "no deferred reason: {row}"
                );
            }
        }
    }

    #[test]
    fn a_run_nothing_here_reaches_says_so_once_rather_than_on_every_row() {
        // One fact about this tool, said once per run. The rows are checked for
        // silence, since a hoist that left the row copies would still pass a
        // count of the run's sentence.
        let panel = shown(everything(), true);
        let agent = advertisement(&panel, "agent");

        for run in runs(&agent) {
            if !run.contains(CANNOT_ANY) {
                continue;
            }
            assert_eq!(
                run.matches(CANNOT_ANY).count(),
                1,
                "the run says it once: {run}"
            );
            for row in capability_rows(run) {
                assert!(
                    !row.contains(DRIVEN),
                    "and no row under it repeats it: {row}"
                );
            }
        }
        // A mixed run keeps the reason on the row that has one.
        let sessions = run_for(&panel, "session/resume");
        assert!(
            !sessions.contains(CANNOT_ANY) && sessions.contains(CANNOT),
            "a run whose rows do not share the reason keeps it on the row: {sessions}"
        );
    }

    #[test]
    fn a_row_the_agent_did_not_advertise_says_nothing_about_driving() {
        // No claim, nothing to report. Audio and embedded context are skipped:
        // their run carries the reason.
        let panel = shown(nothing(), true);

        for row in capability_rows(&advertisement(&panel, "agent")) {
            if row.contains("cap-name\">audio") || row.contains("cap-name\">embeddedContext") {
                continue;
            }
            assert!(
                !row.contains(DRIVEN),
                "a claim this agent never made has nothing to report: {row}"
            );
        }
    }

    #[test]
    fn each_reachable_row_draws_the_outcome_of_the_capability_it_names() {
        // Driving one capability at a time catches a row wired to the wrong key.
        for (capability, key) in REACHED {
            let panel = rendered_driven(DrivenRecord::from_iter([(key, turned_down())]));

            let row = row_for(&panel, capability);
            assert!(
                row.contains(&format!(r#"{DRIVEN} called · refused"#))
                    && row.contains("Method not found"),
                "{capability} says what became of driving it, in the agent's own words: {row}"
            );
            for (other, _) in REACHED.iter().filter(|(other, _)| *other != capability) {
                let row = row_for(&panel, other);
                assert!(
                    row.contains(&format!(r#"{DRIVEN} not called"#)),
                    "and {other} is a row nobody drove: {row}"
                );
            }
        }
    }

    #[test]
    fn every_fourth_fact_and_every_absence_of_one_is_carried_by_words() {
        // Outcomes and their absence are carried in words, never hidden. Both
        // fixtures, since the agent that claimed nothing has different rows.
        for agent in [everything(), nothing()] {
            let panel = rendered_with(
                Some(agent),
                true,
                Some(Restore::Load),
                None,
                None,
                record(turned_down()),
                AuthState::default(),
            );

            let block = advertisement(&panel, "agent");
            for run in runs(&block) {
                // The run's sentence is held to the same rule as a row's.
                let said_of_all = run.contains(CANNOT_ANY);
                assert!(
                    !run.contains(&format!(r#"{UNREACHED} aria-hidden"#)),
                    "what a run says about driving is not hidden either: {run}"
                );

                for row in capability_rows(run) {
                    let Some((_, said)) = row.split_once(DRIVEN) else {
                        // No fourth fact: the row says "not advertised", or its
                        // run says why.
                        assert!(
                            said_of_all || row.contains(&format!(r#"{STATE}not advertised"#)),
                            "a row with no fourth fact says in words why it has none: {row}"
                        );
                        continue;
                    };
                    let said = said
                        .split_once("</span>")
                        .expect("the fourth fact is closed")
                        .0;
                    assert!(
                        said.trim().len() > 1,
                        "the fourth fact is a sentence rather than a class: {row}"
                    );
                    assert!(
                        !row.contains(r#"data-slot="driven" aria-hidden"#),
                        "and it is not hidden from the reader who needs it most: {row}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_clients_own_block_gains_no_fourth_fact() {
        // The record is about agents, so the client's rows gain nothing, and
        // the two accounts stay the same markup. `style.rs` forbids the rule.
        let panel = rendered_driven(record(turned_down()));
        let client = advertisement(&panel, "client");

        assert!(
            !client.contains(DRIVEN),
            "the client's claims have nobody to have driven them: {client}"
        );
        for row in capability_rows(&client) {
            assert!(
                row.contains(STATE),
                "drawn as plainly as the agent's: {row}"
            );
        }
    }
}
