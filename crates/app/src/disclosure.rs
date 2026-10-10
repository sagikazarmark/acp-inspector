//! The shared visual markers: the marker for compact disclosure controls, and
//! the icon each role in the window is drawn with ([`icon`]).

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdChevronRight, LdX},
};

/// A decorative Lucide chevron. Native `details[open]` and explicit
/// `aria-expanded` state rotate the same marker in the stylesheet.
pub fn chevron() -> Element {
    rsx! {
        span { class: "disclosure-icon", aria_hidden: "true",
            Icon { class: "icon", width: 16, height: 16, icon: LdChevronRight }
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
            Icon { class: "icon", width: 16, height: 16, icon: LdX }
        }
    }
}

/// What an icon stands for, wherever one is drawn.
///
/// **The one place an icon is chosen** (D8). Screens ask for a role and never
/// name a glyph. The window draws one family, Lucide, at 16px with a 1.5
/// stroke (the stylesheet's `.icon`), so every icon in it is the same weight.
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
    use dioxus_free_icons::icons::ld_icons::{
        LdArrowLeft, LdArrowRight, LdArrowUp, LdBookOpen, LdCircleHelp, LdHand, LdInfo,
        LdLightbulb, LdListChecks, LdMessageSquare, LdSparkles, LdSquare, LdSquareTerminal,
        LdWrench,
    };

    let drawn = match role {
        Role::User => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdMessageSquare } },
        Role::Agent => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdSparkles } },
        Role::Thought => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdLightbulb } },
        Role::Tool => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdWrench } },
        Role::Plan => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdListChecks } },
        Role::Ask => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdHand } },
        Role::Spec => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdBookOpen } },
        Role::Unnamed => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdCircleHelp } },
        Role::Note => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdInfo } },
        Role::Send => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdArrowUp } },
        Role::Stop => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdSquare } },
        Role::Start => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdSquareTerminal } },
        Role::Outgoing => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdArrowRight } },
        Role::Incoming => rsx! { Icon { class: "icon", width: 16, height: 16, icon: LdArrowLeft } },
    };

    rsx! {
        span { class: "role-icon", "data-icon": "{role:?}", aria_hidden: "true", {drawn} }
    }
}
