//! The window, rendered to a file, so a visual decision can be looked at.
//!
//! **Not a test of anything.** `design/inspector-visual-audit.md` is a record of
//! screenshots somebody took by hand from a running WebView, which is why it
//! goes stale the moment a rail moves: recapturing it means building the shell,
//! launching an agent and driving it to a populated state. This renders the same
//! screens from fixtures with no window and no runtime under them — the
//! components themselves, the committed stylesheet, and a session's worth of
//! made-up traffic for every state a visual pass has to look at: a populated
//! turn, the first run, the capabilities, a permission and an elicitation
//! waiting on the reader, the evidence cards, a turn still streaming, and the
//! three dialogs open.
//!
//! The two requests that wait on the reader are real ones: a scripted agent
//! (`sh`, printing two frames) asks them of a real `Inspector`, because only
//! the core can make a request that can be answered, and a picture of a panel
//! that could not be answered would be a picture of something else.
//!
//! What it cannot show is what only an engine knows: real scrollbars, the
//! disclosure that is a `<details>`, motion. **Every page is drawn still**: the
//! writer marks the document `still`, and the rule that answers it lives in
//! the page this writes rather than in the window's stylesheet, so a capture is
//! the settled frame rather than one caught mid-fade.
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
    AgentCommand, Appearance, ConnectionStatus, Diagnostic, DiagnosticKind, Direction,
    ElicitationRequest, EntryId, EntryKind, Frame, Indentation, Inspector, PermissionRequest,
    Recorded, SessionSettings, TimelineEntry, TracedFrame, Turn, TurnOutcome, TurnState,
    Unrecognized, v1,
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

const SESSION: &str = "testy-session-1";

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

fn update(ordinal: u64, update: v1::SessionUpdate) -> TimelineEntry {
    entry(ordinal, EntryKind::Update(update))
}

fn prompted(ordinal: u64, text: &str) -> TimelineEntry {
    update(
        ordinal,
        v1::SessionUpdate::UserMessageChunk(v1::ContentChunk::new(v1::ContentBlock::from(text))),
    )
}

fn thought(ordinal: u64, text: &str) -> TimelineEntry {
    update(
        ordinal,
        v1::SessionUpdate::AgentThoughtChunk(v1::ContentChunk::new(v1::ContentBlock::from(text))),
    )
}

fn said(ordinal: u64, text: &str) -> TimelineEntry {
    update(
        ordinal,
        v1::SessionUpdate::AgentMessageChunk(
            v1::ContentChunk::new(v1::ContentBlock::from(text))
                .message_id(v1::MessageId::new(format!("m{ordinal}"))),
        ),
    )
}

fn tool(ordinal: u64, call: v1::ToolCall) -> TimelineEntry {
    TimelineEntry {
        id: EntryId::ToolCall(v1::SessionId::new(SESSION), call.tool_call_id.clone()),
        ..update(ordinal, v1::SessionUpdate::ToolCall(call))
    }
}

/// Which screen a page is of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scene {
    /// One finished turn: a prompt, a thought, a tool call, an answer, and a
    /// frame nobody has a name for.
    Populated,
    /// The window as it opens, with nothing launched.
    FirstRun,
    /// The populated window with the rail on what the two parties claimed.
    Capabilities,
    /// A turn stopped on a permission request.
    Permission,
    /// A turn stopped on an elicitation form.
    Elicitation,
    /// The evidence cards: a plan, a diff, a failed tool call, and a turn the
    /// agent refused.
    Evidence,
    /// A turn still streaming: a tool call working and an answer arriving.
    Streaming,
    /// The populated window with the connect dialog open.
    Connect,
    /// The populated window with the sessions dialog open.
    Sessions,
    /// The populated window with the command palette open.
    Palette,
}

impl Scene {
    const ALL: [Scene; 10] = [
        Scene::Populated,
        Scene::FirstRun,
        Scene::Capabilities,
        Scene::Permission,
        Scene::Elicitation,
        Scene::Evidence,
        Scene::Streaming,
        Scene::Connect,
        Scene::Sessions,
        Scene::Palette,
    ];

    /// The file a page of this scene is written to, before its scheme. The
    /// populated page keeps the name it always had.
    fn file(self) -> &'static str {
        match self {
            Scene::Populated => "",
            Scene::FirstRun => "first-run-",
            Scene::Capabilities => "capabilities-",
            Scene::Permission => "permission-",
            Scene::Elicitation => "elicitation-",
            Scene::Evidence => "evidence-",
            Scene::Streaming => "streaming-",
            Scene::Connect => "connect-",
            Scene::Sessions => "sessions-",
            Scene::Palette => "palette-",
        }
    }

    fn live(self) -> bool {
        self != Scene::FirstRun
    }

    /// The dialog this scene draws open, by the id the window gives it.
    fn dialog(self) -> Option<&'static str> {
        match self {
            Scene::Connect => Some("connect-dialog"),
            Scene::Sessions => Some("sessions-dialog"),
            Scene::Palette => Some("palette-dialog"),
            _ => None,
        }
    }

    fn turn(self) -> TurnState {
        match self {
            Scene::FirstRun => TurnState::Idle,
            Scene::Permission | Scene::Elicitation | Scene::Streaming => TurnState::InFlight,
            Scene::Evidence => TurnState::Ended(v1::StopReason::Refusal),
            _ => TurnState::Ended(v1::StopReason::EndTurn),
        }
    }

    /// What the record says the session's turns came to: none before anything
    /// was launched, and the live one open while it streams or waits.
    fn turns(self) -> Vec<Turn> {
        let outcome = match self {
            Scene::FirstRun => return Vec::new(),
            Scene::Permission | Scene::Elicitation | Scene::Streaming => None,
            Scene::Evidence => Some(TurnOutcome::Ended(v1::StopReason::Refusal)),
            _ => Some(TurnOutcome::Ended(v1::StopReason::EndTurn)),
        };
        vec![Turn {
            id: 1,
            at: at(1),
            outcome,
        }]
    }
}

/// The two requests a turn can stop on, asked by a scripted agent of a real
/// `Inspector` — the only thing that can make one that can be answered.
struct Asked {
    _inspector: Inspector,
    permission: PermissionRequest,
    elicitation: ElicitationRequest,
}

async fn ask() -> Asked {
    let permission = r#"{"jsonrpc":"2.0","id":"perm-1","method":"session/request_permission","params":{"sessionId":"testy-session-1","toolCall":{"toolCallId":"call-2","title":"Write transport.rs","kind":"edit","status":"pending","locations":[{"path":"/home/laborant/phoenix/inspector/src/transport.rs","line":42}]},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"allow-always","name":"Always allow edits","kind":"allow_always"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}}"#;
    let elicitation = r#"{"jsonrpc":"2.0","id":"ask-1","method":"elicitation/create","params":{"sessionId":"testy-session-1","mode":"form","message":"Which environment should the retry budget apply to?","requestedSchema":{"type":"object","properties":{"environment":{"type":"string","title":"Environment","enum":["staging","production"],"description":"Where the new budget is rolled out first"},"attempts":{"type":"integer","title":"Attempts","minimum":1,"maximum":10,"default":6},"notify":{"type":"boolean","title":"Notify the on-call channel"}},"required":["environment"]}}}"#;

    let inspector = Inspector::new();
    inspector.connect(
        &AgentCommand {
            command: "sh".into(),
            args: format!("-c\nprintf '%s\\n' '{permission}' '{elicitation}'; cat >/dev/null"),
            ..AgentCommand::default()
        }
        .factory(),
    );
    let (permission, elicitation) = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let (Some(permission), Some(elicitation)) = (
                inspector.pending_permissions().first().cloned(),
                inspector.pending_elicitations().first().cloned(),
            ) {
                break (permission, elicitation);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the scripted agent asks both");
    Asked {
        _inspector: inspector,
        permission,
        elicitation,
    }
}

/// The timeline a scene draws, newest last.
fn entries(scene: Scene, asked: &Asked) -> Vec<TimelineEntry> {
    let prompt = asked_for(scene);
    match scene {
        Scene::FirstRun => Vec::new(),
        Scene::Permission => vec![
            prompted(1, prompt),
            thought(2, "The budget lives in transport.rs; changing it is an edit."),
            tool(
                3,
                v1::ToolCall::new("call-2", "Write transport.rs")
                    .kind(v1::ToolKind::Edit)
                    .status(v1::ToolCallStatus::Pending),
            ),
            TimelineEntry {
                id: EntryId::Permission(asked.permission.id().clone()),
                at: at(4),
                frames: vec![],
                kind: EntryKind::Permission(asked.permission.clone()),
                turn: Some(1),
            },
        ],
        Scene::Elicitation => vec![
            prompted(1, prompt),
            said(2, "Before I change it I need to know where it applies."),
            TimelineEntry {
                id: EntryId::Elicitation(asked.elicitation.id().clone()),
                at: at(3),
                frames: vec![],
                kind: EntryKind::Elicitation(asked.elicitation.clone()),
                turn: Some(1),
            },
        ],
        Scene::Evidence => vec![
            prompted(1, prompt),
            update(
                2,
                v1::SessionUpdate::Plan(v1::Plan::new(vec![
                    v1::PlanEntry::new(
                        "Find where the retry budget is set",
                        v1::PlanEntryPriority::High,
                        v1::PlanEntryStatus::Completed,
                    ),
                    v1::PlanEntry::new(
                        "Raise the attempts from four to six",
                        v1::PlanEntryPriority::Medium,
                        v1::PlanEntryStatus::InProgress,
                    ),
                    v1::PlanEntry::new(
                        "Run the transport tests",
                        v1::PlanEntryPriority::Low,
                        v1::PlanEntryStatus::Pending,
                    ),
                ])),
            ),
            tool(
                3,
                v1::ToolCall::new("call-1", "Edit transport.rs")
                    .kind(v1::ToolKind::Edit)
                    .status(v1::ToolCallStatus::Completed)
                    .content(vec![
                        v1::Diff::new(
                            "/home/laborant/phoenix/inspector/src/transport.rs",
                            "const RETRY_ATTEMPTS: u32 = 6;\nconst RETRY_FLOOR: Duration = Duration::from_millis(250);",
                        )
                        .old_text(
                            "const RETRY_ATTEMPTS: u32 = 4;\nconst RETRY_FLOOR: Duration = Duration::from_millis(250);"
                                .to_owned(),
                        )
                        .into(),
                    ]),
            ),
            tool(
                4,
                v1::ToolCall::new("call-2", "cargo test -p transport")
                    .kind(v1::ToolKind::Execute)
                    .status(v1::ToolCallStatus::Failed)
                    .content(vec![
                        "error: could not compile `transport` (lib test) due to 1 previous error"
                            .into(),
                    ]),
            ),
            said(
                5,
                "I will not raise the budget further without a passing test run.",
            ),
        ],
        Scene::Streaming => vec![
            prompted(1, prompt),
            thought(
                2,
                "Four attempts at a 250ms floor; the budget is set once, at startup.",
            ),
            tool(
                3,
                v1::ToolCall::new("call-1", "Read transport.rs")
                    .kind(v1::ToolKind::Read)
                    .status(v1::ToolCallStatus::InProgress),
            ),
            said(4, "The retry budget is four attempts with a 250ms floor, and"),
        ],
        _ => vec![
            prompted(1, prompt),
            thought(
                2,
                "The budget is likely under transport rather than retry.",
            ),
            tool(
                3,
                v1::ToolCall::new("call-1", "Apply deterministic edit")
                    .kind(v1::ToolKind::Edit)
                    .status(v1::ToolCallStatus::Completed),
            ),
            said(
                4,
                "The retry budget is four attempts with a 250ms floor, set in transport.rs.",
            ),
            // And the thing a flat list could not show: the agent went on
            // talking after it said the turn was over.
            TimelineEntry {
                turn: None,
                ..entry(
                    5,
                    EntryKind::Unrecognized(Unrecognized::NotServiced {
                        method: "session/experimental_progress".to_owned(),
                    }),
                )
            },
        ],
    }
}

fn asked_for(scene: Scene) -> &'static str {
    match scene {
        Scene::Permission | Scene::Elicitation | Scene::Evidence => {
            "Raise the retry budget to six attempts."
        }
        _ => "Read the config and tell me what the retry budget is.",
    }
}

fn frames() -> Vec<TracedFrame> {
    [
        (Direction::ToAgent, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}}}"#),
        (Direction::FromAgent, r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"promptCapabilities":{"image":true,"audio":true}}}}"#),
        (Direction::ToAgent, r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/home/laborant/phoenix/inspector","mcpServers":[]}}"#),
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
        v1::SessionInfo::new(SESSION, "/home/laborant/phoenix/inspector").title("Read the config"),
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
        command: "/home/laborant/phoenix/inspector/.testy/bin/testy".to_owned(),
        args: String::new(),
        env: String::new(),
        cwd: "/home/laborant/phoenix/inspector".to_owned(),
    }
}

/// A scene's timeline, carried as a prop. Never equal to another, so a render
/// is always of the value it was given.
#[derive(Clone)]
struct Drawn(Vec<TimelineEntry>);

impl PartialEq for Drawn {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

/// The whole window, from fixtures — the same regions `App` composes, with the
/// stores replaced by values.
#[component]
fn Window(scene: Scene, drawn: Drawn) -> Element {
    let live = scene.live();
    let view = if scene == Scene::Capabilities {
        rail::Tab::Capabilities
    } else {
        rail::Tab::Session
    };
    // **A window with nothing behind it has nothing in it.** The first-run page
    // is a window before anything was launched: no Timeline, no Session, no
    // Frames, no turn — and no `turn 1` in a header over an empty screen.
    let entries = use_signal(|| drawn.0.clone());
    let frames = use_signal(|| if live { frames() } else { Vec::new() });
    let lines = use_signal(|| if live { lines() } else { Vec::new() });
    let revealed = use_signal(Vec::<u64>::new);
    let decoded = use_signal(HashSet::<u64>::new);
    let session = live.then(|| v1::SessionId::new(SESSION));
    let turn = scene.turn();
    let blocked = matches!(scene, Scene::Permission | Scene::Elicitation);

    // One value for the two surfaces that draw it, exactly as the window builds
    // it: the Agent screen says what was claimed, the dialog acts on what is
    // there.
    let claims = crate::agent::Claims {
        session: session.clone(),
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
            session: session.clone(),
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
                session: session.clone(),
                claims: claims.clone(),
                settings: if live { configured() } else { SessionSettings::default() },
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
                    turn: turn.clone(),
                    session: session.clone(),
                    connected: live,
                    held_frames: 0..6,
                    blocked,
                    problem: None,
                    // The same call the window makes, so the fixture draws the
                    // composer's mode control where the window has one.
                    mode: crate::cycle(&configured(), live),
                    on_prompt: move |_| Ok(()),
                    on_stop: move |()| {},
                    on_set_mode: move |_| {},
                    on_answer: move |_| {},
                    on_elicit: move |_: crate::elicitation::Answer| {},
                    sought: None,
                    on_reveal: move |_| {},
                    described: live,
                    turns: scene.turns(),
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

/// What the writer adds to the page so a capture is the settled frame: no
/// animation and no transition, on a document the writer marks `still`. It is
/// the page's rule and not the window's, so the window never carries it.
const STILL: &str = "html.still *, html.still *::before, html.still *::after { animation: none !important; transition: none !important; }";

/// The rendered window inside the document the shell gives it, in one scheme.
fn page(theme: &str, scene: Scene, asked: &Asked) -> String {
    let mut dom = VirtualDom::new_with_props(
        Window,
        WindowProps {
            scene,
            drawn: Drawn(entries(scene, asked)),
        },
    );
    dom.rebuild_in_place();
    let mut body = dioxus_ssr::render(&dom);
    // A dialog drawn open, as `showModal` leaves it: the attribute the
    // platform sets, and daisyUI's own class for a modal that is showing.
    if let Some(id) = scene.dialog() {
        let closed = format!("<dialog id=\"{id}\" class=\"modal ");
        assert!(body.contains(&closed), "the window draws #{id}");
        body = body.replacen(
            &closed,
            &format!("<dialog open id=\"{id}\" class=\"modal modal-open "),
            1,
        );
    }
    format!(
        "<!doctype html>\n<html lang=\"en\" class=\"still\" data-theme=\"{theme}\">\n<head>\n<meta charset=\"utf-8\">\n<title>ACP Inspector — {theme}</title>\n<style>{sheet}</style>\n<style>{STILL}</style>\n</head>\n<body>\n<div id=\"main\">{body}</div>\n</body>\n</html>\n",
        sheet = style::SHEET,
    )
}

#[tokio::test]
async fn writes_the_window_in_both_schemes() {
    let asked = ask().await;
    let directory = std::env::var("INSPECTOR_MOCK").ok();
    if let Some(directory) = &directory {
        std::fs::create_dir_all(directory).expect("the mock directory is writable");
    }

    for theme in ["inspector-light", "inspector-dark"] {
        for scene in Scene::ALL {
            let html = page(theme, scene, &asked);
            assert!(
                html.contains(r#"data-slot="rail""#)
                    && html.contains(r#"class="turns""#)
                    && html.contains(r#"class="wire""#),
                "{scene:?} renders the rail and both screens"
            );
            match scene {
                Scene::FirstRun => assert!(
                    !html.contains("turn 1"),
                    "a window with nothing launched has no turn to name"
                ),
                Scene::Populated => assert!(
                    !html.contains("No turn yet"),
                    "a window that ended turn 1 does not say there has been no turn"
                ),
                Scene::Permission => assert!(html.contains(r#"data-slot="permission""#)),
                Scene::Elicitation => assert!(html.contains(r#"data-slot="elicitation""#)),
                Scene::Evidence => assert!(
                    html.contains(r#"class="diff""#) && html.contains(r#"data-slot="plan""#)
                ),
                Scene::Streaming => assert!(html.contains("turn-state")),
                _ => {}
            }
            if let Some(directory) = &directory {
                std::fs::write(format!("{directory}/{}{theme}.html", scene.file()), html)
                    .expect("the mock page is written");
            }
        }
    }
}
