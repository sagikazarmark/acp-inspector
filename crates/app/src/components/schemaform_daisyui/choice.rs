//! The daisyUI choice control: a native select. The radio group and the compound select a UI
//! schema can ask for are add-ons, `schemaform_daisyui_radio` and `schemaform_daisyui_select`.

use dioxus::prelude::*;
use schemaform_dioxus::{ChoiceIdentity, ControlRenderContext, use_choice_edit};

use super::Appearance;
use super::mapping::use_choice_binding;
use super::parts::{WidgetLayout, editable, editable_field, kind_name};
use crate::components::native_select::{NativeSelect, NativeSelectOption};

/// The placeholder a choice widget shows while no option is selected.
///
/// A write-only control shows its replacement placeholder, since the value must not be echoed;
/// otherwise nothing, as the built-in's hidden placeholder does. Incompatible data is not shown
/// here but in the readout under the widget, which carries a stable id and describes the widget,
/// as for every other control kind.
pub(crate) fn placeholder(context: &ControlRenderContext) -> String {
    context
        .control()
        .write_only_replacement
        .as_ref()
        .map(|replacement| replacement.placeholder.clone())
        .unwrap_or_default()
}

/// One daisyUI choice control as a native select: the renderer's hook-safe child component.
///
/// Every option, the null option included, is a native `option` whose form value is the
/// option's opaque identity, so the select speaks the same strings the edit hook resynchronises
/// after a rejected write. The placeholder is a disabled first option selected while nothing is,
/// which is also where a write-only control rests after every write.
#[component]
pub(super) fn NativeSelectControl(
    context: ControlRenderContext,
    appearance: Appearance,
) -> Element {
    let edit = use_choice_edit(&context);
    let binding = use_choice_binding(edit.clone());
    if let Err(rendered) = editable(&context, appearance) {
        return rendered;
    }
    let control = context.control();
    let kind = kind_name(control.kind);
    let placeholder = placeholder(&context);
    let options = edit
        .options
        .iter()
        .map(|option| {
            NativeSelectOption::new(option.identity.clone(), option.label.clone())
                .form_value(option.identity.as_str())
                .disabled(option.disabled)
        })
        .collect::<Vec<_>>();

    editable_field(
        &context,
        appearance,
        binding,
        WidgetLayout::Stacked,
        rsx! {
            NativeSelect::<ChoiceIdentity> {
                options,
                placeholder,
                "data-schemaform-control": kind,
                "data-write-only-replacement": control.write_only.then_some(""),
            }
        },
    )
}
