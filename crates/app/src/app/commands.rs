//! Every command the palette offers, built from handlers the window has.

use acp_inspector_core::{Appearance, Cursor, Roots, v1};
use dioxus::prelude::*;

use super::layout::{Focus, Spine};
use crate::console::Tab;
use crate::{agent, connect, palette, rail, session};

/// What the palette needs of the window: the state that decides which commands
/// there are, and the handlers they call.
///
/// A *wiring* rather than a state: nothing here is stored, and every field is
/// either something the window already renders from or a handler a control
/// already calls.
pub(crate) struct PaletteWiring {
    pub(crate) connected: bool,
    pub(crate) described: bool,
    pub(crate) traced: bool,
    pub(crate) spine: Spine,
    pub(crate) on_spine: EventHandler<Spine>,
    pub(crate) on_tab: EventHandler<Tab>,
    pub(crate) on_rail: EventHandler<rail::Tab>,
    pub(crate) on_new_session: EventHandler<Option<Roots>>,
    pub(crate) on_stop: EventHandler<()>,
    pub(crate) on_export: EventHandler<()>,
    pub(crate) on_clear: EventHandler<()>,
    pub(crate) on_indent: EventHandler<()>,
    pub(crate) on_appearance: EventHandler<Appearance>,
}

/// The rest of what the palette offers (X9): the commands that came after the
/// first wiring, kept apart so a fixture that draws the palette without them
/// still builds the same list (`mock.rs`).
pub(crate) struct PaletteExtras {
    /// Whether there is an invocation to run again, and no launch in flight.
    pub(crate) launched: bool,
    pub(crate) turn_running: bool,
    pub(crate) session: Option<v1::SessionId>,
    pub(crate) lists_sessions: bool,
    /// The agent's advertised authentication methods, by id and name, while it
    /// is connected.
    pub(crate) auth_methods: Vec<(v1::AuthMethodId, String)>,
    /// The frame of the newest request waiting on the reader.
    pub(crate) waiting: Option<u64>,
    pub(crate) indented: bool,
    pub(crate) appearance: Appearance,
    pub(crate) on_region: EventHandler<Focus>,
    pub(crate) on_restart: EventHandler<()>,
    pub(crate) on_cancel_turn: EventHandler<()>,
    pub(crate) on_list: EventHandler<Option<Cursor>>,
    pub(crate) on_authenticate: EventHandler<v1::AuthMethodId>,
    pub(crate) on_waiting: EventHandler<u64>,
    pub(crate) on_prompt: EventHandler<()>,
    pub(crate) on_copy_session: EventHandler<()>,
}

impl PaletteExtras {
    /// A window with nothing launched, nothing running and nothing waiting.
    #[cfg(test)]
    pub(crate) fn idle() -> Self {
        Self {
            launched: false,
            turn_running: false,
            session: None,
            lists_sessions: false,
            auth_methods: Vec::new(),
            waiting: None,
            indented: false,
            appearance: Appearance::default(),
            on_region: EventHandler::new(|_| {}),
            on_restart: EventHandler::new(|()| {}),
            on_cancel_turn: EventHandler::new(|()| {}),
            on_list: EventHandler::new(|_| {}),
            on_authenticate: EventHandler::new(|_| {}),
            on_waiting: EventHandler::new(|_| {}),
            on_prompt: EventHandler::new(|()| {}),
            on_copy_session: EventHandler::new(|()| {}),
        }
    }
}

/// The palette's commands for a window with nothing beyond the first wiring.
#[cfg(test)]
pub(crate) fn commands(wiring: PaletteWiring) -> Vec<palette::Command> {
    commands_with(wiring, PaletteExtras::idle())
}

/// Every command the palette offers (`palette.rs`).
///
/// **Gated exactly as the controls are.** A command that needs an agent is not
/// offered without one, for the reason a button that cannot be pressed is not
/// drawn. What is *never* gated on state the protocol leaves to the agent — a
/// session that cannot be opened, a setting that will be refused — stays
/// exactly where §7.3's rule puts it: on the affordance, offered, and answered
/// by the agent.
pub(crate) fn commands_with(wiring: PaletteWiring, extras: PaletteExtras) -> Vec<palette::Command> {
    use palette::{Command, shortcut};

    let PaletteWiring {
        connected,
        described,
        traced,
        spine,
        on_spine,
        on_tab,
        on_rail,
        on_new_session,
        on_stop,
        on_export,
        on_clear,
        on_indent,
        on_appearance,
    } = wiring;
    let PaletteExtras {
        launched,
        turn_running,
        session,
        lists_sessions,
        auth_methods,
        waiting,
        indented,
        appearance,
        on_region,
        on_restart,
        on_cancel_turn,
        on_list,
        on_authenticate,
        on_waiting,
        on_prompt,
        on_copy_session,
    } = extras;
    let mut commands = vec![Command::new(
        "Connection",
        rail::connect_label(launched || connected),
        if launched || connected {
            "Change the command, or run one you have run before"
        } else {
            "Start an agent, or run one you have run before"
        },
        EventHandler::new(move |()| connect::open()),
    )];

    if launched {
        commands.push(
            Command::new(
                "Connection",
                "Restart",
                "Run the same command again. The trace is kept",
                on_restart,
            )
            .shortcut(shortcut::RESTART),
        );
    }
    if connected {
        commands.push(Command::new(
            "Connection",
            "Disconnect",
            "End the connection and stop the agent",
            on_stop,
        ));
    }

    if turn_running {
        commands.push(
            Command::new(
                "Session",
                "Cancel turn",
                "Ask the agent to stop the running prompt (session/cancel)",
                on_cancel_turn,
            )
            .shortcut(shortcut::CANCEL_TURN),
        );
    }
    if let Some(ordinal) = waiting {
        commands.push(Command::new(
            "Session",
            "Go to waiting request",
            "The request the agent is waiting on you to answer",
            EventHandler::new(move |()| on_waiting.call(ordinal)),
        ));
    }
    if session.is_some() {
        commands.push(Command::new(
            "Session",
            "Focus prompt",
            "Put the keyboard in the prompt",
            on_prompt,
        ));
    }
    if described {
        commands.push(
            Command::new(
                "Session",
                "New session",
                &format!(
                    "{} ({})",
                    agent::SESSION_NEW_COSTS,
                    v1::AGENT_METHOD_NAMES.session_new
                ),
                EventHandler::new(move |()| on_new_session.call(None)),
            )
            .shortcut(shortcut::NEW_SESSION),
        );
        commands.push(Command::new(
            "Session",
            "Manage sessions…",
            "Open, switch or end a session",
            EventHandler::new(move |()| session::open()),
        ));
        if lists_sessions && connected {
            commands.push(Command::new(
                "Session",
                "List sessions",
                "Ask the agent which sessions it has (session/list)",
                EventHandler::new(move |()| {
                    session::open();
                    on_list.call(None);
                }),
            ));
        }
    }
    if session.is_some() {
        commands.push(Command::new(
            "Session",
            "Copy session ID",
            "Put the live session's id on the clipboard",
            on_copy_session,
        ));
    }
    for (method, name) in auth_methods {
        commands.push(Command::new(
            "Session",
            &format!("Authenticate with {name}"),
            &format!("Send authenticate with the method {method}"),
            EventHandler::new(move |()| on_authenticate.call(method.clone())),
        ));
    }

    commands.push(
        Command::new(
            "Trace",
            "Export…",
            "Write every frame to a JSONL file, nothing redacted",
            on_export,
        )
        .shortcut(shortcut::EXPORT),
    );
    if traced {
        commands.push(
            Command::new(
                "Trace",
                "Clear",
                "Drop the frames captured so far; exports already written are untouched",
                on_clear,
            )
            .shortcut(shortcut::CLEAR),
        );
    }

    for (chosen, label, says) in [
        (
            Focus::Turn,
            "Show timeline",
            "Show the conversation and the prompt",
        ),
        (
            Focus::Rail,
            "Show details",
            "Show the connection, session and capabilities",
        ),
    ] {
        commands.push(Command::new(
            "View",
            label,
            says,
            EventHandler::new(move |()| on_region.call(chosen)),
        ));
    }
    for (chosen, says, chord) in [
        (
            Tab::Trace,
            "Show every frame that crossed the wire",
            shortcut::SHOW_TRACE,
        ),
        (
            Tab::Diagnostics,
            "Show everything the transport said that was not a frame",
            shortcut::SHOW_DIAGNOSTICS,
        ),
    ] {
        commands.push(
            Command::new(
                "View",
                match chosen {
                    Tab::Trace => "Trace",
                    Tab::Diagnostics => "Diagnostics",
                },
                says,
                EventHandler::new(move |()| on_tab.call(chosen)),
            )
            .shortcut(chord),
        );
    }
    for chosen in Spine::ALL {
        if chosen != spine {
            commands.push(
                Command::new(
                    "View",
                    chosen.label(),
                    chosen.says(),
                    EventHandler::new(move |()| on_spine.call(chosen)),
                )
                .shortcut(shortcut::TRACE_ONLY),
            );
        }
    }
    if connected || described || session.is_some() {
        for (chosen, label, says) in [
            (
                rail::Tab::Session,
                "Session settings",
                "What the live session is configured as",
            ),
            (
                rail::Tab::Capabilities,
                "Capabilities",
                "What the agent claimed, and what this client claimed",
            ),
        ] {
            commands.push(Command::new(
                "View",
                label,
                says,
                EventHandler::new(move |()| on_rail.call(chosen)),
            ));
        }
    }

    commands.push(
        Command::new(
            "Window",
            "Pretty JSON",
            "Draw payloads indented. What crossed the wire is unchanged",
            on_indent,
        )
        .state(indented),
    );
    for chosen in Appearance::ALL {
        commands.push(
            Command::new(
                "Window",
                &format!("{} appearance", chosen.label()),
                "How this window is painted",
                EventHandler::new(move |()| on_appearance.call(chosen)),
            )
            .state(chosen == appearance),
        );
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_offers_every_command_the_window_has_now() {
        #[component]
        fn Host(connected: bool) -> Element {
            let extras = PaletteExtras {
                launched: connected,
                turn_running: connected,
                session: connected.then(|| v1::SessionId::new("sess-1")),
                lists_sessions: connected,
                auth_methods: if connected {
                    vec![(v1::AuthMethodId::new("login"), "Agent login".to_owned())]
                } else {
                    Vec::new()
                },
                waiting: connected.then_some(7),
                indented: true,
                ..PaletteExtras::idle()
            };
            let commands = commands_with(
                PaletteWiring {
                    connected,
                    described: connected,
                    traced: connected,
                    spine: Spine::Split,
                    on_spine: EventHandler::new(|_| {}),
                    on_tab: EventHandler::new(|_| {}),
                    on_rail: EventHandler::new(|_| {}),
                    on_new_session: EventHandler::new(|_| {}),
                    on_stop: EventHandler::new(|()| {}),
                    on_export: EventHandler::new(|()| {}),
                    on_clear: EventHandler::new(|()| {}),
                    on_indent: EventHandler::new(|()| {}),
                    on_appearance: EventHandler::new(|_| {}),
                },
                extras,
            );
            rsx! {
                for command in commands {
                    p { "{command.label}|{command.shortcut.clone().unwrap_or_default()}|{command.state.map(|on| on.to_string()).unwrap_or_default()}" }
                }
            }
        }
        fn rows(connected: bool) -> String {
            let mut dom = VirtualDom::new_with_props(Host, HostProps { connected });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        }

        let live = rows(true);
        for (label, shortcut) in [
            ("Edit command…", None),
            ("Restart", Some(palette::shortcut::RESTART)),
            ("Disconnect", None),
            ("Cancel turn", Some(palette::shortcut::CANCEL_TURN)),
            ("Go to waiting request", None),
            ("Focus prompt", None),
            ("New session", Some(palette::shortcut::NEW_SESSION)),
            ("Manage sessions…", None),
            ("List sessions", None),
            ("Copy session ID", None),
            ("Authenticate with Agent login", None),
            ("Export…", Some(palette::shortcut::EXPORT)),
            ("Clear", Some(palette::shortcut::CLEAR)),
            ("Show timeline", None),
            ("Show details", None),
            ("Trace only", Some(palette::shortcut::TRACE_ONLY)),
            ("Capabilities", None),
        ] {
            let shortcut = shortcut.map(|chord| chord.here()).unwrap_or_default();
            assert!(
                live.contains(&format!("<p>{label}|{shortcut}|")),
                "{label} is offered with its shortcut: {live}"
            );
        }
        assert!(live.contains("<p>Pretty JSON||true</p>"), "{live}");
        assert!(live.contains("<p>System appearance||true</p>"), "{live}");

        let idle = rows(false);
        assert!(idle.contains("<p>Connect…||</p>"), "{idle}");
        for absent in [
            "Restart",
            "Disconnect",
            "Cancel turn",
            "New session",
            "Copy session ID",
            "Capabilities",
            "Clear",
        ] {
            assert!(
                !idle.contains(&format!("<p>{absent}|")),
                "{absent} needs an agent: {idle}"
            );
        }
    }
}
