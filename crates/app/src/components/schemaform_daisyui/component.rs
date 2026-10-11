//! The daisyUI control renderer and the registry that selects it, and the component that binds a
//! form through every seam this package fills.

use std::{rc::Rc, sync::Arc};

use dioxus::prelude::*;
use schemaform::{
    AdvisorySubmission, SubmissionSnapshot,
    definition::{DefinitionNodeView, SemanticKind},
};
use schemaform_dioxus::{
    BUILTIN_CONTROL_PRIORITY, ControlKind, ControlMatcher, ControlRegistry, ControlRenderContext,
    ControlRenderer, FindingCollectionPresenter, FormHandle, HandleError, RenderConfiguration,
    SchemaForm, StructureRenderers, SubmissionMode,
};

use super::boolean::BooleanControl;
use super::choice::NativeSelectControl;
use super::constant::ConstantControl;
use super::findings::DaisyuiFindings;
use super::multiple_choice::MultipleChoiceControl;
use super::shell::DaisyuiShell;
use super::text::TextControl;
use super::{Addons, Appearance, ButtonClasses, Options};
use super::{Density, density::ControlScope};

/// A form rendered through every seam this package fills: the daisyUI control registry, the
/// structure bundle, and the finding presenter in both presenter slots, with any [`Addons`].
///
/// The form is bound once, when the component mounts, as the adapter requires of structure
/// renderers; changing the form handle means remounting. A host that wants one seam to differ —
/// its own shell, say, or the built-in controls under the daisyUI shell — composes a
/// [`RenderConfiguration`] from [`controls`], [`structure`], and [`findings`] instead and renders
/// `SchemaForm` itself; [`configuration`] is the composition this component uses.
///
/// A bind failure is a render error: it reports a definition the renderers cannot present, which
/// is a programming error of the host rather than a state of the form. A UI schema naming a
/// widget symbol whose add-on is not among `addons` is one.
#[component]
pub fn SchemaformDaisyui(
    /// The form to present.
    form: FormHandle,
    /// Receives the submission snapshot of a submission that passed preparation.
    on_submit: EventHandler<SubmissionSnapshot>,
    /// Whether submission is gated on validity or delivered with advisory findings.
    #[props(default)]
    submission_mode: SubmissionMode,
    /// Receives data and findings in advisory mode, on a separate channel from validated snapshots.
    #[props(default)]
    on_advisory_submit: EventHandler<AdvisorySubmission>,
    /// Receives adapter operation failures; failures are dropped when it is not set.
    #[props(default)]
    on_error: EventHandler<HandleError>,
    /// Whether the renderers emit their layout utilities. Fixed at mount, like the renderers.
    #[props(default)]
    appearance: Appearance,
    /// Field layout, fixed at mount like the renderer configuration.
    #[props(default)]
    density: Density,
    /// Classes for the buttons the renderers draw themselves, fixed at mount like the rest.
    #[props(default)]
    buttons: ButtonClasses,
    /// Opt-in renderers for the radio and compound select widgets and for homogeneous arrays,
    /// read once at mount like the rest.
    #[props(default)]
    addons: Addons,
) -> Element {
    let bound = use_hook(|| {
        let options = Options::from(appearance).density(density).buttons(buttons);
        configuration_with_addons(options, &addons)
            .bind(&form)
            .map_err(Rc::new)
    });
    let bound = match bound {
        Ok(bound) => bound,
        Err(error) => return Err(dioxus::core::CapturedError::from_display(error).into()),
    };
    rsx! {
        SchemaForm { form: bound, on_submit, submission_mode, on_advisory_submit, on_error }
    }
}

/// The render configuration [`SchemaformDaisyui`] binds a form with when it has no add-ons:
/// [`controls`] as the control registry, [`structure`] as the structure bundle, and [`findings`]
/// in both presenter slots.
pub fn configuration() -> RenderConfiguration {
    configuration_with(Appearance::default())
}

/// [`configuration`] with every renderer built with `options`: an [`Appearance`] alone, or
/// [`Options`] carrying density and button classes as well.
pub fn configuration_with(options: impl Into<Options>) -> RenderConfiguration {
    configuration_with_addons(options, &Addons::default())
}

/// [`configuration_with`] at `appearance` and `density`.
pub fn configuration_with_density(appearance: Appearance, density: Density) -> RenderConfiguration {
    configuration_with(Options::from(appearance).density(density))
}

/// [`configuration_with`] with each of `addons` applied to the control registry and the
/// structure bundle, built with the same `options`.
pub fn configuration_with_addons(
    options: impl Into<Options>,
    addons: &Addons,
) -> RenderConfiguration {
    let options = options.into();
    RenderConfiguration::builder()
        .controls(addons.controls(controls_with(options), options))
        .structure(addons.structure(structure_with(options), options))
        .summary_presenter(findings_with(options.appearance))
        .local_presenter(findings_with(options.appearance))
        .build()
}

/// Matcher priority at which [`controls`] registers [`DaisyuiControlRenderer`].
///
/// Above [`BUILTIN_CONTROL_PRIORITY`] so the daisyUI renderer wins every control it accepts
/// and the built-in keeps the rest.
pub const DAISYUI_CONTROL_PRIORITY: i32 = BUILTIN_CONTROL_PRIORITY + 10;

/// A control registry in which every control kind renders as a daisyUI field.
///
/// The registry starts from the built-ins, so structural nodes keep their built-in
/// presentation and a control the daisyUI renderer does not accept still renders. Choices render
/// as a native select; the radio group and the compound select are add-ons that register a widget
/// symbol each.
pub fn controls() -> ControlRegistry {
    controls_with(Appearance::default())
}

/// [`controls_with`] at `appearance` and `density`.
pub fn controls_with_density(appearance: Appearance, density: Density) -> ControlRegistry {
    controls_with(Options::from(appearance).density(density))
}

/// [`controls`] with every renderer built with `options`: an [`Appearance`] alone, or [`Options`]
/// carrying density and button classes as well.
pub fn controls_with(options: impl Into<Options>) -> ControlRegistry {
    ControlRegistry::with_builtins().matcher(
        DAISYUI_CONTROL_PRIORITY,
        Arc::new(DaisyuiControls),
        Arc::new(DaisyuiControlRenderer::from(options.into())),
    )
}

/// The structure renderers this component ships: the daisyUI form shell.
///
/// Every slot the component does not implement stays the built-in, so a form bound with this
/// bundle degrades to the adapter's accessible unstyled output for those node kinds rather than
/// losing a region. Homogeneous arrays are one: the `schemaform_daisyui_collection` add-on fills
/// that slot.
pub fn structure() -> StructureRenderers {
    structure_with(Appearance::default())
}

/// [`structure`] with the shell built with `options`: an [`Appearance`] alone, or [`Options`]
/// carrying button classes as well. The shell has no density.
pub fn structure_with(options: impl Into<Options>) -> StructureRenderers {
    let options = options.into();
    StructureRenderers::default().with_shell(
        DaisyuiShell::default()
            .appearance(options.appearance)
            .buttons(options.buttons),
    )
}

/// The finding presenter for both presenter slots of a render configuration.
///
/// Wire it as the summary presenter for the alert and as the local presenter so the findings a
/// built-in container or an add-on's collection renders through
/// `NodePresentation::present_findings` are daisyUI-styled as well.
pub fn findings() -> Arc<dyn FindingCollectionPresenter> {
    findings_with(Appearance::default())
}

/// [`findings`] at `appearance`.
pub fn findings_with(appearance: Appearance) -> Arc<dyn FindingCollectionPresenter> {
    Arc::new(DaisyuiFindings::default().appearance(appearance))
}

/// Accepts exactly the definition nodes [`DaisyuiControlRenderer`] presents itself: those the
/// adapter derives a control kind from.
struct DaisyuiControls;

impl ControlMatcher for DaisyuiControls {
    fn matches(&self, definition: DefinitionNodeView<'_>) -> bool {
        definition.is_multiple_choice()
            || matches!(
                definition.semantic_kind(),
                Some(
                    SemanticKind::String
                        | SemanticKind::Number
                        | SemanticKind::Integer
                        | SemanticKind::Boolean
                        | SemanticKind::Choice
                        | SemanticKind::Null
                )
            )
    }
}

/// Renders every control kind with the registry's `Field` parts and widgets.
///
/// The renderer owns the whole control region: label, widget, help, findings, and presence
/// affordances. Choices are a native select. Unknown future kinds fail visibly rather than
/// silently losing daisyUI presentation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DaisyuiControlRenderer {
    appearance: Appearance,
    density: Density,
    buttons: ButtonClasses,
}

impl DaisyuiControlRenderer {
    /// The same renderer at `appearance`.
    pub fn appearance(self, appearance: Appearance) -> Self {
        Self { appearance, ..self }
    }

    /// The same renderer at `density`.
    pub fn density(self, density: Density) -> Self {
        Self { density, ..self }
    }

    /// The same renderer with `buttons` for the presence operations it draws.
    pub fn buttons(self, buttons: ButtonClasses) -> Self {
        Self { buttons, ..self }
    }

    /// Renders `context` as [`ControlRenderer::render`] does, except that a choice is `choice`'s
    /// widget: how a widget add-on presents its choices with every other kind, the density, and
    /// the button classes unchanged.
    pub(crate) fn render_with_choice(
        &self,
        context: ControlRenderContext,
        choice: fn(ControlRenderContext, Appearance) -> Element,
    ) -> Element {
        let appearance = self.appearance;
        // The kind is definition-stable, so a node always renders the same child component and
        // the hooks inside it are called unconditionally.
        let body = match context.control().kind {
            ControlKind::String | ControlKind::Number | ControlKind::Integer => {
                rsx! { TextControl { context, appearance } }
            }
            ControlKind::Boolean => rsx! { BooleanControl { context, appearance } },
            ControlKind::Choice => choice(context, appearance),
            ControlKind::Constant => rsx! { ConstantControl { context, appearance } },
            ControlKind::MultipleChoice => rsx! { MultipleChoiceControl { context, appearance } },
            // The upstream enum is non-exhaustive, so stable Rust requires this guard.
            kind => panic!("schemaform_daisyui does not yet present control kind {kind:?}"),
        };
        rsx! { ControlScope { density: self.density, buttons: self.buttons, {body} } }
    }
}

impl From<Options> for DaisyuiControlRenderer {
    fn from(options: Options) -> Self {
        Self::default()
            .appearance(options.appearance)
            .density(options.density)
            .buttons(options.buttons)
    }
}

impl ControlRenderer for DaisyuiControlRenderer {
    fn render(&self, context: ControlRenderContext) -> Element {
        self.render_with_choice(context, |context, appearance| {
            rsx! { NativeSelectControl { context, appearance } }
        })
    }
}
