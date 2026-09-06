//! The shared visual marker for compact disclosure controls.

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::hi_outline_icons::{HiChevronRight, HiX},
};

/// A decorative Heroicons Outline chevron. Native `details[open]` and explicit
/// `aria-expanded` state rotate the same marker in the stylesheet.
pub fn chevron() -> Element {
    rsx! {
        span { class: "disclosure-icon", aria_hidden: "true",
            Icon { class: "icon", icon: HiChevronRight }
        }
    }
}

/// The way out of a dialog, as an icon.
///
/// **From the icon family and not from the character set.** It was a literal
/// `✕`, which is a font's idea of a cross rather than this window's: it takes
/// the text metrics of whatever face renders it, sits off the optical centre of
/// its button, and is the one glyph in the window that does not come from the
/// family every other control draws from.
pub fn close() -> Element {
    rsx! {
        span { class: "disclosure-icon", aria_hidden: "true",
            Icon { class: "icon", icon: HiX }
        }
    }
}
