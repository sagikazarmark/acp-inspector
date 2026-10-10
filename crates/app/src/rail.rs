//! The rail: the Connection, and everything scoped to it
//! (`docs/architecture.md` §9, ADR 0009).
//!
//! **One column, three parts.** At the top, the Connection as a card — what it
//! is, what was spawned, where, and the two things that can be done to it.
//! Under that, a tabbed scroller holding the two things a reader looks *up*:
//! what the live Session is configured as, and what the two parties claimed
//! about themselves. At the foot, the way to every command the window has.
//!
//! **Why these and not others.** Everything in this rail is about the
//! Connection or the Session it holds; nothing in it is about the turn, which
//! is the screen beside it, or about the wire, which is the screen beside that.
//! That is the whole of the rule for what may be added here.
//!
//! **And why the two accounts came back to a rail.** They were a rail block in
//! the third ring, moved to the centre screen in the fifth because a capability
//! row is four facts and 288px holds two of them — measured then at 1430px of a
//! 1994px column, which is a specification standing between a reader and the
//! buttons under it. Both halves of that are still true and neither is a problem
//! here: the rows are two lines by construction (`input.css`), and they are
//! *behind a tab*, so their length costs the rail nothing until somebody asks
//! the question they answer. What the centre screen gets back is the whole width
//! for the turn, which is what the centre screen is for.

use acp_inspector_core::{AgentCommand, ConnectionStatus, SessionSettings, v1};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdPower, LdRotateCw},
};

use crate::agent::{Advertiser, AgentClaims, Claims};
use crate::session_settings::{Choice, SessionSettings as SessionSettingsPanel};

/// Which of the rail's two lookups is on screen.
///
/// Window-session state, like the Console's tab and for the same reason: what
/// somebody was reading yesterday is not what this window should open onto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    /// The live Session: which one it is, and what it is configured as.
    #[default]
    Session,
    /// What the agent said it can do, and what this client said it can do.
    Capabilities,
}

impl Tab {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Session => "rail-tab-session",
            Self::Capabilities => "rail-tab-capabilities",
        }
    }

    fn panel_id(self) -> &'static str {
        match self {
            Self::Session => "rail-panel-session",
            Self::Capabilities => "rail-panel-capabilities",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Capabilities => "Capabilities",
        }
    }
}

/// Which tab a key moves to from `selected`, if any: the arrow keys go to the
/// other tab, Home to the first and End to the last — the tab pattern the
/// Console's own tabs follow (`console::tab_for_key`).
fn tab_for_key(selected: Tab, key: &Key) -> Option<Tab> {
    match key {
        Key::ArrowLeft | Key::ArrowRight => Some(match selected {
            Tab::Session => Tab::Capabilities,
            Tab::Capabilities => Tab::Session,
        }),
        Key::Home => Some(Tab::Session),
        Key::End => Some(Tab::Capabilities),
        _ => None,
    }
}

fn focus_tab(tab: Tab) {
    // The id is one of the two literals above, never content from the Agent.
    let _ = document::eval(&format!(
        "document.getElementById({:?})?.focus();",
        tab.id()
    ));
}

/// The rail's Restart button, which focus goes to after a disconnect.
pub const RESTART: &str = "rail-restart";

/// The rail.
#[component]
pub fn Rail(
    status: ConnectionStatus,
    /// The invocation the live agent was launched from, which is what the card
    /// names: the command as it was typed, and the directory it was typed for.
    launched: Option<AgentCommand>,
    session: Option<v1::SessionId>,
    /// What the two parties claimed, and the affordances that hang off the
    /// agent's half (`agent::Claims`).
    claims: Claims,
    settings: SessionSettings,
    settings_epoch: u64,
    changing_mode: bool,
    setting_option: Option<v1::SessionConfigId>,
    /// What the agent said it can be asked to run, counted under the way into
    /// the palette — the drawing puts the number there, and it is the agent's.
    commands: usize,
    tab: Tab,
    on_tab: EventHandler<Tab>,
    on_stop: EventHandler<()>,
    /// Whether a launch is in flight, which is when Restart would race it.
    #[props(default)]
    launching: bool,
    /// Whether the agent this window disconnected is still being stopped
    /// (F6): stdin closed, then SIGTERM, then SIGKILL.
    #[props(default)]
    stopping: bool,
    /// Relaunch the last invocation, keeping the trace (X4).
    #[props(default)]
    on_restart: Option<EventHandler<()>>,
    on_set_mode: EventHandler<v1::SessionModeId>,
    on_set_config_option: EventHandler<Choice>,
) -> Element {
    let connected = status == ConnectionStatus::Connected;
    let described = claims.agent.is_some();
    let on_new_session = claims.on_new_session;
    // Session-scoped surfaces are for a window that has an agent, or the
    // record of one: before the first launch there is nothing for them to say
    // but "none", and a first run reads better without three of those (D10).
    let active = connected || described || session.is_some();
    let restart = on_restart.filter(|_| launched.is_some() && !launching);

    rsx! {
        aside { class: "rail", "data-slot": "rail", aria_label: "Connection",
            section { class: "rail-conn",
                span { class: "eyebrow", "Connection" }
                {connection(status, launched.as_ref(), connected, stopping, on_stop, restart)}
            }

            if active {
                div { class: "rail-tabs", role: "tablist", aria_label: "Details",
                    for choice in [Tab::Session, Tab::Capabilities] {
                        button {
                            key: "{choice.label()}",
                            id: choice.id(),
                            class: "tab rail-tab",
                            r#type: "button",
                            role: "tab",
                            aria_selected: choice == tab,
                            aria_controls: choice.panel_id(),
                            tabindex: if choice == tab { "0" } else { "-1" },
                            onclick: move |_| on_tab.call(choice),
                            // The tab pattern: the arrows move between tabs and
                            // select as they go, Home and End go to the ends.
                            onkeydown: move |event| {
                                if let Some(next) = tab_for_key(tab, &event.key()) {
                                    event.prevent_default();
                                    on_tab.call(next);
                                    focus_tab(next);
                                }
                            },
                            "{choice.label()}"
                        }
                    }
                }

                div {
                    id: tab.panel_id(),
                    class: "rail-body",
                    role: "tabpanel",
                    aria_labelledby: tab.id(),
                    match tab {
                        Tab::Session => rsx! {
                            {live(session.as_ref(), connected, described, on_new_session)}
                            if session.is_some() {
                                SessionSettingsPanel {
                                    settings,
                                    settings_epoch,
                                    changing_mode,
                                    setting_option,
                                    connected,
                                    on_set_mode,
                                    on_set_config_option,
                                }
                            }
                        },
                        Tab::Capabilities => rsx! {
                            AgentClaims { claims: claims.clone(), whose: Advertiser::Agent }
                            AgentClaims { claims, whose: Advertiser::Client }
                        },
                    }
                }
            } else {
                div { class: "rail-body" }
            }

            footer { class: "rail-foot",
                button {
                    class: "palette-open",
                    "data-slot": "open-palette",
                    r#type: "button",
                    aria_haspopup: "dialog",
                    aria_keyshortcuts: if crate::palette::shortcut::MAC { "Meta+K" } else { "Control+K" },
                    title: "Every command this window has, by name",
                    onclick: move |_| crate::palette::open(),
                    "Command palette"
                    kbd { class: "palette-key", {crate::palette::shortcut::PALETTE.here()} }
                }
                // The agent's own count, under the way to the list it is about:
                // what it published, not what this window can do. Nothing is
                // said about none.
                if active && commands > 0 {
                    p { class: "hint", "data-slot": "command-count",
                        if commands == 1 { "1 slash command · type /" } else { "{commands} slash commands · type /" }
                    }
                }
            }
        }
    }
}

/// The Connection, as the card at the top of the rail.
///
/// **What it says is what was asked for and what came back**: the state in the
/// word core uses for it, the transport this build has (`StdioSpawn` — the one
/// factory, §6.1), the invocation as it was typed, and the working directory a
/// session will be opened in. The command is the one the window launched rather
/// than whatever is in the form's fields, which are editable while an agent
/// runs.
fn connection(
    status: ConnectionStatus,
    launched: Option<&AgentCommand>,
    connected: bool,
    stopping: bool,
    on_stop: EventHandler<()>,
    on_restart: Option<EventHandler<()>>,
) -> Element {
    let (tone, said) = if stopping && !connected {
        ("idle", "stopping\u{2026}")
    } else {
        crate::connection_state(status)
    };
    let endpoint = launched.map(invocation).filter(|said| !said.is_empty());
    let cwd = launched
        .map(|command| command.cwd.trim())
        .filter(|cwd| !cwd.is_empty())
        .map(str::to_owned);

    rsx! {
        div { class: "tile", "data-slot": "connection",
            div { class: "tile-line",
                span { class: "dot dot-{tone}", aria_hidden: "true" }
                span { class: "tile-state {tone}", "data-slot": "connection-state", "{said}" }
                // The one transport this build has, named rather than assumed:
                // an agent reached some other way is a second factory behind
                // the same seam (§6.1), and this line is where it would say so.
                span { class: "tile-transport", "stdio" }
            }
            div { class: "tile-endpoint",
                match &endpoint {
                    Some(endpoint) => rsx! { "{endpoint}" },
                    None => rsx! { span { class: "cleared", "no agent launched" } },
                }
            }
            if let Some(cwd) = cwd {
                div { class: "tile-cwd", title: "{cwd}", "cwd {cwd}" }
            }
            div { class: "tile-actions stacked",
                button {
                    class: "btn btn-xs btn-quiet",
                    "data-slot": "configure",
                    r#type: "button",
                    aria_haspopup: "dialog",
                    title: "Open the launch fields",
                    onclick: move |_| crate::connect::open(),
                    {connect_label(launched.is_some())}
                }
                if connected || on_restart.is_some() {
                    div { class: "tile-actions",
                        if connected {
                            button {
                                class: "btn btn-xs btn-quiet btn-bad",
                                "data-slot": "rail-disconnect",
                                r#type: "button",
                                title: "End the connection and stop the agent",
                                onclick: move |_| on_stop.call(()),
                                span { class: "disclosure-icon", aria_hidden: "true",
                                    Icon { class: "icon", width: 16, height: 16, icon: LdPower }
                                }
                                "Disconnect"
                            }
                        }
                        if let Some(on_restart) = on_restart {
                            button {
                                id: RESTART,
                                class: "btn btn-xs btn-quiet btn-go",
                                "data-slot": "restart",
                                r#type: "button",
                                aria_keyshortcuts: if crate::palette::shortcut::MAC { "Meta+R" } else { "Control+R" },
                                title: "Run the same command again ({crate::palette::shortcut::RESTART.here()}). The trace is kept.",
                                onclick: move |_| on_restart.call(()),
                                span { class: "disclosure-icon", aria_hidden: "true",
                                    Icon { class: "icon", width: 16, height: 16, icon: LdRotateCw }
                                }
                                "Restart"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The live Session, and the two ways to another.
///
/// **The id is the name**, because that is what the agent minted and what every
/// call about this session carries; a title this window invented would be a
/// name nothing on the wire answers to.
fn live(
    session: Option<&v1::SessionId>,
    connected: bool,
    described: bool,
    on_new_session: EventHandler<Option<acp_inspector_core::Roots>>,
) -> Element {
    rsx! {
        section { class: "rail-group", "data-slot": "rail-session",
            div { class: "rail-head",
                span { class: "eyebrow", "Session" }
                span { class: "rail-count", if session.is_some() { "1 live" } else { "none" } }
            }
            div {
                class: if session.is_some() { "tile session-tile" } else { "tile session-tile none" },
                match session {
                    Some(session) => rsx! {
                        span { class: "session-tile-name", "Open" }
                        // **Whole and copyable, where the session is named.** It
                        // is the string every other surface asks for — a
                        // `session/load`, a log line, a bug report — so it is
                        // never elided and never only readable.
                        div { class: "tile-line",
                            span { class: "session-tile-id grow", "{session}" }
                            crate::copy::Copy { text: session.to_string(), what: "Session ID" }
                        }
                    },
                    None => rsx! {
                        span { class: "session-tile-name", "No session" }
                        span { class: "session-tile-id", "Not open" }
                    },
                }
            }
            if described {
                div { class: "tile-actions",
                    button {
                        class: "btn btn-xs btn-quiet",
                        "data-slot": "manage-sessions",
                        r#type: "button",
                        aria_haspopup: "dialog",
                        title: "Open, switch or end a session",
                        onclick: move |_| crate::session::open(),
                        "Manage sessions…"
                    }
                    button {
                        class: "btn btn-xs btn-quiet btn-go",
                        "data-slot": "rail-new-session",
                        r#type: "button",
                        title: "{v1::AGENT_METHOD_NAMES.session_new} — {crate::agent::SESSION_NEW_COSTS}",
                        aria_disabled: (!connected).then_some("true"),
                        tabindex: (!connected).then_some("-1"),
                        // No roots ride with it: the control that supplies them
                        // is in the dialog, and for a session that does not
                        // exist yet *none were asked for* is the true answer
                        // rather than *an empty control was* (§7.7).
                        onclick: move |_| {
                            if connected {
                                on_new_session.call(None);
                            }
                        },
                        "New session"
                    }
                }
            }
        }
    }
}

/// What the way to the launch fields is called: Connect… before anything was
/// launched, and Edit command… once there is a command to edit — one name per
/// state, the same in the rail, the palette and the empty timeline.
pub fn connect_label(launched: bool) -> &'static str {
    if launched {
        "Edit command…"
    } else {
        "Connect…"
    }
}

/// A launched invocation on one line, the way it would read if it had been
/// typed at a shell.
fn invocation(command: &AgentCommand) -> String {
    std::iter::once(command.command.trim())
        .chain(command.arguments())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use acp_inspector_core::{ProtocolVersion, v1};

    use super::*;

    fn launched() -> AgentCommand {
        AgentCommand {
            command: "/opt/agents/bin/testy".to_owned(),
            args: "--stdio".to_owned(),
            env: String::new(),
            cwd: "/srv/work".to_owned(),
            ..AgentCommand::default()
        }
    }

    fn described() -> v1::InitializeResponse {
        v1::InitializeResponse::new(ProtocolVersion::V1)
            .agent_info(v1::Implementation::new("testy", "1.2.3").title("Testy"))
    }

    #[derive(Clone, PartialEq)]
    struct Screen {
        status: ConnectionStatus,
        launched: bool,
        session: bool,
        agent: bool,
        tab: Tab,
        settings: bool,
        restart: bool,
        commands: usize,
        stopping: bool,
    }

    impl Default for Screen {
        fn default() -> Self {
            Self {
                status: ConnectionStatus::Connected,
                launched: true,
                session: true,
                agent: true,
                tab: Tab::Session,
                settings: true,
                restart: true,
                commands: 0,
                stopping: false,
            }
        }
    }

    fn shown(screen: Screen) -> String {
        #[component]
        fn Host(screen: Screen) -> Element {
            let mut settings = SessionSettings::default();
            if screen.settings {
                settings.modes = Some(v1::SessionModeState::new(
                    "chat",
                    vec![v1::SessionMode::new("chat", "Chat")],
                ));
            }
            rsx! {
                Rail {
                    status: screen.status,
                    launched: screen.launched.then(launched),
                    session: screen.session.then(|| v1::SessionId::new("sess-1")),
                    claims: Claims::about(screen.agent.then(described)),
                    settings,
                    settings_epoch: 0,
                    changing_mode: false,
                    setting_option: None,
                    commands: screen.commands,
                    tab: screen.tab,
                    on_tab: move |_| {},
                    on_stop: move |()| {},
                    stopping: screen.stopping,
                    on_restart: screen.restart.then(|| EventHandler::new(move |()| {})),
                    on_set_mode: move |_| {},
                    on_set_config_option: move |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new_with_props(Host, HostProps { screen });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_card_says_what_is_connected_and_what_it_was_launched_from() {
        let html = shown(Screen::default());
        assert!(
            html.contains(r#"data-slot="connection-state""#) && html.contains(">connected<"),
            "the state is core's word for it: {html}"
        );
        assert!(
            html.contains("/opt/agents/bin/testy --stdio"),
            "the invocation is the one the window launched, as it was typed: {html}"
        );
        assert!(
            html.contains("cwd /srv/work"),
            "and the directory a session will be opened in: {html}"
        );
        // The one transport this build has, named rather than assumed: an agent
        // reached some other way is a second factory behind the same seam.
        assert!(html.contains(">stdio<"), "{html}");
    }

    #[test]
    fn a_window_with_no_agent_offers_the_way_to_one_and_nothing_else() {
        let html = shown(Screen {
            status: ConnectionStatus::Disconnected,
            launched: false,
            session: false,
            agent: false,
            settings: false,
            ..Screen::default()
        });
        assert!(
            html.contains(">disconnected<") && html.contains("no agent launched"),
            "an absence is said rather than left as a gap: {html}"
        );
        assert!(
            html.contains(r#"data-slot="configure""#)
                && !html.contains(r#"data-slot="rail-disconnect""#),
            "there is nothing to end: {html}"
        );
        assert!(
            !html.contains(r#"data-slot="manage-sessions""#)
                && !html.contains(r#"data-slot="rail-new-session""#),
            "and nothing to open a session on until an agent has described itself: {html}"
        );
        assert!(
            !html.contains("No session")
                && !html.contains(r#"role="tablist""#)
                && !html.contains(r#"data-slot="command-count""#),
            "session-scoped surfaces wait for an agent (D10): {html}"
        );
        assert!(
            html.contains(">Connect…</button>") && !html.contains(r#"data-slot="restart""#),
            "the way in is named for what it does first, and there is nothing to restart: {html}"
        );
    }

    #[test]
    fn a_session_that_is_not_open_says_so_in_two_words() {
        let html = shown(Screen {
            session: false,
            ..Screen::default()
        });
        assert!(
            html.contains("No session") && html.contains(">Not open<"),
            "{html}"
        );
        assert!(
            html.contains(">Manage sessions…</button>")
                && html.contains(">New session</button>")
                && html.contains("title=\"session/new — "),
            "verbs on the buttons, the method in the tooltip: {html}"
        );
    }

    #[test]
    fn a_graceful_stop_in_progress_says_stopping_and_still_offers_restart() {
        let html = shown(Screen {
            status: ConnectionStatus::Disconnected,
            stopping: true,
            ..Screen::default()
        });
        assert!(
            html.contains(">stopping\u{2026}<") && html.contains(r#"id="rail-restart""#),
            "{html}"
        );
        assert!(!html.contains("Disconnect</button>"), "{html}");
    }

    #[test]
    fn restart_is_offered_wherever_there_is_a_command_to_run_again() {
        for status in [
            ConnectionStatus::Connected,
            ConnectionStatus::Lost,
            ConnectionStatus::FailedToStart,
            ConnectionStatus::Disconnected,
        ] {
            let html = shown(Screen {
                status,
                ..Screen::default()
            });
            assert!(
                html.contains(r#"id="rail-restart""#) && html.contains("Restart</button>"),
                "{status:?}: {html}"
            );
            assert!(
                html.contains(">Edit command…</button>") && !html.contains("reconnect"),
                "{status:?}: {html}"
            );
            assert_eq!(
                html.contains("Disconnect</button>"),
                status == ConnectionStatus::Connected,
                "only a live connection can be ended: {html}"
            );
        }
        let launching = shown(Screen {
            restart: false,
            ..Screen::default()
        });
        assert!(!launching.contains(r#"data-slot="restart""#), "{launching}");
    }

    #[test]
    fn the_slash_command_count_is_said_in_words_and_not_at_all_at_zero() {
        assert!(!shown(Screen::default()).contains(r#"data-slot="command-count""#));
        let html = shown(Screen {
            commands: 3,
            ..Screen::default()
        });
        assert!(html.contains("3 slash commands · type /"), "{html}");
        assert!(!html.contains("available_commands"), "{html}");
    }

    #[test]
    fn the_arrow_keys_home_and_end_move_between_the_rail_tabs() {
        assert_eq!(
            tab_for_key(Tab::Session, &Key::ArrowRight),
            Some(Tab::Capabilities)
        );
        assert_eq!(
            tab_for_key(Tab::Capabilities, &Key::ArrowLeft),
            Some(Tab::Session)
        );
        assert_eq!(
            tab_for_key(Tab::Capabilities, &Key::ArrowRight),
            Some(Tab::Session)
        );
        assert_eq!(
            tab_for_key(Tab::Capabilities, &Key::Home),
            Some(Tab::Session)
        );
        assert_eq!(
            tab_for_key(Tab::Session, &Key::End),
            Some(Tab::Capabilities)
        );
        assert_eq!(tab_for_key(Tab::Session, &Key::Enter), None);
    }

    #[test]
    fn a_rail_tab_answers_the_arrow_keys() {
        use std::{any::Any, rc::Rc};

        use dioxus::core::Mutation;
        use dioxus::html::{PlatformEventData, SerializedKeyboardData};

        #[component]
        fn Host() -> Element {
            let mut tab = use_signal(Tab::default);
            rsx! {
                Rail {
                    status: ConnectionStatus::Connected,
                    launched: None,
                    session: None,
                    claims: Claims::about(None),
                    settings: SessionSettings::default(),
                    settings_epoch: 0,
                    changing_mode: false,
                    setting_option: None,
                    commands: 0,
                    tab: tab(),
                    on_tab: move |chosen| tab.set(chosen),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_set_config_option: move |_| {},
                }
            }
        }
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(Host);
        let edits = dom.rebuild_to_vec().edits;
        let first = edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "keydown" => Some(*id),
                _ => None,
            })
            .expect("the tabs listen for keys");
        let key = SerializedKeyboardData::new(
            Key::ArrowRight,
            Code::ArrowRight,
            Location::Standard,
            false,
            Modifiers::empty(),
            false,
        );
        dom.runtime().handle_event(
            "keydown",
            Event::new(
                Rc::new(PlatformEventData::new(Box::new(key))) as Rc<dyn Any>,
                true,
            ),
            first,
        );
        dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"id="rail-panel-capabilities""#),
            "ArrowRight selected the next tab: {html}"
        );
    }

    #[test]
    fn an_agent_that_has_gone_leaves_the_record_and_takes_the_affordances() {
        // The settings and the listing outlive the agent that filled them on
        // purpose (§5); what changes is that nothing can be sent.
        let html = shown(Screen {
            status: ConnectionStatus::Lost,
            ..Screen::default()
        });
        assert!(html.contains(">connection lost<"), "{html}");
        assert!(
            html.contains("sess-1"),
            "the session it opened is still named: {html}"
        );
        let new_session = html
            .split("<button")
            .find(|button| button.contains(r#"data-slot="rail-new-session""#))
            .unwrap_or_default()
            .to_owned();
        assert!(
            new_session.contains(r#"aria-disabled="true""#)
                && new_session.contains(r#"tabindex="-1""#),
            "a control that cannot do what it says is drawn as one: {new_session}"
        );
    }

    #[test]
    fn the_two_lookups_are_tabs_and_each_names_the_panel_it_shows() {
        let session = shown(Screen::default());
        assert!(
            session.contains(r#"id="rail-tab-session""#)
                && session.contains(r#"aria-controls="rail-panel-session""#)
                && session.contains(r#"aria-selected=true"#),
            "the tab and its panel name each other: {session}"
        );
        assert!(
            session.contains(r#"data-slot="session-settings""#)
                && !session.contains(r#"data-slot="claims""#),
            "one at a time: {session}"
        );

        let capabilities = shown(Screen {
            tab: Tab::Capabilities,
            ..Screen::default()
        });
        assert!(
            capabilities.contains(r#"data-slot="claims""#)
                && !capabilities.contains(r#"data-slot="session-settings""#),
            "and the other says what the two parties claimed: {capabilities}"
        );
    }

    #[test]
    fn the_capabilities_tab_draws_both_accounts_and_neither_is_the_others_footnote() {
        // Who claimed a thing is the point of showing it (#86), so both are
        // drawn, in the same rows, one under the other.
        let html = shown(Screen {
            tab: Tab::Capabilities,
            ..Screen::default()
        });
        assert!(
            html.contains(r#"data-advertiser="agent""#)
                && html.contains(r#"data-advertiser="client""#),
            "both accounts are on the surface: {html}"
        );
        assert_eq!(
            html.matches(r#"data-claim="advertised""#).count(),
            2,
            "and they are the same block twice rather than a block and a footnote: {html}"
        );
    }

    #[test]
    fn this_clients_own_claim_does_not_wait_for_an_agent() {
        // It is fixed for every connection and rides the `initialize` request,
        // so it is knowable before there is an agent and unchanged by anything
        // one answers (§7.6).
        let html = shown(Screen {
            agent: false,
            session: false,
            settings: false,
            tab: Tab::Capabilities,
            ..Screen::default()
        });
        assert!(
            html.contains(r#"data-advertiser="client""#) && html.contains("configOptions"),
            "{html}"
        );
    }

    #[test]
    fn session_settings_are_drawn_only_where_there_is_a_session_to_have_any() {
        let none = shown(Screen {
            session: false,
            ..Screen::default()
        });
        assert!(
            !none.contains(r#"data-slot="session-settings""#),
            "a rail with no session draws no settings for one: {none}"
        );
    }

    #[test]
    fn the_way_to_every_command_is_at_the_foot_of_the_rail() {
        let html = shown(Screen::default());
        assert!(
            html.contains(r#"data-slot="open-palette""#)
                && html.contains(r#"aria-haspopup="dialog""#)
                && html.contains(&crate::palette::shortcut::PALETTE.here()),
            "the palette is named, reachable and says its shortcut: {html}"
        );
    }
}
