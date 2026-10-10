//! Drawing one advertiser's account: the count, the narrowing chips, and the
//! runs of claim rows.

use acp_inspector_core::Driven;
use dioxus::prelude::*;

use super::Advertiser;
use super::rows::{Claim, Claimed, Fourth, Shape};

/// What an account says at a glance: how much of it was claimed, and whether
/// anything claimed was refused when somebody took it up.
///
/// **A count is not a grade** (§9). It says how many of the rows below say
/// *advertised*, which is the arithmetic of the list a reader would otherwise
/// do by scrolling it; it scores nothing, orders nothing, and the refusals are
/// counted rather than judged — one refusal is a finding to go and read, not a
/// mark against the agent.
struct Counted {
    advertised: usize,
    total: usize,
    refused: usize,
}

/// Which of an account's rows are on screen.
///
/// **Thirteen rows is a table, and a table is read with a question.** The two a
/// reader brings are *what did it not claim* and *what did it refuse* — and
/// answering either meant reading all thirteen and holding the answer in your
/// head. Narrowing is presentation and nothing else: the count on the summary
/// line goes on counting the whole account, because that count is the agent's
/// answer rather than this control's.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Only {
    #[default]
    Everything,
    Advertised,
    Unadvertised,
    Refused,
}

impl Only {
    /// The four, in the order the chips are drawn.
    const ALL: [Self; 4] = [
        Self::Everything,
        Self::Advertised,
        Self::Unadvertised,
        Self::Refused,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Everything => "All",
            Self::Advertised => "Advertised",
            Self::Unadvertised => "Not advertised",
            Self::Refused => "Refused",
        }
    }

    fn keeps(self, claim: &Claim) -> bool {
        match self {
            Self::Everything => true,
            Self::Advertised => claim.advertised,
            Self::Unadvertised => !claim.advertised,
            Self::Refused => matches!(claim.fourth, Some(Fourth::Outcome(Driven::Refused(_)))),
        }
    }

    /// How many of an account's rows this would leave.
    fn counts(self, groups: &[(&'static str, Vec<Claimed>)]) -> usize {
        groups
            .iter()
            .flat_map(|(_, runs)| runs.iter())
            .flat_map(|run| run.claims.iter())
            .filter(|claim| self.keeps(claim))
            .count()
    }
}

fn counting(groups: &[(&'static str, Vec<Claimed>)]) -> Counted {
    let claims = || {
        groups
            .iter()
            .flat_map(|(_, runs)| runs.iter())
            .flat_map(|run| run.claims.iter())
    };

    Counted {
        advertised: claims().filter(|claim| claim.advertised).count(),
        total: claims().count(),
        refused: claims()
            .filter(|claim| matches!(claim.fourth, Some(Fourth::Outcome(Driven::Refused(_)))))
            .count(),
    }
}

/// One advertiser's account of itself: the claims, grouped the way the protocol
/// groups them and split by the shape each was made in.
///
/// **The same rows for both of them**, which is the whole of drawing one as
/// plainly as the other: one function, one list markup, one statement of shape,
/// so a stylesheet cannot make either account the footnote of the other.
/// What a narrowing is called in a sentence rather than on a chip.
fn which_said(only: Only) -> &'static str {
    match only {
        Only::Everything => "every claim",
        Only::Advertised => "the claims this agent advertised",
        Only::Unadvertised => "the claims it did not advertise",
        Only::Refused => "the claims it advertised and then refused",
    }
}

pub(super) fn advertises(
    by: Advertiser,
    groups: Vec<(&'static str, Vec<Claimed>)>,
    mut only: Signal<Only>,
) -> Element {
    // The headline the summary carries, counted before the rows are consumed
    // and before any of them are narrowed away: it is the agent's answer about
    // itself, and a count that moved when a chip was pressed would be this
    // window's answer about its own filter.
    let counted = counting(&groups);
    // Which chips there are to press, by what each would leave. A chip that
    // narrowed to nothing is a control that does nothing, and one reading
    // `Refused 0` is the empty state's sentence in a shape that means *there
    // are some* (ADR 0006).
    let chips: Vec<(Only, usize)> = Only::ALL
        .into_iter()
        .map(|which| (which, which.counts(&groups)))
        .filter(|(which, count)| *count > 0 || *which == Only::Everything)
        .collect();
    let narrowed = only();
    let showing = narrowed.counts(&groups);
    // Narrowing takes rows off the screen, so the rows that are left are the
    // ones this asked for and never all of them (§9's Console rule, which this
    // is the second surface to state): a run whose every row went is not a run
    // the agent did not make.
    let groups: Vec<_> = groups
        .into_iter()
        .map(|(group, runs)| {
            let runs: Vec<_> = runs
                .into_iter()
                .filter_map(|mut run| {
                    run.claims.retain(|claim| narrowed.keeps(claim));
                    (!run.claims.is_empty()).then_some(run)
                })
                .collect();
            (group, runs)
        })
        .filter(|(_, runs)| !runs.is_empty())
        .collect();
    // What a whole run says once instead of what every row in it said
    // separately, decided before the markup because a run states it above its
    // rows and the rows have to know it was said (§7.7).
    let groups: Vec<_> = groups
        .into_iter()
        .map(|(group, runs)| {
            let runs: Vec<_> = runs
                .into_iter()
                .map(|run| match run.shared_reason() {
                    Some(why) => (run.hushed(), Some(why)),
                    None => (run, None),
                })
                .collect();
            (group, runs)
        })
        .collect();
    rsx! {
        div {
            class: "rail-group advertisement",
            "data-claim": "advertised",
            // Which of the two accounts this block is, for a test to find one
            // by. Data rather than a class because no rule may read it: the two
            // are the same rows at the same weight (#86), and the stylesheet is
            // held to that in `style.rs`.
            "data-advertiser": "{by.whose()}",
            div { class: "rail-head",
                span { class: "eyebrow", "{by.title()}" }
                // **The answer the rows are read to get**, on the line that
                // opens them: how many of the claims below were made, and — the
                // one thing a count cannot say — how many were refused when
                // something drove them (§7.7).
                span { class: "rail-count", "data-slot": "claim-count",
                    "{counted.advertised}/{counted.total}"
                    if counted.refused > 0 {
                        span { class: "claim-refused", "data-slot": "claim-refused",
                            " · {counted.refused} refused"
                        }
                    }
                }
            }
            p { class: "hint", {by.caption()} }

            // The question the table is read with, as chips over it. Only where
            // there is more than one to press: an account whose every row is
            // advertised has one answer, and a row of chips offering to narrow
            // to it is chrome.
            if chips.len() > 1 {
                div { class: "setting-values", role: "group", aria_label: "Narrow the claims",
                    for (which , count) in chips.iter().copied() {
                        button {
                            key: "{which.label()}",
                            class: "chip",
                            r#type: "button",
                            aria_pressed: which == narrowed,
                            onclick: move |_| only.set(which),
                            "{which.label()} {count}"
                        }
                    }
                }
            }
            // **A narrowed surface says what it is hiding**, so that a hidden
            // row and a claim the agent never made never look alike.
            if narrowed != Only::Everything {
                p { class: "hint", "data-slot": "narrowed", role: "status",
                    "Showing {showing} of {counted.total}: {which_said(narrowed)}."
                    button {
                        class: "chip",
                        r#type: "button",
                        onclick: move |_| only.set(Only::Everything),
                        "show all"
                    }
                }
            }

            div { "data-slot": "advertisement-rows",
                for (group , runs) in groups {
                    section { key: "{group}", class: "caps-group",
                        div { class: "caps-group-head",
                            span { class: "caps-group-name top", "{group}" }
                            span { class: "caps-group-rule", aria_hidden: "true" }
                        }
                        for (run , shared) in runs {
                            div { key: "{run.at}",
                                div { class: "caps-group-head",
                                    span { class: "caps-group-name", "{run.at}" }
                                    span { class: "caps-group-rule", aria_hidden: "true" }
                                    span { class: "caps-group-count", {shaped(run.shape)} }
                                }
                                // The shape a run of claims was made in, said
                                // rather than left to be inferred from the field
                                // names: a reader who cannot see the shape reads
                                // their own agent's `initialize` result as
                                // inconsistent rather than as conformant.
                                {stated(run.shape, run.at)}
                                // The reason nothing here drives a single row of
                                // this run, said once above them (§7.7) rather
                                // than word for word on each, which made one
                                // fact about this tool look like three about the
                                // agent.
                                if let Some(why) = shared {
                                    p { class: "hint", "data-slot": "unreached",
                                        "{why.said_of_all()}"
                                    }
                                }
                                {rows_of(run.claims)}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One run's claims, as the rows the drawing gives a capability list: the name
/// the protocol calls it, the answer at the far end, and what else is known
/// under both.
fn rows_of(claims: Vec<Claim>) -> Element {
    rsx! {
        // A list, and the platform's own: what a run of claims is is a list of
        // them, and a reader hearing this screen is told how many there are
        // before the first one rather than counting rows as they arrive.
        ul { class: "caps", "data-slot": "capabilities",
            for claim in claims {
                li {
                    key: "{claim.name}",
                    class: if claim.advertised { "cap yes" } else { "cap no" },
                    span { class: "cap-name", "{claim.name}" }
                    // The answer this screen exists to give, in text (§7.5,
                    // #83). Both halves are drawn at the same weight and the
                    // same size: the half that is an absence is not an aside.
                    span { class: "cap-answer", "data-slot": "state",
                        if claim.advertised { "advertised" } else { "not advertised" }
                    }
                    // And the two facts under them: the field the claim was made
                    // in, where it is not the name already said, and what became
                    // of driving it — or why nothing here can (§7.7).
                    if claim.field != claim.name || claim.fourth.is_some() {
                        span { class: "cap-said",
                            if claim.field != claim.name {
                                span { class: "cap-field", "{claim.field}" }
                            }
                            if claim.field != claim.name && claim.fourth.is_some() {
                                span { aria_hidden: "true", " · " }
                            }
                            if let Some(fourth) = &claim.fourth {
                                span { "data-slot": "driven", {came_back(fourth)} }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The shape a run was made in, in the two words a count column has room for.
fn shaped(shape: Shape) -> &'static str {
    match shape {
        Shape::Flag => "boolean",
        Shape::Object => "object",
        Shape::List => "list",
    }
}

/// Whether the advertisement was ever driven on this connection and what came
/// back — or, where nothing here can drive it, why not (§7.7).
///
/// **Weighted like the rest of the row and no louder.** A refusal gets no
/// colour of its own, no count and no ordering: it is the third thing that
/// happened rather than the bad one, and an agent that advertised a capability
/// and then refused it is a finding the reader draws rather than a grade this
/// tool awards.
///
/// The agent's own error is what a refusal says, whole. A window that
/// paraphrased it would be standing between the reader and the wire — the rule
/// the auth block's refusal already follows.
///
/// The leading space keeps the visible state and this fact separate when a
/// non-visual reader composes the row's text.
fn came_back(fourth: &Fourth) -> Element {
    match fourth {
        Fourth::Outcome(Driven::Unasked) => rsx! { " not called" },
        Fourth::Outcome(Driven::Answered) => rsx! { " called · OK" },
        Fourth::Outcome(Driven::Refused(error)) => rsx! { " called · refused: {error}" },
        // `Driven` is non-exhaustive, and an outcome this window has not been
        // taught must not be drawn as one it has.
        Fourth::Outcome(_) => rsx! { " called, with an outcome this window has no rendering for" },
        Fourth::Unreached(why) => rsx! { "{why.said()}" },
    }
}

/// How a run of claims was made, in the protocol's own terms.
///
/// Said rather than left to be inferred from the field names: a reader who
/// cannot see the shape reads their own agent's `initialize` result as
/// inconsistent rather than as conformant.
fn stated(shape: Shape, at: &'static str) -> Element {
    rsx! {
        p { class: "hint shape", "data-slot": "shape",
            {match shape {
                Shape::Flag => rsx! {
                    "boolean on "
                    code { "{at}" }
                    "; "
                    code { "true" }
                    " claims"
                },
                Shape::Object => rsx! {
                    "object under "
                    code { "{at}" }
                    "; "
                    code { "{{}}" }
                    " claims too"
                },
                Shape::List => rsx! {
                    "list at "
                    code { "{at}" }
                    "; an entry claims, empty none"
                },
            }}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::fixtures::*;

    #[test]
    fn a_capability_row_says_in_text_whether_the_agent_advertised_it() {
        // The state is text-borne, the way tool-call and plan status are: an
        // unlit row that reached a reader as a bare capability name would have
        // handed them the name and kept the answer (#83). Both fixtures,
        // because a row that said "not advertised" whatever the agent claimed
        // would pass a test that only ever asked the agent that claimed
        // nothing.
        for (capabilities, state) in [(everything(), "advertised"), (nothing(), "not advertised")] {
            // The whole span, because "advertised" is a substring of "not
            // advertised" and a test that only asked for the shorter one would
            // read every row as the answer it hoped for.
            let said = format!(r#"{STATE}{state}</span>"#);
            let panel = shown(capabilities, true);
            // The agent's rows, because the fixture is an agent: this client's
            // own claim is drawn beside them and says what *it* advertised
            // (#86), which is not this agent's answer to anything.
            let agent = advertisement(&panel, "agent");
            let rows = capability_rows(&agent);

            assert!(!rows.is_empty(), "there are capability rows to read");
            for row in rows {
                assert!(row.contains(&said), "the row says its state: {row}");
            }
        }
    }
}
