//! The daisyUI form shell: the finding summary, the body, and the submit button inside the
//! adapter-owned form element.

use dioxus::prelude::*;
use schemaform_dioxus::render::{Affordance, AffordanceKind};
use schemaform_dioxus::{ShellContext, ShellRenderer};

use super::Appearance;
use super::buttons::{ButtonClasses, ButtonRole};
use crate::components::button::{Button, ButtonColor};

/// Lays the form's contents out as a vertical grid — the finding summary, then the body, then a
/// daisyUI button for the submit affordance: primary when gated, warning when advisory.
///
/// The button is `type="submit"`, so it submits through the adapter-owned form element and
/// pressing Enter in a text control takes the same path. The summary arrives with its
/// adapter-owned region wrapper; this component's finding presenter ([`super::findings`]) is what
/// frames the findings inside it as an alert, since the shell cannot see whether there are any.
/// A read-only form (`FormHandle::set_read_only`) submits nothing, so the shell omits the button
/// while `ShellContext::read_only` holds, as the built-in shell does.
///
/// The [`Appearance`] axis switches the grid and the button's width utility off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DaisyuiShell {
    appearance: Appearance,
    buttons: ButtonClasses,
}

impl DaisyuiShell {
    /// The same shell at `appearance`.
    pub fn appearance(self, appearance: Appearance) -> Self {
        Self { appearance, ..self }
    }

    /// The same shell with `buttons` for its submit button.
    pub fn buttons(self, buttons: ButtonClasses) -> Self {
        Self { buttons, ..self }
    }
}

impl ShellRenderer for DaisyuiShell {
    fn shell(&self, context: ShellContext) -> Element {
        let submit = context.submit;
        let appearance = self.appearance;
        let buttons = self.buttons;
        rsx! {
            ShellContents {
                submit,
                read_only: context.read_only,
                appearance,
                buttons,
                summary: context.summary,
                body: context.body,
            }
        }
    }
}

#[component]
fn ShellContents(
    submit: Affordance,
    read_only: bool,
    appearance: Appearance,
    buttons: ButtonClasses,
    summary: Element,
    body: Element,
) -> Element {
    let advisory = submit.kind == AffordanceKind::AdvisorySubmit;
    let (role, color) = if advisory {
        (ButtonRole::AdvisorySubmit, ButtonColor::Warning)
    } else {
        (ButtonRole::Submit, ButtonColor::Primary)
    };
    // A host class replaces the mode's colour; the width utility stays the Appearance axis's.
    let (color, host) = match buttons.class(role) {
        Some(host) => (ButtonColor::Default, host),
        None => (color, ""),
    };
    let layout = appearance.utilities("w-fit");
    rsx! {
        div { class: appearance.utilities("grid gap-4"), "data-schemaform-daisyui": "shell",
            {summary}
            {body}
            if !read_only {
                Button {
                    id: submit.id.clone(),
                    r#type: "submit",
                    color,
                    class: "{host} {layout}",
                    "data-schemaform-button": "submit",
                    "aria-label": submit.accessible_name.clone(),
                    "{submit.label}"
                }
            }
        }
    }
}
