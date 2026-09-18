use dioxus::prelude::*;

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

#[component]
pub(super) fn DensityScope(density: Density, children: Element) -> Element {
    use_context_provider(|| density);
    children
}
