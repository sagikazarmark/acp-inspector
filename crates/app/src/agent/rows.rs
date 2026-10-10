//! A capability row: the claim, the shape it was made in, and its fourth fact.

use acp_inspector_core::{AgentCapability, AuthState, Driven, DrivenRecord};

/// The fourth fact a capability row carries, which is one of two things (§7.7).
///
/// **Never both, and never one dressed as the other.** *Not driven* on a row no
/// affordance reaches is a sentence about the agent when the true sentence is
/// about this tool, and a reader who takes it at face value goes hunting for a
/// button that never existed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Fourth {
    /// Something here can drive the advertisement, and this is what became of
    /// driving it — core's record for every capability but one, and the auth
    /// state for that one.
    Outcome(Driven),
    /// Nothing here can drive it, and this is why.
    Unreached(Unreached),
}

/// Why nothing in this tool drives an advertisement (§7.7).
///
/// **A fact about where the affordances live, not a state a store holds.**
/// Core's record answers what became of driving a capability; it cannot answer
/// whether anything here *could*, because that is a question about this window
/// — which is why these are decided beside the rows and written down as an
/// enumeration rather than looked up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unreached {
    /// `session/resume` where `session/load` is advertised too: the restore
    /// property prefers load ([`Restore::preferred`]), so a resume is never sent
    /// against such an agent and the row would otherwise read *not driven*
    /// forever (§7.5).
    LoadPreferred,
    /// `session/close` or `session/delete` where `session/list` is not: both
    /// affordances hang off a listing row, so there is no row to press them on
    /// — the consequence §7.5 records, made visible where it bites.
    NoListingRow,
}

impl Unreached {
    /// The half of the sentence that is the reason, without the correction in
    /// front of it.
    ///
    /// Split out because the correction is said once wherever the reason is
    /// shared — a run of rows nothing here reaches states it above them — and
    /// the two forms must not be able to drift into saying different things
    /// about the same limit.
    fn because(self) -> &'static str {
        match self {
            Self::LoadPreferred => "it prefers session/load, which this agent advertised too",
            Self::NoListingRow => {
                "it is pressed on a listing row, and this agent advertised no session/list"
            }
        }
    }

    /// What the row says instead of an outcome.
    ///
    /// Every one of them says *this tool cannot drive it* first and the reason
    /// second, because the first half is the correction a reader needs — the
    /// blankness is the inspector's and not their agent's — and the second half
    /// is what tells them whether it is a limit they can do anything about.
    ///
    /// The leading space is the one [`came_back`] takes, for the same reason.
    pub(super) fn said(self) -> String {
        format!(" this tool cannot drive it: {}", self.because())
    }

    /// The same sentence about a whole run of rows, said above them.
    ///
    /// **The correction is not dropped, only said once** (§7.7): a reader who
    /// meets three rows and no outcome still has to be told the blankness is
    /// this tool's, and it is the plural that makes it one statement about the
    /// run rather than the first of three identical ones. No leading space,
    /// because it is a sentence of its own rather than the fourth thing a row
    /// composes.
    pub(super) fn said_of_all(self) -> String {
        format!("this tool cannot drive any of these: {}", self.because())
    }
}

/// The shapes ACP v1 states an advertisement in.
///
/// The first two are kept apart rather than flattened into one notion of
/// support (§7.5). `session/load` is gated by a top-level boolean while resume,
/// close, delete, list and `additionalDirectories` are gated by the *presence*
/// of a capability object under `sessionCapabilities` — and the schema records
/// that the asymmetry is expected to be unified in a later protocol version.
/// Until it is, an agent author reading this screen should be able to see why
/// their own agent advertises support in two different shapes.
///
/// The third is neither, and is here because an Advertisement is wider than a
/// capability (`CONTEXT.md`, *Advertisement*): `authenticate` is gated on the
/// `authMethods` the agent listed, which is a list on the `initialize` result
/// beside the capability object rather than anything inside it (§7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// A boolean field, where `false` and absent are the same claim.
    Flag,
    /// A capability object, where the presence of the object is the claim and
    /// an empty one advertises as loudly as a full one.
    Object,
    /// A list, where an entry is the claim and an empty list is no claim at
    /// all.
    List,
}

/// A run of claims made in one shape at one place in the `initialize` result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Claimed {
    /// The shape every claim in the run was made in.
    pub(super) shape: Shape,
    /// Where the run lives, by the name the protocol gives that object.
    pub(super) at: &'static str,
    pub(super) claims: Vec<Claim>,
}

impl Claimed {
    /// The reason nothing in this tool drives *any* row of the run, where every
    /// row of it has the same one (§7.7).
    ///
    /// **A run and not a group**, because a run is what shares a shape and a
    /// place in the `initialize` result, and the sentence hoisted here is about
    /// where the affordances are rather than about the agent: the three prompt
    /// content shapes are one run and one limit, and so are the two MCP
    /// transports. The session lifecycle is not — an unreachable `session/close`
    /// sits beside a `session/list` that was driven — so it keeps the reason on
    /// the rows that have one, which is the answer for a mixed run.
    ///
    /// **Two or more, never one.** A run of a single row has nothing to say
    /// twice, and the plural sentence a hoist puts above it would be reporting a
    /// pattern where there is one fact.
    pub(super) fn shared_reason(&self) -> Option<Unreached> {
        let mut reasons = self.claims.iter().map(|claim| match claim.fourth {
            Some(Fourth::Unreached(why)) => Some(why),
            _ => None,
        });
        let first = reasons.next().flatten()?;
        (self.claims.len() > 1 && reasons.all(|why| why == Some(first))).then_some(first)
    }

    /// The same run with the fourth fact taken off its rows, for when the run
    /// itself is about to say it.
    ///
    /// The fact is removed rather than merely undrawn: a row that kept it in the
    /// markup and hid it would be the dimness rule broken by another name, and
    /// what a reader — or a screen reader — meets is a run whose rows report no
    /// outcome under a sentence saying why none of them can.
    pub(super) fn hushed(mut self) -> Self {
        for claim in &mut self.claims {
            claim.fourth = None;
        }
        self
    }
}

/// One advertisement: what it is called, where the claim is read from, whether
/// this agent made it, and — where the agent made it — what happened when
/// somebody took it up, or why nothing here can take it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Claim {
    /// What the protocol calls the capability — the method it gates where it
    /// gates one, the field name where it gates a field.
    pub(super) name: &'static str,
    /// The field the claim itself is made in, which for the session lifecycle
    /// is not the method name it gates.
    pub(super) field: &'static str,
    pub(super) advertised: bool,
    /// What became of driving the advertisement, or why nothing here can
    /// (§7.7) — and `None` on a row that says nothing about driving at all.
    ///
    /// **A fourth element of the same row, not a lane of its own**: no colour
    /// for a refusal, no count, no ordering. Which half a row is stays carried
    /// by weight on screen and by words in the accessibility tree, as
    /// advertised/not-advertised already is.
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

    /// A capability the protocol calls by the very field it is claimed in,
    /// which is every one of them outside the session lifecycle.
    pub(super) fn self_named(name: &'static str, advertised: bool) -> Self {
        Self::new(name, name, advertised)
    }

    /// The row's fourth fact, read out of core's record (§7.7).
    ///
    /// **The capability is named rather than matched**: the record is keyed on
    /// [`AgentCapability`], so a row asks about a variant that has to exist —
    /// a key this file matched loosely would render a capability upstream adds
    /// as *unknown* instead of breaking a build, which is what the schema
    /// tripwire behind these lists exists to prevent.
    pub(super) fn driven(self, record: &DrivenRecord, capability: AgentCapability) -> Self {
        self.if_advertised(Fourth::Outcome(record.of(capability)))
    }

    /// The auth row's fourth fact, read out of the auth state (§7.7).
    ///
    /// **The one row that does not ask the record, because `authenticate` has
    /// no entry in it** — the argument is at the top of this file and the
    /// mapping is [`AuthState::driven`]. The block below these rows reads the
    /// same value, so the two cannot say different things about one login.
    pub(super) fn logged_in(self, auth: &AuthState) -> Self {
        self.if_advertised(Fourth::Outcome(auth.driven()))
    }

    /// Why nothing in this tool reaches *this agent's* claim (§7.7).
    ///
    /// Which agent it is decides it: a `session/resume` is unreachable only
    /// beside an advertised `session/load`, and a `session/close` only where
    /// there is no listing to press it on.
    pub(super) fn unreached(self, why: Unreached) -> Self {
        self.if_advertised(Fourth::Unreached(why))
    }

    /// [`driven`](Self::driven), [`logged_in`](Self::logged_in) and
    /// [`unreached`](Self::unreached) all go through here, and
    /// [`undrivable`](Self::undrivable) pointedly does not.
    ///
    /// **A row the agent did not advertise says nothing about driving** (§7.7).
    /// There is no claim to have driven, and *not driven* about a capability
    /// nobody offered is an answer to a question nobody asked — the row's
    /// answer is the *not advertised* it already gives.
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

    /// Why nothing here drives a single row of a run, said once above them.
    const UNREACHED: &str = r#"data-slot="unreached""#;

    /// The run the row naming `capability` belongs to, out of the agent's own
    /// block.
    fn run_for(panel: &str, capability: &str) -> String {
        runs(&advertisement(panel, "agent"))
            .into_iter()
            .find(|run| run.contains(capability))
            .unwrap_or_else(|| panic!("the {capability} row is in a run: {panel}"))
            .to_owned()
    }

    /// A refusal in the agent's own words, as the record would have carried
    /// one.
    fn turned_down() -> Driven {
        Driven::Refused(CallError::Rejected(v1::Error::method_not_found()))
    }

    /// The record of an agent whose `session/load` was driven, and what came of
    /// it.
    fn record(outcome: Driven) -> DrivenRecord {
        DrivenRecord::from_iter([(AgentCapability::Load, outcome)])
    }

    /// Every row an affordance reaches on an agent that advertised everything,
    /// and the capability each of them is the record's key for (§7.7).
    ///
    /// **`session/resume` is not among them, and its absence is the ring's last
    /// decision** (§7.5): an agent that advertised load too never gets a resume
    /// sent to it, so on this fixture that row says why rather than reporting an
    /// outcome. Where resume *is* what this tool would send, it records like the
    /// rest — which is
    /// `an_agent_that_advertised_resume_alone_records_outcomes_on_the_row`.
    ///
    /// `authenticate` is not here either, for the other reason: it is a row an
    /// affordance reaches and it has no entry in this record at all, because
    /// `AuthState` is its record (§7.7).
    ///
    /// Stated by name rather than derived from `AgentCapability::ALL`, because
    /// what is being asserted is the *wiring*: a row that asked the record about
    /// the wrong capability would report one agent's answer on another's row,
    /// and a list computed from the same source as the panel's could not catch
    /// it.
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

    /// What a row says where nothing in this tool can drive it (§7.7), and the
    /// whole of what those rows have in common: the sentence is about the tool.
    const CANNOT: &str = " this tool cannot drive it";

    /// The same sentence about a whole run, which is where it is said when
    /// every row of the run has the one reason (§7.7).
    const CANNOT_ANY: &str = "this tool cannot drive any of these";

    /// Every capability row nothing in this tool can drive, however the panel
    /// says so: on the row, or once above the run the row belongs to.
    ///
    /// One list rather than two, because the rule is about what a *reader* is
    /// told — a row whose run says why is a row that says why, and a test that
    /// counted only the row-borne form would pass a panel that had quietly
    /// stopped saying it about five of them.
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

    /// Everything a row says when it *is* reporting an outcome — so a test can
    /// assert a row says none of them.
    ///
    /// Every arm [`came_back`] has, the outcome this window has no rendering for
    /// included: a row that leaked *that* onto an unreached row would be
    /// reporting an outcome as surely as the three named ones.
    const OUTCOMES: [&str; 4] = [
        " not called",
        " called · OK",
        " called · refused",
        " called, with an outcome",
    ];

    /// The `session/load` row of the agent's own block.
    fn load_row(panel: &str) -> String {
        row_for(panel, "session/load")
    }

    #[test]
    fn the_load_row_says_whether_the_advertisement_was_ever_driven() {
        // The fourth fact (§7.7): a capability nobody pressed the button on and
        // one the agent honoured stop looking the same on the screen built to
        // audit its claims. All three outcomes, because a row that said one
        // thing whatever the record held would pass a test that only ever asked
        // about one of them.
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

        // And a refusal carries the agent's own error, which is the reason to
        // draw it rather than a paraphrase of it.
        let refused = load_row(&rendered_driven(record(turned_down())));
        assert!(
            refused.contains("Method not found"),
            "in the agent's own words: {refused}"
        );
    }

    #[test]
    fn a_refusal_is_a_fourth_element_of_the_same_row_and_no_louder() {
        // **No new panel, no lane, no colour of its own** (§7.7). It is read
        // after what the row already said, in the same row, at the same weight:
        // a refusal is the third thing that happened rather than the bad one,
        // and an agent that advertised a capability and then refused it is a
        // finding the reader draws rather than a grade this tool awards. The
        // stylesheet holds the other half of this (`style.rs`).
        let row = load_row(&rendered_driven(record(turned_down())));

        let state = row.find(STATE).expect("the row says its state");
        let driven = row.find(DRIVEN).expect("and what became of driving it");
        assert!(state < driven, "the fourth fact is the fourth thing: {row}");
        assert!(
            !row.contains("bad"),
            "and it is not painted as a failure: {row}"
        );
    }

    /// Every capability row of the agent's own block that reports an outcome.
    fn reporting(panel: &str) -> Vec<String> {
        capability_rows(&advertisement(panel, "agent"))
            .into_iter()
            .filter(|row| OUTCOMES.iter().any(|outcome| row.contains(outcome)))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_row_an_affordance_reaches_reports_an_outcome_and_no_other_row_does() {
        // The rows something in this tool sends a call for (§7.7). The rows
        // nothing here can drive report no outcome, because *not driven* on one
        // of those would be a sentence about the agent when the truth is about
        // this tool — and a row wired to nothing would be reporting the same
        // thing by accident.
        //
        // `authenticate` is the one this list does not carry, because it is the
        // one whose outcome is not in that record at all: `AuthState` is its
        // record (§7.7), and the row is reached all the same.
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
        // **The sentence is about this tool** (§7.7). Six rows on an agent that
        // advertised everything: the resume this tool never sends because it
        // prefers load, and the five it cannot drive on any agent at all. Each
        // says so where the outcome would have been — on the row, or, where
        // every row of a run has the one reason, once above the run — so a
        // reader stops hunting for a button that never existed.
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
        // **The restore property prefers load** (§7.5), so a resume is never
        // sent against such an agent — and a row reading *not driven* forever
        // would read as the agent being left untested by accident rather than as
        // the tool decision it is.
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

    /// An agent that reopens sessions the one way this tool would then send.
    fn only_resuming() -> v1::InitializeResponse {
        described(v1::AgentCapabilities::new().session_capabilities(
            v1::SessionCapabilities::new().resume(v1::SessionResumeCapabilities::new()),
        ))
    }

    #[test]
    fn an_agent_that_advertised_resume_alone_records_outcomes_on_the_row() {
        // The other half of the pair: where resume is what an agent's
        // advertisement entitles this tool to send, the row is one an affordance
        // reaches and it records like every other (§7.7).
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

    /// An agent that ends the sessions it holds and will not name one.
    fn ending_without_listing() -> v1::InitializeResponse {
        described(
            v1::AgentCapabilities::new().session_capabilities(
                v1::SessionCapabilities::new()
                    .close(v1::SessionCloseCapabilities::new())
                    .delete(v1::SessionDeleteCapabilities::new()),
            ),
        )
    }

    /// The same agent, listing them too.
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
        // **The consequence §7.5 records, made visible where it bites** (§7.7):
        // both affordances hang off a listing row, so an agent that advertised
        // neither listing has both claims displayed and nowhere to drive them
        // from.
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
        // The same two claims on an agent that names its sessions: there is a
        // row to press them on, so the fourth fact is what became of pressing
        // it.
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
        // **Whether or not the agent advertised them** (§7.7), because the
        // sentence is about this tool: the composer sends one text block and
        // The MCP editor supplies stdio only, and an agent's silence does not change
        // either. Both fixtures, because a run that said it whatever it was
        // handed would pass a test that only ever asked one of them.
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
        // **One fact about this tool, not three about the agent** (§7.7). The
        // three prompt content shapes share one reason and the two MCP
        // transports share another, and a rail 280px wide carrying the same
        // sentence three times over is what made the reason read as a property
        // of each row.
        //
        // The rows are checked for silence rather than the sentence for being
        // singular, because that is the thing that would come back: a hoist
        // that left the row-borne copy in place would read as one statement and
        // still be drawn as four.
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
        // And the mixed run keeps it on the row that has one: an unreachable
        // `session/resume` sits beside a `session/list` something here drives,
        // so there is nothing for the run to say about all of them.
        let sessions = run_for(&panel, "session/resume");
        assert!(
            !sessions.contains(CANNOT_ANY) && sessions.contains(CANNOT),
            "a run whose rows do not share the reason keeps it on the row: {sessions}"
        );
    }

    #[test]
    fn a_row_the_agent_did_not_advertise_says_nothing_about_driving() {
        // There is no claim to have driven, so there is nothing to say: *not
        // driven* about a capability nobody offered is an answer to a question
        // nobody asked, and the row's answer is the *not advertised* it already
        // gives (§7.7).
        //
        // The five this tool cannot drive on any agent say so whatever the
        // agent claimed — but they say it above their run now rather than on
        // the row, so there is nothing left here to except: every row in this
        // block is silent about driving, and the runs are where the exception
        // reads.
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
        // **The wiring, row by row** (§7.7): the record is keyed on a capability
        // and a row asks it about one, so a row asking about the wrong one would
        // report an answer the agent gave about a different call. Driving one at
        // a time is what tells that apart — every row lit at once would pass
        // whatever each of them asked for.
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
        // **Not by dimness** (§7.7, and #83's rule about the half of an answer
        // that is an absence). The state above it is visible text; nothing
        // repeats *this*, so it is text too. A reader who gets neither colour
        // nor weight still reads an outcome, or reads that there is none and
        // why.
        //
        // Both fixtures, because an agent that claimed nothing draws the five
        // rows this tool cannot drive on any agent and nothing else, and a row
        // that was words on one fixture and a bare class on the other would pass
        // half a test.
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
                // The run's own sentence is held to the same rule as a row's:
                // the reason five rows have no fourth fact is read there now,
                // and a reason hidden from the accessibility tree would be the
                // dimness rule broken one level up.
                let said_of_all = run.contains(CANNOT_ANY);
                assert!(
                    !run.contains(&format!(r#"{UNREACHED} aria-hidden"#)),
                    "what a run says about driving is not hidden either: {run}"
                );

                for row in capability_rows(run) {
                    let Some((_, said)) = row.split_once(DRIVEN) else {
                        // The absence, which is the other half of the rule: a
                        // row that says nothing about driving says why in words
                        // it already had — the agent never made the claim — or
                        // its run says why for all of them.
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
        // **Both advertisers stay drawn identically** (#86, §7.7): the record is
        // about what an *agent* did when it was driven, so the client's rows
        // carry no such fact — and that must not become a difference in markup
        // or styling that tells the two blocks apart. `style.rs` forbids the
        // rule; this forbids the row.
        let panel = rendered_driven(record(turned_down()));
        let client = advertisement(&panel, "client");

        assert!(
            !client.contains(DRIVEN),
            "the client's claims have nobody to have driven them: {client}"
        );
        // And they are the same rows they always were, in the same markup as
        // the agent's beside them.
        for row in capability_rows(&client) {
            assert!(
                row.contains(STATE),
                "drawn as plainly as the agent's: {row}"
            );
        }
    }
}
