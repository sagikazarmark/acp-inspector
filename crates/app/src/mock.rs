//! The window, rendered to a file, so a visual decision can be looked at.
//!
//! **Not a test of anything.** `design/inspector-visual-audit.md` is a record of
//! screenshots somebody took by hand from a running WebView, which is why it
//! goes stale the moment a rail moves: recapturing it means building the shell,
//! launching an agent and driving it to a populated state. This renders the same
//! screens from fixtures with no window, no agent and no runtime under them —
//! the components themselves, the committed stylesheet, and one populated
//! session's worth of made-up traffic.
//!
//! What it cannot show is what only an engine knows: real fonts, real
//! scrollbars, the disclosure that is a `<details>`. What it does show is
//! layout, colour, density and state, in both schemes, which is what a visual
//! pass is about.
//!
//! ```text
//! INSPECTOR_MOCK=target/mock cargo test -p acp-inspector mock::writes
//! ```
//!
//! It writes nothing unless that variable is set, so an ordinary `cargo test`
//! runs it as a render that is thrown away — which is the assertion worth
//! having here: every screen still renders.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use acp_inspector_core::{
    AgentCommand, Appearance, ConnectionStatus, Diagnostic, DiagnosticKind, Direction, EntryId,
    EntryKind, Frame, Indentation, Recorded, SessionSettings, TimelineEntry, TracedFrame,
    TurnState, Unrecognized, v1,
};
use dioxus::prelude::*;

use crate::connect;
use crate::console::{Console, Tab};
use crate::rail::Rail;
use crate::timeline::Timeline;
use crate::{Focus, Spine, TopBar, rail, style};

/// When the fixture traffic crossed. Fixed, so two renders of the same screen
/// are the same bytes and a diff between them is a change somebody made.
fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(73_200 + seconds)
}

fn entry(ordinal: u64, kind: EntryKind) -> TimelineEntry {
    TimelineEntry {
        id: EntryId::Arrival(ordinal),
        at: at(ordinal),
        frames: vec![Recorded {
            frame: Frame::new(r#"{"jsonrpc":"2.0","method":"session/update"}"#),
            captured: Some(ordinal),
        }],
        kind,
        turn: Some(1),
    }
}

fn said(ordinal: u64, text: &str) -> TimelineEntry {
    entry(
        ordinal,
        EntryKind::Update(v1::SessionUpdate::AgentMessageChunk(
            v1::ContentChunk::new(v1::ContentBlock::from(text))
                .message_id(v1::MessageId::new("m1")),
        )),
    )
}

/// One populated session: a prompt, an answer, a thought, a tool call the agent
/// finished, a plan and a frame nobody has a name for.
fn entries() -> Vec<TimelineEntry> {
    vec![
        entry(
            1,
            EntryKind::Update(v1::SessionUpdate::UserMessageChunk(v1::ContentChunk::new(
                v1::ContentBlock::from("Read the config and tell me what the retry budget is."),
            ))),
        ),
        entry(
            2,
            EntryKind::Update(v1::SessionUpdate::AgentThoughtChunk(v1::ContentChunk::new(
                v1::ContentBlock::from("The budget is likely under transport rather than retry."),
            ))),
        ),
        entry(
            3,
            EntryKind::Update(v1::SessionUpdate::ToolCall(
                v1::ToolCall::new("call-1", "Apply deterministic edit")
                    .kind(v1::ToolKind::Edit)
                    .status(v1::ToolCallStatus::Completed),
            )),
        ),
        said(
            4,
            "The retry budget is four attempts with a 250ms floor, set in transport.rs.",
        ),
        // And the thing a flat list could not show: the agent went on talking
        // after it said the turn was over.
        TimelineEntry {
            turn: None,
            ..entry(
                5,
                EntryKind::Unrecognized(Unrecognized::NotServiced {
                    method: "session/experimental_progress".to_owned(),
                }),
            )
        },
    ]
}

fn frames() -> Vec<TracedFrame> {
    [
        (Direction::ToAgent, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}}}"#),
        (Direction::FromAgent, r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"promptCapabilities":{"image":true,"audio":true}}}}"#),
        (Direction::ToAgent, r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/home/laborant/dioxus-chat.orig/phoenix/inspector","mcpServers":[]}}"#),
        (Direction::FromAgent, r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"testy-session-1","modes":{"currentModeId":"chat","availableModes":[{"id":"chat","name":"Chat"}]}}}"#),
        (Direction::ToAgent, r#"{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"testy-session-1","prompt":[{"type":"text","text":"Read the config"}]}}"#),
        (Direction::FromAgent, r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"testy-session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"The retry budget"}}}}"#),
    ]
    .into_iter()
    // Repeated until the list overflows: what a scroller does at its end is
    // only visible on a list long enough to have one.
    .cycle()
    .take(36)
    .enumerate()
    .map(|(ordinal, (direction, json))| TracedFrame {
        at: at(ordinal as u64),
        connection: 1,
        direction,
        frame: Frame::new(json),
    })
    .collect()
}

/// The other half of the Console: what the agent said about itself that was not
/// a frame, beside what the transport said about the agent.
fn lines() -> Vec<Diagnostic> {
    [
        DiagnosticKind::Stderr("testy: listening on stdio".to_owned()),
        DiagnosticKind::Stderr(
            "testy: warning: MCP server list was empty, continuing without one".to_owned(),
        ),
        DiagnosticKind::Stderr("testy: session testy-session-1 opened".to_owned()),
    ]
    .into_iter()
    .enumerate()
    .map(|(ordinal, kind)| Diagnostic {
        at: at(ordinal as u64),
        kind,
    })
    .collect()
}

fn described() -> v1::InitializeResponse {
    v1::InitializeResponse::new(acp_inspector_core::ProtocolVersion::V1)
        .agent_capabilities(
            v1::AgentCapabilities::new()
                .load_session(true)
                .prompt_capabilities(v1::PromptCapabilities::new().image(true).audio(true))
                .session_capabilities(
                    v1::SessionCapabilities::new()
                        .list(v1::SessionListCapabilities::new())
                        .close(v1::SessionCloseCapabilities::new()),
                ),
        )
        .agent_info(v1::Implementation::new("testy", "1.2.3").title("Testy"))
        .auth_methods(vec![v1::AuthMethod::Agent(v1::AuthMethodAgent::new(
            "testy-agent-auth",
            "Testy agent auth",
        ))])
}

/// What a `session/list` came back with: two sessions, one of them the live
/// one, so the dialog draws both states of a row.
fn listed() -> acp_inspector_core::SessionListing {
    let mut listing = acp_inspector_core::SessionListing::default();
    listing.sessions = vec![
        v1::SessionInfo::new("testy-session-1", "/home/laborant/phoenix/inspector")
            .title("Read the config"),
        v1::SessionInfo::new("testy-session-2", "/home/laborant/phoenix/bridge"),
    ];
    listing
}

fn configured() -> SessionSettings {
    let mut settings = SessionSettings::default();
    settings.modes = Some(v1::SessionModeState::new(
        "chat",
        vec![
            v1::SessionMode::new("chat", "Chat"),
            v1::SessionMode::new("plan", "Plan"),
        ],
    ));
    settings.config_options = Some(vec![v1::SessionConfigOption::select(
        "verbosity",
        "Verbosity",
        "verbose",
        vec![
            v1::SessionConfigSelectOption::new("brief", "Brief"),
            v1::SessionConfigSelectOption::new("normal", "Normal"),
            v1::SessionConfigSelectOption::new("verbose", "Verbose"),
        ],
    )]);
    settings
}

fn launched() -> AgentCommand {
    AgentCommand {
        command: "/home/laborant/dioxus-chat.orig/phoenix/inspector/.testy/bin/testy".to_owned(),
        args: String::new(),
        env: String::new(),
        cwd: "/home/laborant/dioxus-chat.orig/phoenix/inspector".to_owned(),
    }
}

/// The whole window, from fixtures — the same regions `App` composes, with the
/// stores replaced by values.
#[component]
fn Window(view: rail::Tab, live: bool) -> Element {
    // **A window with nothing behind it has nothing in it.** The first-run page
    // was drawn from the populated fixtures with only the toolbar and the
    // claims told that no agent was running, so the screen it exists to show —
    // the one a reader meets before they have launched anything — arrived as a
    // full Timeline, a live Session's Settings and thirty-six Frames under a
    // bar saying *Disconnected*. It was the one screen nobody had looked at,
    // which is what its own comment says, and rendering it that way is how it
    // stayed that way.
    let entries = use_signal(|| if live { entries() } else { Vec::new() });
    let frames = use_signal(|| if live { frames() } else { Vec::new() });
    let lines = use_signal(|| if live { lines() } else { Vec::new() });
    let revealed = use_signal(Vec::<u64>::new);
    let decoded = use_signal(HashSet::<u64>::new);

    // One value for the two surfaces that draw it, exactly as the window builds
    // it: the Agent screen says what was claimed, the dialog acts on what is
    // there.
    let claims = crate::agent::Claims {
        session: Some(v1::SessionId::new("testy-session-1")),
        listing: live.then(listed),
        // The first-run page is a window with no agent behind it, so the
        // surfaces that act on one are drawn the way that window draws them:
        // the record still there, the controls inert, and each surface saying
        // which of the two it is.
        connected: live,
        ..crate::agent::Claims::about(live.then(described))
    };

    rsx! {
        TopBar {
            status: if live { ConnectionStatus::Connected } else { ConnectionStatus::Disconnected },
            appearance: Appearance::System,
            // The window as it opens: the wire, which is both the default and
            // the state a visual pass should be looking at, since it is what
            // every screen under this bar is drawn from (§8).
            indentation: Indentation::Wire,
            subject: crate::subject(
                live.then(described).as_ref(),
                live.then(launched).as_ref(),
            ),
            protocol: live.then(|| "1".to_owned()),
            session: live.then(|| v1::SessionId::new("testy-session-1")),
            spine: Spine::Split,
            on_spine: move |_| {},
            on_choose: move |_| {},
            on_indent: move |_| {},
        }
        connect::Connect {
            running: live,
            launching: false,
            recent: vec![launched()],
            unsaved: None,
            on_launch: move |_| {},
        }
        div { class: Focus::Turn.class(),
            Rail {
                status: if live { ConnectionStatus::Connected } else { ConnectionStatus::Disconnected },
                launched: live.then(launched),
                session: live.then(|| v1::SessionId::new("testy-session-1")),
                claims: claims.clone(),
                settings: configured(),
                settings_epoch: 0,
                changing_mode: false,
                setting_option: None,
                commands: 0,
                tab: view,
                on_tab: move |_| {},
                on_stop: move |()| {},
                on_set_mode: move |_| {},
                on_set_config_option: move |_| {},
            }
            div { class: Spine::Split.class(),
                Timeline {
                    entries,
                    turn: TurnState::Idle,
                    session: live.then(|| v1::SessionId::new("testy-session-1")),
                    connected: live,
                    held_frames: 0..6,
                    blocked: false,
                    problem: None,
                    // The same call the window makes, so the fixture draws the
                    // composer's mode control where the window has one.
                    mode: crate::cycle(&configured(), live),
                    on_prompt: move |_| {},
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_| {},
                    on_elicit: move |_: crate::elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_| {},
                    described: live,
                    turns: vec![acp_inspector_core::Turn {
                        id: 1,
                        at: at(6),
                        outcome: Some(acp_inspector_core::TurnOutcome::Ended(v1::StopReason::EndTurn)),
                    }],
                }
                Console {
                    frames,
                    dropped_frames: 0,
                    saved: None,
                    on_export: move |()| {},
                    on_clear: move |()| {},
                    lines,
                    dropped_lines: 0,
                    tab: if view == rail::Tab::Session { Tab::Trace } else { Tab::Diagnostics },
                    on_select: move |_| {},
                    revealed,
                    decoded,
                    on_seek: move |_: u64| {},
                }
            }
        }

        // The way between the three regions on a window too narrow for them,
        // which the stylesheet draws only below the width where they stop
        // fitting — and which the narrow captures are taken to show.
        div { class: "mobile-bar seg", role: "group", aria_label: "Which region is shown",
            for chosen in Focus::ALL {
                button {
                    key: "{chosen.label()}",
                    class: "seg-item",
                    "data-slot": "focus",
                    r#type: "button",
                    aria_pressed: chosen == Focus::Turn,
                    "{chosen.label()}"
                }
            }
        }

        crate::session::Sessions { claims }

        // Every command the window has, from the same list the window builds:
        // the fixture draws the surface rather than a hand-written stand-in for
        // it, so a capture of the palette is a capture of what is there.
        crate::palette::Palette {
            commands: crate::commands(crate::PaletteWiring {
                connected: live,
                described: live,
                traced: live,
                spine: Spine::Split,
                on_spine: EventHandler::new(move |_| {}),
                on_tab: EventHandler::new(move |_| {}),
                on_rail: EventHandler::new(move |_| {}),
                on_new_session: EventHandler::new(move |_| {}),
                on_stop: EventHandler::new(move |()| {}),
                on_export: EventHandler::new(move |()| {}),
                on_clear: EventHandler::new(move |()| {}),
                on_indent: EventHandler::new(move |()| {}),
                on_appearance: EventHandler::new(move |_| {}),
            }),
        }
    }
}

/// The rendered window inside the document the shell gives it, in one scheme.
fn page(theme: &str, view: rail::Tab, live: bool) -> String {
    let mut dom = VirtualDom::new_with_props(Window, WindowProps { view, live });
    dom.rebuild_in_place();
    let body = dioxus_ssr::render(&dom);
    format!(
        "<!doctype html>\n<html lang=\"en\" data-theme=\"{theme}\">\n<head>\n<meta charset=\"utf-8\">\n<title>ACP Inspector — {theme}</title>\n<style>{sheet}</style>\n</head>\n<body>\n<div id=\"main\">{body}</div>\n</body>\n</html>\n",
        sheet = style::SHEET,
    )
}

#[test]
fn writes_the_window_in_both_schemes() {
    for theme in ["inspector-light", "inspector-dark"] {
        let trace = page(theme, rail::Tab::Session, true);
        assert!(
            trace.contains(r#"data-slot="rail""#)
                && trace.contains(r#"class="turns""#)
                && trace.contains(r#"class="wire""#),
            "the populated window renders the rail and both screens"
        );
        if let Ok(directory) = std::env::var("INSPECTOR_MOCK") {
            std::fs::create_dir_all(&directory).expect("the mock directory is writable");
            std::fs::write(format!("{directory}/{theme}.html"), trace)
                .expect("the mock page is written");
            // The screen a new window opens onto, which had no fixture at all
            // and is therefore the one nobody had looked at.
            std::fs::write(
                format!("{directory}/first-run-{theme}.html"),
                page(theme, rail::Tab::Session, false),
            )
            .expect("the first-run page is written");
            std::fs::write(
                format!("{directory}/capabilities-{theme}.html"),
                page(theme, rail::Tab::Capabilities, true),
            )
            .expect("the capabilities page is written");
        }
    }
}
