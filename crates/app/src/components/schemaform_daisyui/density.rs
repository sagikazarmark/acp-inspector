use dioxus::prelude::*;

use super::ButtonClasses;

/// Placement and spacing of field content. Independent of whether the host
/// supplies its own utilities through [`super::Appearance::None`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    #[default]
    Default,
    /// Presence operations share the label row; choice options may wrap.
    Compact,
}

impl Density {
    pub const ALL: [Self; 2] = [Self::Default, Self::Compact];
}

pub(super) fn compact() -> bool {
    try_consume_context::<Density>() == Some(Density::Compact)
}

/// What a control renders inside: its density and the host's button classes, as contexts the
/// shared parts read.
#[component]
pub(super) fn ControlScope(density: Density, buttons: ButtonClasses, children: Element) -> Element {
    use_context_provider(|| density);
    use_context_provider(|| buttons);
    children
}
