//! The rail: the Connection card, then tabs for the live Session's settings
//! and the two parties' capabilities, then the way to the command palette.
//! Only things scoped to the Connection or its Session belong here.
//! See `docs/architecture.md` §9 and ADR 0009.

use acp_inspector_core::{AgentCommand, ConnectionStatus, SessionSettings, Stated, v1};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdMaximize2, LdPower, LdRotateCw},
};

use crate::agent::{Advertiser, AgentClaims, Claims};
use crate::session_settings::{Choice, SessionSettings as SessionSettingsPanel};

/// Which of the rail's two lookups is on screen. Not persisted.
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

/// Which tab a key moves to from `selected`, if any: arrows to the other tab,
/// Home and End to the ends (as `console::tab_for_key`).
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

/// Whether the two accounts are open wide over the screens. Provided by the
/// window; a rail drawn without it offers no way there.
#[derive(Clone, Copy)]
pub struct WideCapabilities(pub Signal<bool>);

/// The wide capabilities sheet's element id.
const SHEET: &str = "capabilities-sheet";

impl WideCapabilities {
    /// Draws the sheet's rows and then shows it modal, so the platform's
    /// `<dialog>` owns focus, Escape and the backdrop.
    pub fn open(self) {
        let mut open = self.0;
        open.set(true);
        document::eval(&format!(
            "requestAnimationFrame(() => document.getElementById({SHEET:?})?.showModal())"
        ));
    }

    /// Shuts the sheet and gives the rows back to the rail.
    pub fn close(self) {
        let mut open = self.0;
        open.set(false);
        document::eval(&format!("document.getElementById({SHEET:?})?.close()"));
    }
}

/// The two accounts, side by side over the screens: the rail tab's rows with
/// room for all four facts. Rows are drawn only while open, and the rail's tab
/// says where they went, so no id is on the page twice.
#[component]
pub fn CapabilitiesSheet(claims: Claims) -> Element {
    let Some(wide) = try_use_context::<WideCapabilities>() else {
        return rsx! {};
    };
    let open = (wide.0)();
    rsx! {
        dialog {
            id: SHEET,
            class: "modal capabilities-sheet",
            "data-slot": "capabilities-sheet",
            aria_labelledby: "capabilities-sheet-title",
            // Escape closes the dialog natively; the rows go back to the rail.
            oncancel: move |_| {
                let mut open = wide.0;
                open.set(false);
            },
            div { class: "modal-box dialog-box sheet-box", tabindex: "-1", autofocus: true,
                header { class: "dialog-head",
                    h2 { id: "capabilities-sheet-title", "Capabilities" }
                    span { class: "grow" }
                    button {
                        class: "btn btn-sm btn-quiet btn-square",
                        r#type: "button",
                        aria_label: "Close",
                        title: "Close",
                        onclick: move |_| wide.close(),
                        {crate::disclosure::close()}
                    }
                }
                if open {
                    div { class: "dialog-body sheet-accounts",
                        AgentClaims { claims: claims.clone(), whose: Advertiser::Agent }
                        AgentClaims { claims, whose: Advertiser::Client }
                    }
                }
            }
            form { method: "dialog", class: "modal-backdrop",
                button { tabindex: "-1", aria_hidden: "true", onclick: move |_| { let mut open = wide.0; open.set(false); }, "close" }
            }
        }
    }
}

/// The rail's narrowest and widest, in CSS pixels.
pub const RAIL_MIN: u32 = 220;
pub const RAIL_MAX: u32 = 420;
/// The rail's width before anybody has dragged it: the drawing's.
pub const RAIL_DEFAULT: u32 = 244;
/// How far one arrow key moves the rail's edge.
const RAIL_STEP: u32 = 16;

/// Where the rail's width is remembered: the webview's `localStorage`, since
/// core's Settings has no layout preferences. A refusing store leaves the default.
const RAIL_KEY: &str = "acp-inspector.rail-width";

/// A width a reader asked for, held to the rail's range.
pub fn clamp_width(width: f64) -> u32 {
    (width.round().max(0.0) as u32).clamp(RAIL_MIN, RAIL_MAX)
}

/// Reads the remembered width, once, into `width`.
pub fn use_remembered_width(mut width: Signal<u32>) {
    use_hook(move || {
        spawn(async move {
            let read = document::eval(&format!(
                "try {{ return Number(localStorage.getItem({RAIL_KEY:?})) || null; }} catch (_) {{ return null; }}"
            ));
            if let Ok(serde_json::Value::Number(stored)) = read.await
                && let Some(stored) = stored.as_f64()
            {
                width.set(clamp_width(stored));
            }
        });
    });
}

fn remember_width(width: u32) {
    document::eval(&format!(
        "try {{ localStorage.setItem({RAIL_KEY:?}, \"{width}\"); }} catch (_) {{}}"
    ));
}

/// Follows the pointer from press to release in the page, setting the width
/// variable on every move without a render; only the final width is sent back.
const RESIZE: &str = r#"
const [min, max] = await dioxus.recv();
const handle = document.querySelector('[data-slot="rail-resize"]');
const body = handle?.closest('.body');
const rail = body?.querySelector('.rail');
if (!handle || !body || !rail) return;
const left = rail.getBoundingClientRect().left;
let width = rail.getBoundingClientRect().width;
const move = (event) => {
  width = Math.round(Math.min(max, Math.max(min, event.clientX - left)));
  body.style.setProperty('--rail-width', `${width}px`);
  handle.setAttribute('aria-valuenow', String(width));
};
const up = () => {
  document.removeEventListener('pointermove', move);
  document.removeEventListener('pointerup', up);
  document.documentElement.removeAttribute('data-resizing');
  dioxus.send(width);
};
document.documentElement.setAttribute('data-resizing', 'true');
document.addEventListener('pointermove', move);
document.addEventListener('pointerup', up);
"#;

/// The draggable edge between the rail and the screens, also moved with ←/→.
/// A focusable WAI-ARIA window separator with its value.
#[component]
pub fn RailHandle(width: Signal<u32>) -> Element {
    let mut width = width;
    let mut set = move |next: u32| {
        let next = next.clamp(RAIL_MIN, RAIL_MAX);
        if next != *width.peek() {
            width.set(next);
            remember_width(next);
        }
    };
    rsx! {
        div {
            class: "rail-resize",
            "data-slot": "rail-resize",
            role: "separator",
            aria_orientation: "vertical",
            aria_label: "Rail width",
            aria_controls: crate::navigation::RAIL,
            aria_valuemin: "{RAIL_MIN}",
            aria_valuemax: "{RAIL_MAX}",
            aria_valuenow: "{width}",
            tabindex: "0",
            title: "Drag, or use ←/→, to resize the rail",
            onpointerdown: move |event| {
                if event.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                    return;
                }
                event.prevent_default();
                let mut resizing = document::eval(RESIZE);
                let _ = resizing.send((RAIL_MIN, RAIL_MAX));
                spawn(async move {
                    if let Ok(done) = resizing.recv::<f64>().await {
                        set(clamp_width(done));
                    }
                });
            },
            onkeydown: move |event| {
                let now = *width.peek();
                let next = match event.key() {
                    Key::ArrowLeft => now.saturating_sub(RAIL_STEP),
                    Key::ArrowRight => now + RAIL_STEP,
                    Key::Home => RAIL_MIN,
                    Key::End => RAIL_MAX,
                    _ => return,
                };
                event.prevent_default();
                set(next);
            },
        }
    }
}

/// The rail.
#[component]
pub fn Rail(
    status: ConnectionStatus,
    /// The invocation the live agent was launched from, which the card names.
    launched: Option<AgentCommand>,
    session: Option<v1::SessionId>,
    /// What the two parties claimed, and the agent's affordances.
    claims: Claims,
    settings: SessionSettings,
    settings_epoch: u64,
    changing_mode: bool,
    setting_option: Option<v1::SessionConfigId>,
    /// How many slash commands the agent published.
    commands: usize,
    tab: Tab,
    on_tab: EventHandler<Tab>,
    on_stop: EventHandler<()>,
    /// Whether a launch is in flight, which is when Restart would race it.
    #[props(default)]
    launching: bool,
    /// Whether the agent this window disconnected is still being stopped
    /// (stdin closed, then SIGTERM, then SIGKILL).
    #[props(default)]
    stopping: bool,
    /// Relaunches the last invocation, keeping the trace.
    #[props(default)]
    on_restart: Option<EventHandler<()>>,
    on_set_mode: EventHandler<v1::SessionModeId>,
    on_set_config_option: EventHandler<Choice>,
    /// Closes the rail on Escape where it is a drawer. `None` where it is not.
    #[props(default)]
    on_dismiss: Option<EventHandler<()>>,
) -> Element {
    // While the capabilities are open wide, the tab says so instead of
    // drawing them twice.
    let wide = try_use_context::<WideCapabilities>();
    let widened = wide.is_some_and(|wide| (wide.0)());
    let connected = status == ConnectionStatus::Connected;
    let described = claims.agent.is_some();
    let on_new_session = claims.on_new_session;
    let title = claims.live_title();
    // Session-scoped surfaces need an agent or the record of one; before the
    // first launch they would only say "none".
    let active = connected || described || session.is_some();
    let restart = on_restart.filter(|_| launched.is_some() && !launching);

    rsx! {
        aside { class: "rail", "data-slot": "rail", aria_label: "Connection",
            id: crate::navigation::RAIL,
            tabindex: "-1",
            onkeydown: move |event| {
                if event.key() == Key::Escape
                    && let Some(dismiss) = on_dismiss
                {
                    event.prevent_default();
                    dismiss.call(());
                }
            },
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
                            {live(session.as_ref(), title, connected, described, on_new_session)}
                            if let Some(used) = claims.live_usage() {
                                {usage(used)}
                            }
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
                        Tab::Capabilities if widened => rsx! {
                            p { class: "hint", "data-slot": "capabilities-wide", "Shown open wide over the screens." }
                        },
                        Tab::Capabilities => rsx! {
                            // A capability row is four facts and the rail holds
                            // two; the sheet gives them the screens' width.
                            if let Some(wide) = wide {
                                div { class: "rail-head",
                                    span { class: "grow" }
                                    button {
                                        class: "btn btn-xs btn-quiet",
                                        "data-slot": "open-wide",
                                        r#type: "button",
                                        aria_haspopup: "dialog",
                                        title: "Read both accounts side by side, over the screens",
                                        onclick: move |_| wide.open(),
                                        Icon { class: "icon", width: 16, height: 16, icon: LdMaximize2 }
                                        "Open wide"
                                    }
                                }
                            }
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
                // The agent's published count; nothing is said about none.
                if active && commands > 0 {
                    p { class: "hint", "data-slot": "command-count",
                        if commands == 1 { "1 slash command · type /" } else { "{commands} slash commands · type /" }
                    }
                }
            }
        }
    }
}

/// The Connection card: its state, the transport, the invocation and the cwd.
/// The command is the one launched, not the form's fields, which stay editable
/// while an agent runs.
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
        crate::app::bar::connection_state(status)
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
                // The one transport this build has (§6.1).
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

/// The live Session, named by its title where the agent gave it one and by
/// its id always, and the two ways to another.
fn live(
    session: Option<&v1::SessionId>,
    title: Option<Stated>,
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
                        match title {
                            Some(Stated { value: Some(title), by }) => rsx! {
                                span { class: "session-tile-name", "data-slot": "session-title",
                                    title: crate::agent::whole(&title, by),
                                    "{title}"
                                }
                            },
                            _ => rsx! { span { class: "session-tile-name", "Open" } },
                        }
                        // Whole and copyable: it is what every other surface
                        // asks for.
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
                        // No roots: the roots control is in the dialog (§7.7).
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

/// How much of the live session's context window is in use and what the
/// session has cost, as the agent's last `usage_update` said.
///
/// Drawn only once one has arrived: an agent that sent none has said nothing,
/// which is not the same as saying zero. The timeline keeps every update; this
/// is the latest.
fn usage(usage: &v1::UsageUpdate) -> Element {
    let share = (usage.size > 0).then(|| usage.used as f64 / usage.size as f64);
    // The bar stops at full; the number says what the agent said.
    let fill = share.map_or(0.0, |share| share.min(1.0) * 100.0);
    let exact = format!("{} of {} tokens", usage.used, usage.size);

    rsx! {
        section { class: "rail-group", "data-slot": "usage",
            div { class: "rail-head",
                span { class: "eyebrow", "Context" }
                if let Some(share) = share {
                    span { class: "rail-count", "{(share * 100.0).round()}%" }
                }
            }
            div {
                class: "usage-meter",
                role: "meter",
                aria_label: "Context window in use",
                aria_valuemin: "0",
                aria_valuemax: "{usage.size}",
                aria_valuenow: "{usage.used}",
                aria_valuetext: "{exact}",
                style: "--used: {fill:.1}%",
            }
            p { class: "usage-facts",
                span { title: "{exact}", "{compact(usage.used)} / {compact(usage.size)} tokens" }
                if let Some(cost) = &usage.cost {
                    span { title: "Cumulative session cost", "{cost.amount} {cost.currency}" }
                }
            }
        }
    }
}

/// A token count in a few characters: `950`, `42.1k`, `1.2M`.
fn compact(count: u64) -> String {
    let (scaled, unit) = match count {
        0..1_000 => return count.to_string(),
        1_000..1_000_000 => (count as f64 / 1_000.0, "k"),
        _ => (count as f64 / 1_000_000.0, "M"),
    };
    let shown = format!("{scaled:.1}");
    format!("{}{unit}", shown.strip_suffix(".0").unwrap_or(&shown))
}

/// The launch fields' entry label, shared by the rail, the palette and the
/// empty timeline: Connect… before a launch, Edit command… after.
pub fn connect_label(launched: bool) -> &'static str {
    if launched {
        "Edit command…"
    } else {
        "Connect…"
    }
}

/// A launched invocation on one line, as typed at a shell.
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
    fn the_rails_width_is_held_to_its_range() {
        assert_eq!(clamp_width(100.0), RAIL_MIN);
        assert_eq!(clamp_width(-5.0), RAIL_MIN);
        assert_eq!(clamp_width(300.4), 300);
        assert_eq!(clamp_width(9999.0), RAIL_MAX);
    }

    #[test]
    fn the_rails_edge_is_a_separator_a_keyboard_can_move() {
        #[component]
        fn Host() -> Element {
            let width = use_signal(|| RAIL_DEFAULT);
            rsx! { RailHandle { width } }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        for wanted in [
            r#"role="separator""#,
            r#"aria-orientation="vertical""#,
            r#"aria-valuenow="244""#,
            r#"aria-valuemin="220""#,
            r#"aria-valuemax="420""#,
            r#"tabindex="0""#,
        ] {
            assert!(html.contains(wanted), "{wanted}: {html}");
        }
    }

    #[test]
    fn the_capabilities_open_wide_where_the_window_offers_it() {
        #[component]
        fn Host() -> Element {
            use_context_provider(|| WideCapabilities(Signal::new(false)));
            let claims = Claims::about(None);
            rsx! {
                Rail {
                    status: ConnectionStatus::Connected,
                    launched: None,
                    session: None,
                    claims: claims.clone(),
                    settings: SessionSettings::default(),
                    settings_epoch: 0,
                    changing_mode: false,
                    setting_option: None,
                    commands: 0,
                    tab: Tab::Capabilities,
                    on_tab: move |_| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_set_config_option: move |_| {},
                }
                CapabilitiesSheet { claims }
            }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(r#"data-slot="open-wide""#), "{html}");
        assert!(
            html.contains(r#"id="capabilities-sheet""#)
                && html.contains(r#"aria-labelledby="capabilities-sheet-title""#),
            "{html}"
        );
        // Closed, the sheet draws no rows, so nothing is on the page twice.
        assert_eq!(html.matches(r#"data-slot="claims""#).count(), 2, "{html}");
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
    fn the_live_session_is_named_by_the_title_the_agent_gave_it() {
        assert!(
            shown(Screen::default()).contains(r#"<span class="session-tile-name">Open</span>"#),
            "untitled, it is named by its state"
        );

        #[component]
        fn Titled() -> Element {
            let mut reports = acp_inspector_core::SessionReports::default();
            reports.info(
                &v1::SessionId::new("sess-1"),
                &v1::SessionInfoUpdate::new().title("Fix the flaky build"),
            );
            rsx! {
                Rail {
                    status: ConnectionStatus::Connected,
                    launched: Some(launched()),
                    session: Some(v1::SessionId::new("sess-1")),
                    claims: Claims {
                        session: Some(v1::SessionId::new("sess-1")),
                        reports,
                        ..Claims::about(Some(described()))
                    },
                    settings: SessionSettings::default(),
                    settings_epoch: 0,
                    changing_mode: false,
                    setting_option: None,
                    commands: 0,
                    tab: Tab::Session,
                    on_tab: move |_| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_set_config_option: move |_| {},
                }
            }
        }
        let mut dom = VirtualDom::new(Titled);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(
                "title=\"Fix the flaky build\nFrom session_info_update\">Fix the flaky build</span>"
            ) && html.contains(">sess-1<"),
            "titled, it is named by the title and still by its id: {html}"
        );
    }

    #[test]
    fn a_token_count_is_said_in_a_few_characters() {
        assert_eq!(compact(950), "950");
        assert_eq!(compact(42_120), "42.1k");
        assert_eq!(compact(200_000), "200k");
        assert_eq!(compact(1_250_000), "1.2M");
    }

    #[test]
    fn the_context_in_use_is_drawn_once_the_agent_has_said_it() {
        assert!(
            !shown(Screen::default()).contains(r#"data-slot="usage""#),
            "no usage_update, no readout: nothing was said, which is not zero"
        );

        let html =
            usage_shown(v1::UsageUpdate::new(42_120, 200_000).cost(v1::Cost::new(0.31, "USD")));
        assert!(
            html.contains(r#"role="meter""#)
                && html.contains(r#"aria-valuenow="42120""#)
                && html.contains(r#"aria-valuemax="200000""#)
                && html.contains(r#"aria-valuetext="42120 of 200000 tokens""#),
            "the meter carries the exact numbers: {html}"
        );
        assert!(
            html.contains(">21%<") && html.contains("42.1k / 200k tokens"),
            "{html}"
        );
        assert!(html.contains(">0.31 USD<"), "the cost as sent: {html}");

        let overfull = usage_shown(v1::UsageUpdate::new(300, 200));
        assert!(
            overfull.contains("--used: 100.0%") && overfull.contains(">150%<"),
            "the bar stops at full and the number does not: {overfull}"
        );
        assert!(
            !usage_shown(v1::UsageUpdate::new(10, 0)).contains("%<"),
            "no window, no share of it"
        );
    }

    /// The rail's session tab with `sess-1` live and `usage` its last update.
    fn usage_shown(usage: v1::UsageUpdate) -> String {
        #[component]
        fn Host(usage: v1::UsageUpdate) -> Element {
            let mut reports = acp_inspector_core::SessionReports::default();
            reports.usage(&v1::SessionId::new("sess-1"), &usage);
            rsx! {
                Rail {
                    status: ConnectionStatus::Connected,
                    launched: Some(launched()),
                    session: Some(v1::SessionId::new("sess-1")),
                    claims: Claims {
                        session: Some(v1::SessionId::new("sess-1")),
                        reports,
                        ..Claims::about(Some(described()))
                    },
                    settings: SessionSettings::default(),
                    settings_epoch: 0,
                    changing_mode: false,
                    setting_option: None,
                    commands: 0,
                    tab: Tab::Session,
                    on_tab: move |_| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_set_config_option: move |_| {},
                }
            }
        }
        let mut dom = VirtualDom::new_with_props(Host, HostProps { usage });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
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
        // The rail itself listens first (Escape closes its drawer); the
        // first tab is the next.
        let first = edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "keydown" => Some(*id),
                _ => None,
            })
            .nth(1)
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
        // The record outlives the agent (§5); only sending stops.
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
        // Both accounts, in the same rows.
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
        // The client's claim rides the request, so it is known before an agent.
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
