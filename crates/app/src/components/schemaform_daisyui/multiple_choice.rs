//! A multiple choice keeps native checkbox semantics and the adapter's option ids.

use dioxus::prelude::*;
use dioxus_field::FieldContext;
use schemaform_dioxus::{ControlRenderContext, use_multiple_choice_edit};

use super::Appearance;
use super::mapping::field_meta_values;
use super::parts::{
    incompatible_description, label_class, presence_affordances, read_only_field, supplements,
};
use super::shell::advisory_presentation;
use crate::components::field::Field;

#[component]
pub(super) fn MultipleChoiceControl(
    context: ControlRenderContext,
    appearance: Appearance,
) -> Element {
    let edit = use_multiple_choice_edit(&context);
    let Some(projection) = context.node().read().ok().flatten() else {
        return rsx! {};
    };
    let presentation = context.presentation();
    let control = context.control();
    let selected = edit.selected.cloned();
    if projection.read_only {
        let labels = edit
            .options
            .iter()
            .filter(|option| selected.contains(&option.identity))
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return read_only_field(presentation, control, appearance, labels, &[]);
    }
    let field_context =
        FieldContext::empty().with_meta_values(field_meta_values(presentation, control));
    let color = if presentation.invalid {
        "checkbox-error"
    } else {
        ""
    };
    let mut described_by = presentation.described_by().unwrap_or_default();
    // Native options share the Field's error region, whose id `supplements`
    // registers for Field-aware widgets. Raw inputs must project it explicitly.
    let errors_id = (presentation.invalid && !advisory_presentation())
        .then(|| format!("{}-errors", presentation.element_id));
    if let Some(id) = &errors_id {
        if !described_by.is_empty() {
            described_by.push(' ');
        }
        described_by.push_str(id);
    }
    if presentation.incompatible_value.is_some() {
        if !described_by.is_empty() {
            described_by.push(' ');
        }
        described_by.push_str(&format!("{}-incompatible", presentation.element_id));
    }
    let described_by = (!described_by.is_empty()).then_some(described_by);
    let compact = super::density::compact();
    let options = rsx! {
                for option in edit.options.clone() {
                    div { key: "{option.identity.as_str()}", class: appearance.utilities("flex items-center gap-2"),
                        input {
                            id: edit.option_element_id(&option.identity),
                            name: control.name.clone(),
                            r#type: "checkbox",
                            class: "checkbox {color}",
                            checked: selected.contains(&option.identity),
                            disabled: option.disabled,
                            "aria-invalid": presentation.invalid,
                            "aria-describedby": described_by.clone(),
                            "aria-errormessage": errors_id.clone(),
                            oninput: { let identity = option.identity.clone(); move |_| edit.toggle.call(identity.clone()) },
                            onblur: move |_| edit.blur.call(()),
                        }
                        label { r#for: edit.option_element_id(&option.identity), "{option.label}" }
                    }
                }
    };
    rsx! {
        Field { context: field_context, "data-schemaform-daisyui": "multiple-choice",
            "data-schemaform-density": if compact { "compact" } else { "default" },
            if compact {
                div { "data-schemaform-field-header": "", class: appearance.utilities("flex min-w-0 flex-wrap items-center justify-between gap-2"),
                    span { id: format!("{}-legend", presentation.element_id), class: "fieldset-legend {label_class(presentation)}",
                        "{presentation.label}"
                        if control.required { " (required)" }
                    }
                    {presence_affordances(&presentation.presence, appearance)}
                }
            }
            fieldset {
                id: presentation.element_id.clone(),
                class: "fieldset",
                tabindex: "-1",
                "data-schemaform-control": "multiple-choice",
                "data-focus-first-descendant": "",
                "aria-labelledby": compact.then(|| format!("{}-legend", presentation.element_id)),
                if !compact {
                    legend { class: "fieldset-legend {label_class(presentation)}",
                        "{presentation.label}"
                        if control.required { " (required)" }
                    }
                }
                if let Some(status) = control.write_only_status.clone() {
                    output { "data-write-only-status": "", "{status}" }
                }
                if compact {
                    div { "data-schemaform-options": "", class: appearance.utilities("flex flex-wrap items-center gap-2"), {options} }
                } else { {options} }
                {incompatible_description(presentation, appearance)}
                {supplements(presentation, appearance)}
            }
        }
    }
}
