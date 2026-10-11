//! The seam opt-in add-on Components extend this package through.

use std::rc::Rc;

use schemaform_dioxus::{ControlRegistry, StructureRenderers};

use super::Options;

/// An opt-in Component that adds renderers to the seams this package fills, installed only by a
/// host whose forms reach them: `schemaform_daisyui_radio` and `schemaform_daisyui_select` add a
/// widget symbol each, `schemaform_daisyui_collection` the homogeneous-array renderer.
///
/// Both methods default to leaving their argument alone, so an add-on implements only the seam it
/// fills. Each receives the [`Options`] the rest of the form is built with, so an add-on's
/// renderers share the form's appearance, density, and button classes.
pub trait Addon {
    /// `registry` with this add-on's control renderers registered.
    fn controls(&self, registry: ControlRegistry, options: Options) -> ControlRegistry {
        let _ = options;
        registry
    }

    /// `structure` with this add-on's structure renderers in their slots.
    fn structure(&self, structure: StructureRenderers, options: Options) -> StructureRenderers {
        let _ = options;
        structure
    }
}

/// The add-ons a form is bound with, as a value a component prop can hold.
///
/// Read once, when the form is bound, like every other renderer input; two values compare equal
/// when they hold the same add-ons in the same order. Adding the same widget add-on twice
/// registers its symbol twice, which the adapter reports as an ambiguous widget at bind.
#[derive(Clone, Default)]
pub struct Addons(Vec<Rc<dyn Addon>>);

impl Addons {
    /// These add-ons followed by `addon`.
    pub fn with(mut self, addon: impl Addon + 'static) -> Self {
        self.0.push(Rc::new(addon));
        self
    }

    pub(super) fn controls(&self, registry: ControlRegistry, options: Options) -> ControlRegistry {
        self.0.iter().fold(registry, |registry, addon| {
            addon.controls(registry, options)
        })
    }

    pub(super) fn structure(
        &self,
        structure: StructureRenderers,
        options: Options,
    ) -> StructureRenderers {
        self.0.iter().fold(structure, |structure, addon| {
            addon.structure(structure, options)
        })
    }
}

impl PartialEq for Addons {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|(this, that)| Rc::ptr_eq(this, that))
    }
}
