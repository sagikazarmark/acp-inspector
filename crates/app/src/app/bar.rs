//! The bar across the top of the window, and the title the window is given.

use acp_inspector_core::{AgentCommand, Appearance, ConnectionStatus, Indentation, v1};
use dioxus::prelude::*;

use super::layout::Spine;
use crate::appearance::AppearanceControl;
use crate::indent::IndentationControl;
use crate::navigation;
#[cfg(feature = "desktop")]
use crate::shell;

/// What the window is looking at, and how to say it in one line — or `None`,
/// where it is not looking at anything.
///
/// The Agent's own account of itself where it gave one, and the invocation it
/// was started from where it did not: an agent that sent no `agentInfo` is a
/// fact about that agent, and the command is still what the reader typed.
///
/// **Nothing launched is `None` and not this tool's own name.** The name is the
/// window's, and the window now says it in its own corner (`TopBar`): a
/// connection chip reading *ACP Inspector* beside a title reading *ACP
/// Inspector* was the tool introducing itself twice and the agent nowhere. The
/// title bar still falls back to it, because a task switcher entry has to say
/// something.
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

// And the frame the operating system draws, which is the one piece of this
// window's chrome the window does not paint. A desktop application names
// what it is looking at in its title bar — the task switcher, the dock and
// the window list all read it — and a tool that said only its own name
// there was a tool whose four open windows were four identical entries.
// Only where there is a title bar: another renderer's is not the shell's
// to set from here.
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

/// Whether the bar is the window's title bar (D17): the desktop build on
/// macOS, where `shell.rs` makes the title bar transparent over the content.
const TITLED: bool = cfg!(all(feature = "desktop", target_os = "macos"));

/// A press on the bar's empty space moves the window, where the bar is the
/// title bar. Only the primary button, and never from a control: the handlers
/// are on the bar's empty pieces and not on the bar.
fn drag(event: MouseEvent) {
    #[cfg(feature = "desktop")]
    if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
        shell::drag_window();
    }
    #[cfg(not(feature = "desktop"))]
    let _ = event;
}

/// And a double-click on it zooms, as one on a title bar does.
fn zoom(_: MouseEvent) {
    #[cfg(feature = "desktop")]
    shell::zoom_window();
}

/// The bar: the tool's own mark, what it is connected to, the two facts a
/// reader checks without looking away, and the switches that are about the
/// whole window.
///
/// **Connecting is not here.** The drawing puts what a connection *is* and what
/// can be done to it in one card at the top of the rail, which is where a
/// desktop tool puts a connection it holds one of; the bar names it and says
/// whether it is up.
#[component]
pub(crate) fn TopBar(
    status: ConnectionStatus,
    appearance: Appearance,
    /// Whether payloads are drawn indented (§8). In the bar beside the
    /// appearance, because it is the same kind of preference: one switch, about
    /// how the whole window reads.
    indentation: Indentation,
    /// What the window is looking at: the running Agent, or nothing at all
    /// before one has been launched. The tool's own name is the mark beside it
    /// and is never the subject.
    subject: Option<String>,
    /// What version of the protocol the agent answered `initialize` with, where
    /// one has. The agent's own number and not this client's claim.
    protocol: Option<String>,
    /// The live Session, named where the window's other fact is.
    session: Option<v1::SessionId>,
    spine: Spine,
    /// Whether the agent is waiting on the reader, which the Split choice
    /// carries while Trace only hides the timeline the request is on.
    #[props(default)]
    waiting: bool,
    on_spine: EventHandler<Spine>,
    on_choose: EventHandler<Appearance>,
    on_indent: EventHandler<Indentation>,
    /// Whether the rail is open as a drawer (D18), which the button that
    /// opens it says.
    #[props(default)]
    drawer: bool,
    /// Opens or closes the rail's drawer. The button is drawn by the
    /// stylesheet only where the rail is one.
    #[props(default)]
    on_drawer: Option<EventHandler<()>>,
) -> Element {
    let (tone, said) = connection_state(status);

    rsx! {
        header {
            // Where the toolbar is the title bar (D17, macOS), it leaves room
            // for the traffic lights and its empty space moves the window.
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
            div { class: "bar-brand", onmousedown: drag, ondoubleclick: zoom,
                span { class: "mark", aria_hidden: "true", "A" }
                h1 { "ACP Inspector" }
                span { class: "tag version", "v{env!(\"CARGO_PKG_VERSION\")}" }
            }
            // What is connected, as one chip: the state as a dot, the agent as
            // the name it gave, and the transport it was reached over.
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

            // The two protocol facts, in the register the wire is read in.
            div { class: "bar-facts", onmousedown: drag, ondoubleclick: zoom,
                if let Some(protocol) = protocol {
                    span { class: "bar-protocol", "protocol v{protocol}" }
                }
                if let Some(session) = session {
                    span { class: "bar-rule", aria_hidden: "true", "|" }
                    span { class: "bar-session", "{session}" }
                }
            }

            // Hidden below the width where one region shows at a time, where
            // it would choose between layouts nobody can see (X7).
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

/// The Connection, in the tone the window draws it and the word core gives it.
///
/// One table, because three surfaces say it: the chip in the bar, the card in
/// the rail, and the sentence a screen reader hears.
pub(crate) fn connection_state(status: ConnectionStatus) -> (&'static str, &'static str) {
    match status {
        ConnectionStatus::Connected => ("live", "connected"),
        ConnectionStatus::FailedToStart => ("gone", "failed to start"),
        ConnectionStatus::Lost => ("gone", "connection lost"),
        ConnectionStatus::Disconnected => ("idle", "disconnected"),
        // `ConnectionStatus` is non-exhaustive. A state this window has not
        // heard of must not be drawn as nothing, and must not be drawn as one
        // of the states it *has* heard of either.
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

        // Nothing launched: the tool's own name, which is the only subject
        // there is.
        assert_eq!(subject(None, None), None);

        // Launched, and the agent has not described itself yet — or described
        // itself without an `agentInfo`, which is a fact about that agent. The
        // command is what the reader typed and is still the subject.
        assert_eq!(subject(None, Some(&command)).as_deref(), Some("testy"));

        // And once it has said who it is, it is who it says it is.
        let described = v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1)
            .agent_info(v1::Implementation::new("testy", "1.2.3").title("Testy"));
        assert_eq!(
            subject(Some(&described), Some(&command)).as_deref(),
            Some("Testy 1.2.3")
        );

        // The title is optional in the protocol, and the name is what is left.
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

        // **The tool names itself once, in its own corner**: the mark and the
        // title are the window's identity, and everything to the right of them
        // is about the agent.
        assert!(
            html.contains(r#"<header class="bar"#)
                && html.contains(r#"class="mark""#)
                && html.contains("<h1>ACP Inspector</h1>")
                && html.matches("ACP Inspector").count() == 1,
            "the window says what it is once: {html}"
        );
        // The chip carries the connection: a dot, the state in words for a
        // reader who cannot see it, and the transport it was reached over.
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

        // The two protocol facts, and neither drawn when there is nothing to
        // say.
        assert!(
            html.contains("protocol v1") && !html.contains(r#"class="bar-session""#),
            "the bar names a session only where there is one: {html}"
        );

        // Which screens are on, as a segmented control: two exclusive states,
        // one of which is always true.
        assert!(
            html.contains(r#"data-slot="spine""#)
                && html.contains(">Split</button>")
                && html.contains(">Trace only</button>")
                && html.contains(r#"class="seg bar-layout" data-slot="layout-toggle""#)
                && html.contains(r#"aria-pressed=true"#),
            "the spine says which screens the window is spending its width on: {html}"
        );

        // And the two preferences beside it, each a group of exclusive answers
        // with the whole name on every one of them.
        assert!(
            html.contains(r#"data-slot="appearance""#)
                && html.contains(r#"aria-label="System appearance""#)
                && html.contains(r#"data-slot="indentation""#)
                && html.contains(">Pretty JSON<"),
            "appearance and indentation are named segmented groups: {html}"
        );

        // **Connecting is not in the bar.** What a connection is and what can
        // be done to it is one card at the top of the rail (ADR 0009).
        assert!(
            !html.contains("Connect…") && !html.contains("Disconnect"),
            "the bar names the connection and does not act on it: {html}"
        );
    }
}
