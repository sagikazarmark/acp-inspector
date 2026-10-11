//! The classes a host gives the buttons the renderers draw themselves.

use dioxus::prelude::*;

/// A host's classes for the buttons this package renders itself, one per role.
///
/// The package draws its own buttons for presence operations (Set, Clear, Remove …), a
/// collection item's actions, a collection's append affordance, and the submit affordance in
/// either submission mode. A role left unset keeps the package's own colour, size and variant. A
/// role given a class renders `btn` plus that class instead, so the host's variant never fights
/// the package's `btn-primary` or `btn-sm`: include a size class if the role needs one, since
/// [`super::Density::Compact`]'s `btn-xs` no longer applies either. Structural classes stay:
/// an item action keeps `join-item btn-square`, and the [`super::Appearance`] axis still governs
/// the layout utilities.
///
/// The classes are `&'static str` because Tailwind finds class names by scanning source: a host's
/// classes have to appear there as literals anyway.
///
/// Every button carries `data-schemaform-button` naming its role (`presence`, `item`, `append`,
/// `submit`), for a host that styles them from its stylesheet instead. The finding summary's
/// buttons carry `finding` but take no class here: they are daisyUI `link`s rather than `btn`s,
/// styled as the alert's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonClasses {
    presence: Option<&'static str>,
    item: Option<&'static str>,
    append: Option<&'static str>,
    submit: Option<&'static str>,
    advisory_submit: Option<&'static str>,
}

impl ButtonClasses {
    /// The presence operation buttons beside or below a field, and on a collection.
    pub const fn presence(self, class: &'static str) -> Self {
        Self {
            presence: Some(class),
            ..self
        }
    }

    /// A collection item's insert, move and remove buttons.
    pub const fn item(self, class: &'static str) -> Self {
        Self {
            item: Some(class),
            ..self
        }
    }

    /// A collection's append button.
    pub const fn append(self, class: &'static str) -> Self {
        Self {
            append: Some(class),
            ..self
        }
    }

    /// The submit button in gated submission mode.
    pub const fn submit(self, class: &'static str) -> Self {
        Self {
            submit: Some(class),
            ..self
        }
    }

    /// The submit button in advisory submission mode.
    pub const fn advisory_submit(self, class: &'static str) -> Self {
        Self {
            advisory_submit: Some(class),
            ..self
        }
    }

    pub(crate) const fn class(self, role: ButtonRole) -> Option<&'static str> {
        match role {
            ButtonRole::Presence => self.presence,
            ButtonRole::Item => self.item,
            ButtonRole::Append => self.append,
            ButtonRole::Submit => self.submit,
            ButtonRole::AdvisorySubmit => self.advisory_submit,
        }
    }
}

/// A button the package draws itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonRole {
    Presence,
    Item,
    Append,
    Submit,
    AdvisorySubmit,
}

/// The classes in scope: those the control renderer provides, else the package's own.
pub(super) fn current() -> ButtonClasses {
    try_consume_context::<ButtonClasses>().unwrap_or_default()
}
