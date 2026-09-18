//! The daisyUI form shell: the finding summary, the body, and the submit button inside the
//! adapter-owned form element.

use dioxus::prelude::*;
use schemaform_dioxus::render::{Affordance, AffordanceKind};
use schemaform_dioxus::{ShellContext, ShellRenderer};

use super::Appearance;
use crate::components::button::{Button, ButtonColor};

/// Lays the form's contents out as a vertical grid — the finding summary, then the body, then a
/// daisyUI button for the submit affordance: primary when gated, warning when advisory.
///
/// The button is `type="submit"`, so it submits through the adapter-owned form element and
/// pressing Enter in a text control takes the same path. The summary arrives with its
/// adapter-owned region wrapper; this component's finding presenter ([`super::findings`]) is what
/// frames the findings inside it as an alert, since the shell cannot see whether there are any.
/// The [`Appearance`] axis switches the grid and the button's width utility off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DaisyuiShell {
    appearance: Appearance,
}

impl DaisyuiShell {
    /// The same shell at `appearance`.
    pub fn appearance(self, appearance: Appearance) -> Self {
        Self { appearance }
    }
}

impl ShellRenderer for DaisyuiShell {
    fn shell(&self, context: ShellContext) -> Element {
        let submit = context.submit;
        let appearance = self.appearance;
        rsx! {
            ShellContents { submit, appearance, summary: context.summary, body: context.body }
        }
    }
}

/// The adapter exposes mode on the shell affordance, but not on finding contexts.
/// A reactive scope lets both presenter slots and Field supplements follow a mode
/// change on the mounted form. A host using its own shell can provide the same scope.
#[derive(Clone, Copy)]
pub struct AdvisoryPresentation(pub ReadSignal<bool>);

pub(super) fn advisory_presentation() -> bool {
    try_consume_context::<AdvisoryPresentation>().is_some_and(|mode| (mode.0)())
}

#[component]
fn ShellContents(
    submit: Affordance,
    appearance: Appearance,
    summary: Element,
    body: Element,
) -> Element {
    let advisory = submit.kind == AffordanceKind::AdvisorySubmit;
    let mode = use_memo(use_reactive!(|(advisory)| advisory));
    use_context_provider(|| AdvisoryPresentation(mode.into()));
    rsx! {
        div { class: appearance.utilities("grid gap-4"), "data-schemaform-daisyui": "shell",
            {summary}
            {body}
            Button {
                id: submit.id.clone(),
                r#type: "submit",
                color: if advisory { ButtonColor::Warning } else { ButtonColor::Primary },
                class: appearance.utilities("w-fit"),
                "aria-label": submit.accessible_name.clone(),
                "{submit.label}"
            }
        }
    }
}
