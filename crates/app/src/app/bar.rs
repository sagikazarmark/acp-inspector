//! The bar across the top of the window, and the title the window is given.

use acp_inspector_core::{AgentCommand, Appearance, ConnectionStatus, Indentation, v1};
use dioxus::prelude::*;

use super::layout::Spine;
use crate::appearance::AppearanceControl;
use crate::indent::IndentationControl;
use crate::navigation;
#[cfg(feature = "desktop")]
use crate::shell;

/// What the window is looking at, in one line: the agent's `agentInfo` where it
/// gave one, else the program it was launched from, else `None`.
pub(crate) fn subject(
    described: Option<&v1::InitializeResponse>,
    launched: Option<&AgentCommand>,
) -> Option<String> {
    if let Some(info) = described.and_then(|agent| agent.agent_info.as_ref()) {
        let name = info.title.clone().unwrap_or_else(|| info.name.to_string());
        return Some(format!("{name} {}", info.version));
    }

    launched
        .map(|command| {
            let command = command.command.trim();
            command
                .rsplit_once('/')
                .map(|(_, program)| program)
                .unwrap_or(command)
                .to_owned()
        })
        .filter(|program| !program.is_empty())
}

/// Names the window after what it is looking at, for the task switcher and
/// dock. Only where there is a title bar.
#[cfg(feature = "desktop")]
pub(crate) fn use_title(
    described: Signal<Option<v1::InitializeResponse>>,
    launched: Signal<Option<AgentCommand>>,
) {
    use_effect(move || {
        let titled = match subject(described.read().as_ref(), launched.read().as_ref()) {
            Some(subject) => format!("{subject} — ACP Inspector"),
            None => "ACP Inspector".to_owned(),
        };
        shell::set_title(&titled);
    });
}

/// Whether the bar is the window's title bar: the desktop build on macOS, where
/// `shell.rs` makes the title bar transparent over the content.
const TITLED: bool = cfg!(all(feature = "desktop", target_os = "macos"));

/// A primary-button press on the bar's empty pieces moves the window, where the
/// bar is the title bar. Never bound on controls.
fn drag(event: MouseEvent) {
    #[cfg(feature = "desktop")]
    if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
        shell::drag_window();
    }
    #[cfg(not(feature = "desktop"))]
    let _ = event;
}

/// A double-click on the bar's empty pieces zooms, as on a title bar.
fn zoom(_: MouseEvent) {
    #[cfg(feature = "desktop")]
    shell::zoom_window();
}

/// The bar: the tool's mark, the connection chip, the protocol and session
/// facts, and the window-wide switches. Connecting lives in the rail.
#[component]
pub(crate) fn TopBar(
    status: ConnectionStatus,
    appearance: Appearance,
    /// Whether payloads are drawn indented.
    indentation: Indentation,
    /// The running agent, or `None` before one is launched.
    subject: Option<String>,
    /// The protocol version the agent answered `initialize` with.
    protocol: Option<String>,
    /// The live session.
    session: Option<v1::SessionId>,
    spine: Spine,
    /// Whether the agent is waiting on the reader; flagged on Split while Trace
    /// only hides the timeline.
    #[props(default)]
    waiting: bool,
    on_spine: EventHandler<Spine>,
    on_choose: EventHandler<Appearance>,
    on_indent: EventHandler<Indentation>,
    /// Whether the rail is open as a drawer.
    #[props(default)]
    drawer: bool,
    /// Toggles the rail's drawer. The stylesheet draws the button only where the
    /// rail is one.
    #[props(default)]
    on_drawer: Option<EventHandler<()>>,
    /// Whether the rail is hidden on a wide window.
    #[props(default)]
    collapsed: bool,
    /// Hides or shows the rail on a wide window. The stylesheet draws the
    /// button only where the rail sits beside the screens.
    #[props(default)]
    on_collapse: Option<EventHandler<()>>,
) -> Element {
    let (tone, said) = connection_state(status);

    rsx! {
        header {
            // As the title bar, it leaves room for the traffic lights.
            class: if TITLED { "bar titled" } else { "bar" },
            if TITLED {
                span { class: "bar-lights", aria_hidden: "true", onmousedown: drag, ondoubleclick: zoom }
            }
            if let Some(on_drawer) = on_drawer {
                button {
                    id: navigation::DRAWER_TOGGLE,
                    class: "btn btn-sm btn-quiet btn-square bar-drawer",
                    "data-slot": "rail-toggle",
                    r#type: "button",
                    aria_label: "Details",
                    aria_expanded: "{drawer}",
                    aria_controls: navigation::RAIL,
                    title: if drawer { "Close the details" } else { "Show the connection, session and capabilities" },
                    onclick: move |_| on_drawer.call(()),
                    dioxus_free_icons::Icon { class: "icon", width: 16, height: 16, icon: dioxus_free_icons::icons::ld_icons::LdPanelLeft }
                }
            }
            if let Some(on_collapse) = on_collapse {
                button {
                    class: "btn btn-sm btn-quiet btn-square bar-collapse",
                    "data-slot": "rail-collapse",
                    r#type: "button",
                    aria_label: "Details",
                    aria_expanded: "{!collapsed}",
                    aria_controls: navigation::RAIL,
                    title: if collapsed { "Show the connection, session and capabilities" } else { "Hide the details" },
                    onclick: move |_| on_collapse.call(()),
                    dioxus_free_icons::Icon { class: "icon", width: 16, height: 16, icon: dioxus_free_icons::icons::ld_icons::LdPanelLeft }
                }
            }
            div { class: "bar-brand", onmousedown: drag, ondoubleclick: zoom,
                span { class: "mark", aria_hidden: "true", "A" }
                h1 { "ACP Inspector" }
                span { class: "tag version", "v{env!(\"CARGO_PKG_VERSION\")}" }
            }
            // The state as a dot, the agent's name, and the transport.
            div { class: "bar-conn", "data-slot": "connection-chip",
                span { class: "dot dot-{tone}", aria_hidden: "true" }
                span { class: "sr-only", "Connection: {said}." }
                span { class: "conn-name",
                    match &subject {
                        Some(subject) => rsx! { "{subject}" },
                        None => rsx! { span { class: "cleared", "no agent" } },
                    }
                }
                span { class: "conn-meta", "stdio" }
            }

            span { class: "grow", onmousedown: drag, ondoubleclick: zoom }

            div { class: "bar-facts", onmousedown: drag, ondoubleclick: zoom,
                if let Some(protocol) = protocol {
                    span { class: "bar-protocol", "protocol v{protocol}" }
                }
                if let Some(session) = session {
                    span { class: "bar-rule", aria_hidden: "true", "|" }
                    span { class: "bar-session", "{session}" }
                }
            }

            // Hidden where one region shows at a time, since it would choose between
            // layouts nobody can see.
            div { class: "seg bar-layout", "data-slot": "layout-toggle", role: "group", aria_label: "Which screens are shown",
                for chosen in Spine::ALL {
                    {
                        let flagged = waiting && chosen == Spine::Split && spine == Spine::Wire;
                        rsx! {
                            button {
                                key: "{chosen.label()}",
                                class: "seg-item",
                                "data-slot": "spine",
                                "data-waiting": flagged.then_some("true"),
                                r#type: "button",
                                aria_pressed: chosen == spine,
                                title: chosen.says(),
                                onclick: move |_| on_spine.call(chosen),
                                "{chosen.label()}"
                                if flagged {
                                    span { class: "region-badge", "data-slot": "waiting-badge", "waiting on you" }
                                }
                            }
                        }
                    }
                }
            }

            IndentationControl { chosen: indentation, on_choose: on_indent }
            AppearanceControl { chosen: appearance, on_choose }
        }
    }
}

/// The connection's tone and word, shared by the bar's chip, the rail's card and
/// the screen-reader sentence.
pub(crate) fn connection_state(status: ConnectionStatus) -> (&'static str, &'static str) {
    match status {
        ConnectionStatus::Connected => ("live", "connected"),
        ConnectionStatus::FailedToStart => ("gone", "failed to start"),
        ConnectionStatus::Lost => ("gone", "connection lost"),
        ConnectionStatus::Disconnected => ("idle", "disconnected"),
        // Non-exhaustive: an unknown state is neither nothing nor a known one.
        _ => ("idle", "unrecognized state"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_names_what_the_window_is_looking_at() {
        let command = AgentCommand {
            command: "/opt/agents/bin/testy".to_owned(),
            args: String::new(),
            env: String::new(),
            cwd: String::new(),
            ..AgentCommand::default()
        };

        assert_eq!(subject(None, None), None);

        // No `agentInfo` yet (or ever): the command is the subject.
        assert_eq!(subject(None, Some(&command)).as_deref(), Some("testy"));

        let described = v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1)
            .agent_info(v1::Implementation::new("testy", "1.2.3").title("Testy"));
        assert_eq!(
            subject(Some(&described), Some(&command)).as_deref(),
            Some("Testy 1.2.3")
        );

        let unnamed = v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1)
            .agent_info(v1::Implementation::new("testy", "1.2.3"));
        assert_eq!(
            subject(Some(&unnamed), None).as_deref(),
            Some("testy 1.2.3")
        );
    }

    #[test]
    fn the_bar_names_the_tool_once_and_the_agent_beside_it() {
        #[component]
        fn Host(status: ConnectionStatus, subject: Option<String>) -> Element {
            rsx! {
                TopBar {
                    status,
                    appearance: Appearance::System,
                    indentation: Indentation::Wire,
                    subject,
                    protocol: Some("1".to_owned()),
                    session: None,
                    spine: Spine::Split,
                    on_spine: move |_| {},
                    on_choose: move |_| {},
                    on_indent: move |_| {},
                }
            }
        }

        fn bar(status: ConnectionStatus, subject: Option<&str>) -> String {
            let mut dom = VirtualDom::new_with_props(
                Host,
                HostProps {
                    status,
                    subject: subject.map(str::to_owned),
                },
            );
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        }

        let html = bar(ConnectionStatus::Disconnected, None);

        // The tool names itself once; everything to its right is about the agent.
        assert!(
            html.contains(r#"<header class="bar"#)
                && html.contains(r#"class="mark""#)
                && html.contains("<h1>ACP Inspector</h1>")
                && html.matches("ACP Inspector").count() == 1,
            "the window says what it is once: {html}"
        );
        assert!(
            html.contains(r#"data-slot="connection-chip""#)
                && html.contains(r#"class="dot dot-idle""#)
                && html.contains("Connection: disconnected.")
                && html.contains("stdio"),
            "the chip says what is connected and how: {html}"
        );
        assert!(
            html.contains("no agent"),
            "with nothing launched it says so rather than naming this tool: {html}"
        );
        let running = bar(ConnectionStatus::Connected, Some("Testy 1.2.3"));
        assert!(
            running.contains(r#"class="dot dot-live""#)
                && running.contains("Testy 1.2.3")
                && running.contains("Connection: connected."),
            "and with an agent it is the agent's own name: {running}"
        );

        assert!(
            html.contains("protocol v1") && !html.contains(r#"class="bar-session""#),
            "the bar names a session only where there is one: {html}"
        );

        assert!(
            html.contains(r#"data-slot="spine""#)
                && html.contains(">Split</button>")
                && html.contains(">Trace only</button>")
                && html.contains(r#"class="seg bar-layout" data-slot="layout-toggle""#)
                && html.contains(r#"aria-pressed=true"#),
            "the spine says which screens the window is spending its width on: {html}"
        );

        assert!(
            html.contains(r#"data-slot="appearance""#)
                && html.contains(r#"aria-label="System appearance""#)
                && html.contains(r#"data-slot="indentation""#)
                && html.contains(">Pretty JSON<"),
            "appearance and indentation are named segmented groups: {html}"
        );

        // Connecting lives in the rail, not the bar.
        assert!(
            !html.contains("Connect…") && !html.contains("Disconnect"),
            "the bar names the connection and does not act on it: {html}"
        );
    }
}
