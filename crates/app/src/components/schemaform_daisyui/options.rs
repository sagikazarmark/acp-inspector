//! Everything fixed at bind that shapes how the renderers present a form.

use super::{Appearance, ButtonClasses, Density};

/// The presentation every renderer in this package is built with: the [`Appearance`] axis, the
/// field [`Density`], and the host's [`ButtonClasses`].
///
/// An [`Appearance`] converts into options with the other two at their defaults, so
/// `configuration_with(Appearance::None)` reads as it always has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub(crate) appearance: Appearance,
    pub(crate) density: Density,
    pub(crate) buttons: ButtonClasses,
}

impl Options {
    /// The same options at `appearance`.
    pub const fn appearance(self, appearance: Appearance) -> Self {
        Self { appearance, ..self }
    }

    /// The same options at `density`.
    pub const fn density(self, density: Density) -> Self {
        Self { density, ..self }
    }

    /// The same options with `buttons`.
    pub const fn buttons(self, buttons: ButtonClasses) -> Self {
        Self { buttons, ..self }
    }
}

impl From<Appearance> for Options {
    fn from(appearance: Appearance) -> Self {
        Self::default().appearance(appearance)
    }
}
