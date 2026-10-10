//! How the window spends its width: the two screens, the regions of a narrow
//! window, and the announcement when the Console changes on its own.

use acp_inspector_core::ConnectionStatus;
use dioxus::prelude::*;

use crate::console;

/// Which screens the window spends its width on: both, or the wire alone.
/// Window-session state, never persisted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Spine {
    #[default]
    Split,
    Wire,
}

impl Spine {
    pub(crate) const ALL: [Self; 2] = [Self::Split, Self::Wire];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Split => "Split",
            Self::Wire => "Trace only",
        }
    }

    pub(crate) fn says(self) -> &'static str {
        match self {
            Self::Split => "Show the timeline and the trace side by side",
            Self::Wire => "Give the trace the whole width",
        }
    }

    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::Split => "split spine-split",
            Self::Wire => "split spine-wire",
        }
    }

    /// The other layout, for the desktop menu's toggle.
    #[cfg(feature = "desktop")]
    pub(crate) fn other(self) -> Self {
        match self {
            Self::Split => Self::Wire,
            Self::Wire => Self::Split,
        }
    }
}

/// Which region a window too narrow for three columns shows (below 760px, a
/// width only the web build reaches). The stylesheet decides the threshold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Turn,
    Wire,
    Rail,
}

impl Focus {
    pub(crate) const ALL: [Self; 3] = [Self::Rail, Self::Turn, Self::Wire];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Rail => "Details",
            Self::Turn => "Timeline",
            Self::Wire => "Trace",
        }
    }

    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::Rail => "body on-rail",
            Self::Turn => "body on-turn",
            Self::Wire => "body on-wire",
        }
    }
}

#[component]
pub(crate) fn RegionBar(
    selected: Focus,
    /// Whether the agent is waiting on the reader; the timeline's item says so
    /// while another region is showing.
    #[props(default)]
    waiting: bool,
    on_select: EventHandler<Focus>,
) -> Element {
    rsx! {
        div { class: "mobile-bar seg", role: "group", aria_label: "Which region is shown",
            for chosen in Focus::ALL {
                {
                    let flagged = waiting && chosen == Focus::Turn && selected != Focus::Turn;
                    rsx! {
                        button {
                            key: "{chosen.label()}",
                            class: "seg-item",
                            "data-slot": "focus",
                            "data-waiting": flagged.then_some("true"),
                            r#type: "button",
                            aria_pressed: chosen == selected,
                            aria_label: if flagged { format!("{}, waiting on you", chosen.label()) } else { chosen.label().to_owned() },
                            onclick: move |_| on_select.call(chosen),
                            "{chosen.label()}"
                            if flagged {
                                span { class: "region-badge", "data-slot": "waiting-badge", aria_hidden: "true", "waiting on you" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The two live screens and the layout that chooses which is visible.
#[component]
pub(crate) fn Screens(spine: Spine, timeline: Element, console: Element) -> Element {
    rsx! {
        div { class: spine.class(),
            // Layout changes visibility, not the lifetime of draft-owning
            // components. The existing spine-wire rule uses display: none,
            // removing the Timeline from keyboard and accessibility navigation.
            {timeline}
            {console}
        }
    }
}

/// Announces the one automatic surface change without making either evidence
/// list live or moving focus. Persistent, so a status change only changes its
/// text.
#[component]
pub(crate) fn ConsoleAnnouncement(status: ConnectionStatus) -> Element {
    let said = console::diagnostics_announcement(status);

    rsx! {
        div {
            class: "sr-only",
            "data-slot": "console-announcement",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            if let Some(said) = said {
                "{said}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::stores::PRESERVE_CONSOLE_FOCUS;

    fn console_announcement(status: ConnectionStatus) -> String {
        let mut dom =
            VirtualDom::new_with_props(ConsoleAnnouncement, ConsoleAnnouncementProps { status });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn automatic_diagnostics_navigation_is_announced_without_taking_focus() {
        let quiet = console_announcement(ConnectionStatus::Disconnected);
        assert!(
            quiet.contains(r#"data-slot="console-announcement""#)
                && quiet.contains(r#"role="status""#)
                && quiet.contains(r#"aria-live="polite""#)
                && quiet.contains(r#"aria-atomic="true""#)
                && !quiet.contains("Diagnostics opened"),
            "the persistent region starts empty: {quiet}"
        );

        for (status, words) in [
            (
                ConnectionStatus::FailedToStart,
                "Failed to start. Diagnostics opened.",
            ),
            (
                ConnectionStatus::Lost,
                "Connection lost. Diagnostics opened.",
            ),
        ] {
            let html = console_announcement(status);
            assert!(
                html.contains(words),
                "the automatic surface change is explicit: {html}"
            );
        }

        assert!(
            PRESERVE_CONSOLE_FOCUS.contains("console-tab-trace")
                && PRESERVE_CONSOLE_FOCUS.contains(r#"closest('[data-slot="trace"]')"#)
                && PRESERVE_CONSOLE_FOCUS.contains("console-tab-diagnostics")
                && PRESERVE_CONSOLE_FOCUS.contains(".focus()"),
            "only focus already inside the replaced Console surface is transferred: {PRESERVE_CONSOLE_FOCUS}"
        );
    }

    #[test]
    fn the_narrow_bar_names_the_regions_and_flags_a_hidden_request() {
        fn bar(selected: Focus, waiting: bool) -> String {
            #[component]
            fn Host(selected: Focus, waiting: bool) -> Element {
                rsx! { RegionBar { selected, waiting, on_select: move |_| {} } }
            }
            let mut dom = VirtualDom::new_with_props(Host, HostProps { selected, waiting });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        }

        let html = bar(Focus::Turn, false);
        for label in ["Details", "Timeline", "Trace"] {
            assert!(html.contains(&format!(">{label}</button>")), "{html}");
        }
        assert!(
            !html.contains("Messages") && !html.contains(">Session<"),
            "{html}"
        );

        assert!(!bar(Focus::Turn, true).contains("waiting-badge"));
        let hidden = bar(Focus::Wire, true);
        assert!(
            hidden.contains(r#"data-slot="waiting-badge""#)
                && hidden.contains(r#"aria-label="Timeline, waiting on you""#),
            "{hidden}"
        );
    }
}
