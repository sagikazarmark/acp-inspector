//! Drawing one advertiser's account: the count, the narrowing chips, and the
//! runs of claim rows.

use acp_inspector_core::Driven;
use dioxus::prelude::*;

use super::Advertiser;
use super::rows::{Claim, Claimed, Fourth, Shape};

/// An account at a glance: how many rows say advertised, and how many claims
/// were refused when driven. A count, not a grade (§9).
struct Counted {
    advertised: usize,
    total: usize,
    refused: usize,
}

/// Which of an account's rows are on screen. Presentation only: the count on
/// the summary line always covers the whole account.
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

/// What a narrowing is called in a sentence rather than on a chip.
fn which_said(only: Only) -> &'static str {
    match only {
        Only::Everything => "every claim",
        Only::Advertised => "the claims this agent advertised",
        Only::Unadvertised => "the claims it did not advertise",
        Only::Refused => "the claims it advertised and then refused",
    }
}

/// One advertiser's account: the claims grouped as the protocol groups them and
/// split by the shape each was made in. Both accounts go through here, so
/// neither can be styled as the other's footnote.
pub(super) fn advertises(
    by: Advertiser,
    groups: Vec<(&'static str, Vec<Claimed>)>,
    mut only: Signal<Only>,
) -> Element {
    // Counted before narrowing: it is the agent's answer, not the filter's.
    let counted = counting(&groups);
    // A chip that would narrow to nothing is not offered (ADR 0006).
    let chips: Vec<(Only, usize)> = Only::ALL
        .into_iter()
        .map(|which| (which, which.counts(&groups)))
        .filter(|(which, count)| *count > 0 || *which == Only::Everything)
        .collect();
    let narrowed = only();
    let showing = narrowed.counts(&groups);
    // A run whose every row was narrowed away is dropped, so it does not read
    // as a run the agent did not make.
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
    // Decided before the markup: a run that states a shared reason above its
    // rows takes it off the rows (§7.7).
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
            // For tests only; no style rule may read it (see `style.rs`).
            "data-advertiser": "{by.whose()}",
            div { class: "rail-head",
                span { class: "eyebrow", "{by.title()}" }
                // How many claims were made, and how many were refused (§7.7).
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

            // Only where there is more than one chip to press.
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
            // A narrowed surface says what it hides, so a hidden row never
            // looks like a claim the agent did not make.
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
                                // The shape, said: without it a conformant result
                                // reads as inconsistent.
                                {stated(run.shape, run.at)}
                                // The shared reason, said once above the run.
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

/// One run's claims as rows: the protocol name, the answer, then the field and
/// the fourth fact.
fn rows_of(claims: Vec<Claim>) -> Element {
    rsx! {
        // A real list, so a screen reader announces how many rows there are.
        ul { class: "caps", "data-slot": "capabilities",
            for claim in claims {
                li {
                    key: "{claim.name}",
                    class: if claim.advertised { "cap yes" } else { "cap no" },
                    span { class: "cap-name", "{claim.name}" }
                    // The answer in text, both halves at the same weight.
                    span { class: "cap-answer", "data-slot": "state",
                        if claim.advertised { "advertised" } else { "not advertised" }
                    }
                    // The field, where it differs from the name, and the fourth fact.
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

/// What became of driving the advertisement, or why nothing here can (§7.7).
///
/// No colour, count or ordering of its own: a refusal is a finding, not a
/// grade. The agent's error is shown whole. The leading space separates this
/// from the state when the row's text is read aloud.
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

/// How a run's claims were made, in the protocol's terms: without it a
/// conformant `initialize` result reads as inconsistent.
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
        // Both fixtures: a row that always said "not advertised" would pass
        // the one that claimed nothing.
        for (capabilities, state) in [(everything(), "advertised"), (nothing(), "not advertised")] {
            // The whole span: "advertised" is a substring of "not advertised".
            let said = format!(r#"{STATE}{state}</span>"#);
            let panel = shown(capabilities, true);
            // The agent's rows; the client's account beside them is not its answer.
            let agent = advertisement(&panel, "agent");
            let rows = capability_rows(&agent);

            assert!(!rows.is_empty(), "there are capability rows to read");
            for row in rows {
                assert!(row.contains(&said), "the row says its state: {row}");
            }
        }
    }
}
