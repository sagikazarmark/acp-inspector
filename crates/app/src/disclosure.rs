//! The shared visual markers: the marker for compact disclosure controls, and
//! the icon each role in the window is drawn with ([`icon`]).

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

/// What an icon stands for, wherever one is drawn.
///
/// **The one place an icon is chosen** (D8). Screens ask for a role and never
/// name a glyph, so moving the window to another set (Heroicons v2, or Lucide
/// through `dioxus-free-icons`' `lucide` feature) is an edit to [`icon`] and
/// nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// A user's message.
    User,
    /// The agent's message.
    Agent,
    /// The agent's thinking.
    Thought,
    /// A tool call.
    Tool,
    /// The agent's plan.
    Plan,
    /// A request the turn stopped on.
    Ask,
    /// A conformance annotation: what the specification requires.
    Spec,
    /// Traffic this window has no name for.
    Unnamed,
    /// Anything else the timeline notes.
    Note,
    /// Send a prompt.
    Send,
    /// Stop a turn.
    Stop,
    /// The first-run card: start an agent.
    Start,
    /// Sent to the agent.
    Outgoing,
    /// Received from the agent.
    Incoming,
}

/// The icon for a role, decorative: what it stands for is said in text beside
/// it, so it is hidden from the accessibility tree.
pub(crate) fn icon(role: Role) -> Element {
    use dioxus_free_icons::icons::hi_outline_icons::{
        HiArrowSmLeft, HiArrowSmRight, HiArrowUp, HiBookOpen, HiChat, HiCog, HiHand,
        HiInformationCircle, HiLightBulb, HiQuestionMarkCircle, HiSparkles, HiStop, HiTerminal,
        HiViewList,
    };

    let drawn = match role {
        Role::User => rsx! { Icon { class: "icon", icon: HiChat } },
        Role::Agent => rsx! { Icon { class: "icon", icon: HiSparkles } },
        Role::Thought => rsx! { Icon { class: "icon", icon: HiLightBulb } },
        Role::Tool => rsx! { Icon { class: "icon", icon: HiCog } },
        Role::Plan => rsx! { Icon { class: "icon", icon: HiViewList } },
        Role::Ask => rsx! { Icon { class: "icon", icon: HiHand } },
        Role::Spec => rsx! { Icon { class: "icon", icon: HiBookOpen } },
        Role::Unnamed => rsx! { Icon { class: "icon", icon: HiQuestionMarkCircle } },
        Role::Note => rsx! { Icon { class: "icon", icon: HiInformationCircle } },
        Role::Send => rsx! { Icon { class: "icon", icon: HiArrowUp } },
        Role::Stop => rsx! { Icon { class: "icon", icon: HiStop } },
        Role::Start => rsx! { Icon { class: "icon", icon: HiTerminal } },
        Role::Outgoing => rsx! { Icon { class: "icon", icon: HiArrowSmRight } },
        Role::Incoming => rsx! { Icon { class: "icon", icon: HiArrowSmLeft } },
    };

    rsx! {
        span { class: "role-icon", "data-icon": "{role:?}", aria_hidden: "true", {drawn} }
    }
}
