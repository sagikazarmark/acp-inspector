//! The daisyUI boolean control.

use std::rc::Rc;

use dioxus::prelude::*;
use dioxus_field::{
    ChangeOrigin, FieldControlOptions, FieldSurface, merge_attributes, use_binding, use_field_meta,
    use_focus_registration,
};
use dioxus_primitives::dioxus_attributes::attributes;
use schemaform::form::ScalarValueState;
use schemaform_dioxus::{ControlRenderContext, use_boolean_edit};

use super::Appearance;
use super::mapping::use_boolean_binding;
use super::parts::{WidgetLayout, editable, editable_field, kind_name};
use crate::components::field::FieldDescription;
use crate::components::native_select::{NativeSelect, NativeSelectOption};

/// One daisyUI boolean control: the renderer's hook-safe child component.
///
/// A write-only boolean is a replacement select over the localized false and true labels, so the
/// value it holds is never shown; the tri-state binding never reads a value for it either. Every
/// other boolean, nullable or not, is a native checkbox carrying daisyUI's `checkbox` class, which
/// keeps the built-in's native semantics. While the data holds no boolean, missing or null, it is
/// indeterminate; a nullable one reaches null through the set-null affordance, and a click from
/// any state makes it a boolean, as the native control defines toggling.
#[component]
pub(super) fn BooleanControl(context: ControlRenderContext, appearance: Appearance) -> Element {
    let edit = use_boolean_edit(&context);
    let binding = use_boolean_binding(edit);
    let projection = match editable(&context, appearance) {
        Ok(projection) => projection,
        Err(rendered) => return rendered,
    };
    let control = context.control();
    let kind = kind_name(control.kind);
    // The edit reads a missing value as unchecked, which is indistinguishable from false; the
    // projection still says no boolean is there.
    let unset = matches!(
        projection.value_state,
        Some(ScalarValueState::Missing | ScalarValueState::Null)
    );

    if let Some(replacement) = control.write_only_replacement.clone() {
        // The value must not be echoed, so the widget is a replacement select that the edit hook
        // puts back on its placeholder after every write, as the built-in does.
        let labels = control
            .boolean_labels
            .clone()
            .expect("the adapter localizes false and true labels for every boolean control");
        let options = vec![
            NativeSelectOption::new(false, labels.false_label).form_value("false"),
            NativeSelectOption::new(true, labels.true_label).form_value("true"),
        ];
        return editable_field(
            &context,
            appearance,
            binding,
            WidgetLayout::Stacked,
            rsx! {
                NativeSelect::<bool> {
                    options,
                    placeholder: replacement.placeholder,
                    "data-schemaform-control": kind,
                    "data-write-only-replacement": "",
                }
            },
        );
    }

    editable_field(
        &context,
        appearance,
        binding,
        WidgetLayout::Row,
        rsx! {
            NativeCheckbox { indeterminate: unset, "data-schemaform-control": kind }
            {unset_description(&context, unset)}
        },
    )
}

/// What an indeterminate checkbox means, for assistive technology: it announces the state as
/// mixed, and no finding describes it before the user or a submission touches the field. A
/// fixed English string, like the package's other prose; the dash is the visible cue.
fn unset_description(context: &ControlRenderContext, unset: bool) -> Element {
    let id = format!("{}-unset", context.presentation().element_id);
    rsx! {
        if unset {
            FieldDescription {
                id: Rc::from(id.as_str()),
                class: "sr-only",
                "data-schemaform-unset": "",
                "{UNSET}"
            }
        }
    }
}

/// The description of a checkbox that holds no boolean.
const UNSET: &str = "Not set";

/// A native checkbox styled with daisyUI's `checkbox` class and bound the way the registry's
/// widgets are: it resolves the tri-state binding and the metadata from the surrounding `Field`,
/// reads and writes the binding's two-state view, and registers for the field's focus requests.
///
/// The element carries the adapter's element id through the metadata, which is what the edit
/// hook resynchronises after a rejected write. Required is spelled as `aria-required`, as the
/// built-in does: a native `required` on a checkbox would demand the checked state rather than
/// a present value. Classes passed by the caller concatenate with the checkbox's own; every
/// other attribute the caller passes overrides the checkbox's.
///
/// `indeterminate` is the native property of that name, which has no HTML attribute, so it is set
/// on the element by id after every render that changes it and after every input: the browser
/// clears it on a click, and a write the core rejects leaves the node without a boolean. A click
/// from the indeterminate state checks the box, so the first write is `true`.
#[component]
fn NativeCheckbox(
    /// Whether the checkbox shows that it holds no boolean.
    indeterminate: bool,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let binding = use_binding::<Option<bool>>(None, None);
    let meta = use_field_meta(None);
    let mut control: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let focus_control = use_callback(move |()| {
        if let Some(control) = control() {
            spawn(async move {
                let _ = control.set_focus(true).await;
            });
        }
    });
    use_focus_registration(focus_control);
    let mut inputs = use_signal(|| 0u32);
    use_effect(use_reactive!(|indeterminate| {
        // Read for the subscription: every input re-asserts the state.
        inputs();
        let eval = document::eval(INDETERMINATE_SCRIPT);
        let _ = eval.send((meta.id().to_string(), indeterminate));
    }));

    let checked = (binding.read)().unwrap_or(false);
    let color = if meta.invalid() { "checkbox-error" } else { "" };
    let base = attributes!(input {
        class: "checkbox {color}",
        r#type: "checkbox",
    });
    let meta_attributes =
        meta.attributes_for(&FieldControlOptions::new().surface(FieldSurface::BUTTON_WIDGET));
    let merged = merge_attributes(vec![meta_attributes, base, attributes]);
    let write_binding = binding.clone();
    let focus_exit_binding = binding;

    rsx! {
        input {
            checked,
            onmounted: move |event: MountedEvent| control.set(Some(event.data())),
            oninput: move |event: FormEvent| {
                write_binding.write(Some(event.checked()), ChangeOrigin::User);
                inputs += 1;
            },
            onfocusout: move |_| focus_exit_binding.focus_exit(),
            ..merged,
        }
    }
}

/// Sets the native `indeterminate` property of the element with the received id.
const INDETERMINATE_SCRIPT: &str = r#"
const [id, indeterminate] = await dioxus.recv();
const control = document.getElementById(id);
if (control) control.indeterminate = indeterminate;
"#;
