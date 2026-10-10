//! The ACP Inspector's screens: a window onto an [`Inspector`].
//!
//! **Strictly presentational** (`docs/architecture.md` §5). Everything here
//! subscribes to a store and renders it, or takes a click and calls core back.
//! There is no protocol logic, no connection lifecycle and nothing to decide: a
//! failed spawn opens the console because the *status* says the agent never
//! started, and frames appear because the *trace* grew — both of them facts
//! core's tests assert without a window (`crates/core/tests/inspector.rs`).
//!
//! **One crate, a feature per renderer**
//! (`docs/adr/0011-one-ui-crate-a-feature-per-renderer.md`). The screens are
//! the same program whichever window draws them; what differs is the shell
//! around them — the frame, the title bar, the menu strip — and that is
//! `shell.rs`, the one module that imports `dioxus::desktop`, gated on the
//! `desktop` feature along with the three places this file calls into it.
//!
//! This is also the repo's first desktop artifact (§15 q1), so what it proves
//! beyond the screens is the ground itself: a WebView shell over an async child
//! process, built and run by plain `cargo run`.

#[cfg(not(any(feature = "desktop", feature = "web")))]
compile_error!(
    "acp-inspector draws in a renderer, and a build has to pick one: `--features desktop` or `--features web`"
);

mod agent;
mod app;
mod appearance;
mod attachment_draft;
mod client_services;
mod clipboard;
#[allow(dead_code, unused_imports)] // Unmodified registry install includes its complete public API.
mod components;
mod composer;
mod connect;
mod console;
mod copy;
mod disclosure;
mod elicitation;
mod elicitation_schema;
mod elicitation_words;
mod export;
mod indent;
mod json;
mod page;
// The window's own mark, drawn into its icon: reached only through the shell,
// so it goes where the shell goes. The bundle's icon includes the same file by
// path and does not pass through here (`examples/bundle-icon.rs`).
mod attachment;
#[cfg(feature = "desktop")]
mod mark;
mod mcp;
#[cfg(test)]
mod mock;
mod navigation;
mod palette;
mod permission;
mod rail;
mod session;
mod session_settings;
#[cfg(feature = "desktop")]
mod shell;
mod spawn;
mod style;
mod tail;
mod time;
mod timeline;
mod toast;
mod trace;
mod update;
// The Console's tools beyond the frames (F1, F2, F4, F5).
mod raw;
mod view;
mod wire;

use std::collections::HashSet;

use acp_inspector_core::{AgentCommand, Indentation, Inspector, v1};
use dioxus::prelude::*;

use app::bar::TopBar;
use app::claims::Lifecycle;
use app::commands::{PaletteExtras, PaletteWiring, commands_with};
use app::layout::{ConsoleAnnouncement, RegionBar, Screens};
pub(crate) use app::layout::{Focus, Spine};
use app::quit::QuitDialog;
use app::settings::SessionSettingsWork;
use console::{Console, Tab};
use rail::Rail;
use timeline::Timeline;
use toast::Toast;

fn main() {
    app::crash::install_crash_log();
    // Which window draws the screens is the build's to say, not this file's:
    // the desktop shell is a frame, a title bar, a menu and an icon around them
    // (`shell.rs`), and any other renderer gets the screens and nothing around.
    #[cfg(feature = "desktop")]
    shell::launch(App);
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// The window: a spawn form, the turn timeline, and the Console — the trace and
/// the agent's stderr as tabs — under both.
#[component]
fn App() -> Element {
    // One inspector for the window's lifetime. One connection at a time is
    // core's rule (`CONTEXT.md`, *Connection*), so the window has nothing to
    // arbitrate.
    let inspector = use_hook(Inspector::new);
    use_drop({
        let inspector = inspector.clone();
        move || inspector.disconnect()
    });
    #[cfg(feature = "desktop")]
    use_context_provider(shell::export_retrieval);
    mcp::provide(inspector.clone());
    client_services::provide(inspector.clone());

    // And the one thing the window says about *itself* rather than about the
    // agent: that a copy took, or that one of its own files was set aside
    // (`toast.rs`). Provided here because the controls it answers for are on
    // three screens, and takes itself away on a clock this call starts — the
    // window's, so no row can carry it off.
    let announcer = toast::provide();
    let kept = app::kept::use_kept(announcer);
    // One listener for every tailing list in the window, installed once. Which
    // lists exist changes as tabs are selected and surfaces empty, so it
    // captures rather than being attached per element (`tail.rs`).
    use_effect(|| {
        document::eval(tail::WATCH);
    });
    // And the one keystroke that is not a menu accelerator, because it has to
    // work wherever the reader's focus is (`palette.rs`).
    use_effect(|| {
        document::eval(&palette::watch());
    });

    // Both screens from the start: the Console's promise is that the wire and
    // the agent's diagnostics sit *beside* the turn instead of in a separate
    // terminal (§9), and a live view behind a click is a click away from being
    // the terminal it replaced.
    let navigation = navigation::use_navigation();
    let spine = navigation.spine;
    // And which of the Console's two surfaces is on screen. The window's rather
    // than the panel's, because the one thing that changes it without a click is
    // a connection status, and statuses arrive here. Not persisted, and the
    // default is stated by the type (`console::Tab`).
    let tab = navigation.console;
    // Which lookup the rail is on. The window's, like the two above and for the
    // same reason: it is never written to `settings.json`, because what somebody
    // was reading yesterday is not what this window opens onto.
    let rail_tab = navigation.rail;
    // Which region a narrow window shows. Destination navigation coordinates
    // it with the wide layout; CSS alone decides the breakpoint.
    let focus = navigation.region;
    // What each screen has been asked to show of the other's (§9: the turn and
    // the wire, at once). Both are the window's rather than either screen's,
    // because pointing at the other screen is not a thing a screen can do to
    // itself — and reaching the trace also opens the panel it is in, which is
    // this window's state and nobody else's.
    let revealed = navigation.revealed;
    let sought = navigation.sought;
    // The rail as a drawer, its width, and its two accounts open wide: the
    // narrow layout's three pieces of state (D18).
    let drawer = navigation.drawer;
    let rail_width = use_signal(|| rail::RAIL_DEFAULT);
    rail::use_remembered_width(rail_width);
    use_context_provider(|| rail::WideCapabilities(Signal::new(false)));

    let settings_work = use_signal(SessionSettingsWork::default);
    let stores = app::stores::use_stores(&inspector, settings_work, navigation);
    let app::stores::Stores {
        status,
        frames,
        dropped,
        retention,
        lines,
        dropped_lines,
        entries,
        turns,
        turn,
        blocked,
        session,
        prompt_epoch,
        configured,
        described,
        auth,
        ..
    } = stores;
    // The failure is a string because a rendered failure is a sentence: an
    // `io::Error` is neither cloneable nor comparable, and what the panel does
    // with one is print it.
    let mut export_work = use_signal(export::Work::default);
    // Pairing, pending calls, sending by hand, and a trace opened from a file
    // shown in place of the live one (`wire.rs`, `view.rs`).
    let wire = wire::use_wire(inspector.clone(), status);
    let lifecycle = app::claims::use_lifecycle();
    let Lifecycle {
        auth_work,
        failed,
        mut problem,
        ..
    } = lifecycle;
    let launch = app::launch::use_launch(&inspector, stores, kept, lifecycle);
    let app::launch::Launch {
        launched,
        launching,
        unsaved,
        mut prefill,
        live,
        ..
    } = launch;
    let gates = app::claims::use_gates(&inspector, described);

    // The one thing that shuts the Sessions dialog which is not the reader: a
    // session that opened. That is what every control in it which opens one was
    // pressed for, and the screen it opened onto is behind the dialog. A session
    // *ending* leaves it open on purpose — closing or deleting one is managing
    // the list, and a window that walked away in the middle would be answering a
    // click nobody made. `peek` rather than a read, because what this compares
    // against is the last render's value and subscribing to it would make the
    // effect its own cause.
    let mut opened_before = use_signal(|| None::<v1::SessionId>);
    use_effect(move || {
        let live = session();
        if live.is_some() && *opened_before.peek() != live {
            session::close();
            problem.set(None);
        }
        opened_before.set(live);
    });

    #[cfg(feature = "desktop")]
    app::bar::use_title(described, launched);

    // Which captured frames the typed layer made an entry of — the answer a
    // trace row needs to offer the way back to the turn. A memo off the
    // timeline rather than a store of its own: it is a question about entries
    // this window already has, and the timeline announcing a revision is
    // exactly when the answer changes (§11 seam 3).
    let decoded = use_memo(move || {
        wire.view
            .entries_or(entries)
            .read()
            .iter()
            .flat_map(|entry| entry.frames.iter().filter_map(|recorded| recorded.captured))
            .collect::<HashSet<u64>>()
    });

    let prompting = inspector.clone();
    let cancelling = inspector.clone();
    let cancelling_turn = inspector.clone();
    let opening_from_timeline = inspector.clone();
    let exporting = inspector.clone();
    let on_export = EventHandler::new(move |()| {
        navigation.console(Tab::Trace);
        // The activation owns this snapshot, even if Clear or a restart follows.
        let recorded = *dropped.peek() + frames.peek().len();
        let Some(snapshot) = export::start(export_work, exporting.trace(), recorded, Some(wire))
        else {
            return;
        };
        spawn(async move {
            let result = export::write(snapshot).await;
            export_work.write().finish(result);
        });
    });
    let clearing = inspector.clone();
    let on_cancel_turn = EventHandler::new(move |()| {
        if turn.peek().is_running() {
            let inspector = cancelling_turn.clone();
            spawn(async move {
                let _ = inspector.cancel().await;
            });
        }
    });

    let quit = app::quit::use_quit(&inspector, stores, export_work, launch.stopping);
    #[cfg(feature = "desktop")]
    app::menu::use_menu(
        &inspector,
        app::menu::Menu {
            stores,
            wire,
            navigation,
            launch,
            lifecycle,
            on_export,
            on_cancel_turn,
            on_quit: quit.on_quit,
        },
    );

    // The newest request the agent is waiting on the reader for, by the frame
    // that asked it — what "Go to waiting request" goes to.
    let waiting_request = use_memo(move || {
        blocked()
            .then(|| {
                entries
                    .read()
                    .iter()
                    .rev()
                    .find(|entry| entry.is_waiting())
                    .and_then(|entry| entry.frames.iter().find_map(|recorded| recorded.captured))
            })
            .flatten()
    });

    let claims = app::claims::claims(&inspector, stores, gates, lifecycle, launched);
    let failure = launch.failure(stores, problem);
    let copying_session = announcer;
    let connected = stores.connected();

    rsx! {
        style { {style::SHEET} }

        // A screen that fails to draw is caught here rather than leaving a blank
        // window; the stores and the connection are above it and survive (C8).
        ErrorBoundary {
            handle_error: app::crash::recovery,

        TopBar {
            status: status(),
            appearance: (kept.appearance)(),
            indentation: (kept.indentation)(),
            subject: app::bar::subject(described.read().as_ref(), launched.read().as_ref()),
            protocol: described.read().as_ref().map(|agent| agent.protocol_version.to_string()),
            session: session(),
            spine: spine(),
            waiting: blocked(),
            drawer: drawer(),
            on_drawer: move |()| navigation.toggle_drawer(stores.connected() || described.read().is_some() || session().is_some()),
            on_spine: move |chosen| navigation.layout(chosen),
            on_choose: move |chosen| kept.choose_appearance(chosen),
            on_indent: move |chosen| kept.choose_indentation(chosen),
        }

        // A trace opened from a file, said, with the way back (`view.rs`).
        view::Banner { view: wire.view }

        // Where a session is opened, switched or ended, over the screen it
        // changes: the Timeline's own header is what opens it (`session.rs`).
        session::Sessions { claims: claims.clone() }
        // The two accounts, open wide over the screens (D18).
        rail::CapabilitiesSheet { claims: claims.clone() }

        // Where an agent is started, over the window rather than beside it: a
        // task done once per connection does not earn a column of every screen
        // (`connect.rs`). A launch that fails is said in it, and it stays open.
        connect::Connect {
            running: connected,
            launching: launching(),
            recent: (kept.remembered)(),
            unsaved: unsaved(),
            failure,
            launched: launched(),
            prefill: prefill(),
            on_launch: move |agent: AgentCommand| launch.launch.call(agent),
            on_remove: kept.on_remove_recent,
            on_forget: kept.on_forget_recent,
        }

        // Quitting with frames nobody exported asks first (F9).
        QuitDialog { on_export, on_quit: quit.quit_now }

        // Everything a command in the palette can ask for is a control that is
        // also on screen (`palette.rs`), and every one of them is built from a
        // handler this window already has.
        palette::Palette { commands: wire::with_commands(wire, EventHandler::new(move |()| navigation.console(Tab::Trace)), commands_with(PaletteWiring {
            connected,
            described: described.read().is_some(),
            traced: !frames().is_empty(),
            spine: spine(),
            on_spine: EventHandler::new(move |chosen| navigation.layout(chosen)),
            on_tab: EventHandler::new(move |chosen| navigation.console(chosen)),
            on_rail: EventHandler::new(move |chosen| navigation.show_rail(chosen)),
            on_new_session: claims.on_new_session,
            on_stop: launch.on_disconnect,
            on_export,
            on_clear: EventHandler::new(move |()| navigation.clear()),
            on_indent: EventHandler::new(move |()| {
                kept.choose_indentation(Indentation::of(!(kept.indentation)().indented()));
            }),
            on_appearance: EventHandler::new(move |chosen| kept.choose_appearance(chosen)),
        }, PaletteExtras {
            launched: launched.read().is_some() && !launching(),
            turn_running: turn().is_running(),
            session: session(),
            lists_sessions: (gates.lists_sessions)(),
            auth_methods: described
                .read()
                .as_ref()
                .filter(|_| connected)
                .map(|agent| {
                    agent
                        .auth_methods
                        .iter()
                        .map(|method| (method.id().clone(), method.name().to_owned()))
                        .collect()
                })
                .unwrap_or_default(),
            waiting: waiting_request(),
            indented: (kept.indentation)().indented(),
            appearance: (kept.appearance)(),
            on_region: EventHandler::new(move |chosen| navigation.region(chosen)),
            on_restart: launch.on_restart,
            on_cancel_turn,
            on_list: claims.on_list,
            on_authenticate: claims.on_authenticate,
            on_waiting: EventHandler::new(move |ordinal| navigation.waiting(ordinal)),
            on_prompt: EventHandler::new(move |()| navigation.prompt()),
            on_copy_session: EventHandler::new(move |()| {
                if let Some(id) = session.peek().clone() {
                    spawn(async move {
                        if copy::copied(id.to_string()).await {
                            copying_session.say("Copied session ID".to_owned());
                        }
                    });
                }
            }),
        })) }

        div { class: if drawer() { format!("{} drawer-open", focus().class()) } else { focus().class().to_owned() },
            style: "--rail-width: {rail_width}px",
            Rail {
                status: status(),
                launched: launched(),
                session: session(),
                claims: claims.clone(),
                settings: configured(),
                settings_epoch: settings_work().generation,
                changing_mode: settings_work().modes > 0,
                setting_option: settings_work().option,
                commands: timeline::commands(&entries()).len(),
                tab: rail_tab(),
                on_tab: move |chosen| navigation.rail(chosen),
                on_dismiss: move |()| navigation.close_drawer(),
                on_stop: launch.on_disconnect,
                launching: launching(),
                stopping: (launch.stopping)(),
                on_restart: launch.on_restart,
                on_set_mode: app::settings::set_mode(inspector.clone(), settings_work),
                on_set_config_option: app::settings::set_config_option(inspector.clone(), settings_work),
            }
            // The rail's edge, which a reader drags to resize it, and the
            // shade behind it where it is a drawer (D18). Both are drawn by
            // the stylesheet only where they apply.
            rail::RailHandle { width: rail_width }
            div {
                class: "rail-backdrop",
                "data-slot": "rail-backdrop",
                aria_hidden: "true",
                onclick: move |_| navigation.close_drawer(),
            }

            Screens { spine: spine(),
            timeline: rsx! {
            Timeline {
                retention: wire.view.retention_or(retention()),
                prompt_epoch: prompt_epoch(),
                image_advertised: described().is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.image),
                audio_advertised: described().is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.audio),
                embedded_advertised: described().is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.embedded_context),
                entries: wire.view.entries_or(entries),
                turn: turn(),
                session: wire.view.live(session()),
                connected,
                held_frames: wire.view.held_or(dropped() as u64..(dropped() + frames().len()) as u64),
                blocked: blocked(),
                problem: problem(),
                // The same call the Agent screen makes, offered on the screen
                // that says there is no session — and only while there is an
                // agent that has described itself, which is what `live` means
                // and what that screen's own button is drawn behind. No roots ride
                // with it: the control that supplies them is on the rail, and
                // for a session that does not exist yet *none were asked for*
                // is the true answer rather than *an empty control was*.
                on_new_session: live().map(|command| {
                    let inspector = opening_from_timeline.clone();
                    EventHandler::new(move |()| {
                        if !mcp::ready_to_open() { return; }
                        app::claims::open_session(inspector.clone(), command.clone(), None, failed, problem);
                    })
                }),
                // Both of these answer with what the agent said, and neither
                // answer is waited for: the turn's state is the stream's to
                // tell, and it has already told the store by the time a call
                // returns (§11 seam 2).
                mode: app::settings::cycle(&configured(), connected),
                on_set_mode: app::settings::set_mode(inspector.clone(), settings_work),
                on_prompt: move |content: Vec<v1::ContentBlock>| {
                    prompting.submit_prompt(content).map(|_| ())
                },
                on_stop: move |()| {
                    let inspector = cancelling.clone();
                    spawn(async move {
                        let _ = inspector.cancel().await;
                    });
                },
                // The request answers itself: it arrived carrying its own
                // resolver (§5), so what a click needs is the request and the
                // option, and nothing here has to find the connection it came
                // from. Not waited for, like the rest: what the panel then
                // shows is what the store says became of it.
                on_answer: move |answer: permission::Answer| {
                    spawn(async move {
                        let _ = answer.request.select(&answer.option).await;
                    });
                },
                // The same shape for the other blocking request (§7.8), and the
                // same reason it needs nothing else: three answers rather than
                // an option id, each of them a claim about what the reader did,
                // and the request knows how to send whichever it was.
                on_elicit: move |answer: elicitation::Answer| {
                    spawn(async move {
                        let _ = match answer.reply {
                            elicitation::Reply::Accept(content) => {
                                answer.request.accept(content).await
                            }
                            elicitation::Reply::Decline => answer.request.decline().await,
                            elicitation::Reply::Cancel => answer.request.cancel().await,
                        };
                    });
                },
                // A row the reader asked for from the Console, and an entry's
                // frames on their way there. Reaching the trace opens the
                // Console and selects the tab it is on: an affordance that
                // scrolled a panel nobody can see would be a control that does
                // nothing, and this is the same escalation a failed spawn makes
                // for the same reason (§9).
                turns: wire.view.turns_or(turns()),
                // Whether there is an agent that could open a session, which is
                // what the header's session control is drawn behind. The two
                // accounts it used to draw are the rail's now.
                described: described.read().is_some(),
                sought: sought(),
                on_reveal: move |frames| navigation.reveal(frames),
                // The login the agent asked for, where the session would be
                // (X5): the Capabilities surface's own guidance and actions.
                login: agent::login_guidance(
                    described.read().as_ref().map(|agent| agent.auth_methods.as_slice()).unwrap_or_default(),
                    &auth(),
                    &auth_work().pending,
                    connected,
                    claims.on_authenticate,
                ),
                // The first-run card's recent invocations (D10, X10): the
                // launch form's own rows, launching directly; a row whose
                // variable values were not kept opens the form filled in.
                recent: (!kept.remembered.read().is_empty()).then(|| rsx! {
                    spawn::RecentList {
                        entries: (kept.remembered)(),
                        labelled_by: "onboarding-recent-label".to_owned(),
                        launching: launching(),
                        on_launch: move |entry: AgentCommand| {
                            if mcp::attempt_launch() {
                                launch.launch.call(entry);
                            }
                        },
                        on_refill: move |entry: AgentCommand| {
                            let next = prefill.peek().as_ref().map_or(0, |(nth, _)| nth + 1);
                            prefill.set(Some((next, entry)));
                            connect::open();
                        },
                        on_remove: kept.on_remove_recent,
                        on_forget: kept.on_forget_recent,
                    }
                }),
            }
            },

            // The wire and the agent's diagnostics, beside the turn: two tabs,
            // one at a time (§9, ADR 0009).
            console: rsx! { Console {
                frames: wire.view.frames_or(frames),
                dropped_frames: wire.view.dropped_or(dropped()),
                saved: export_work.read().result.clone(),
                export_pending: export_work.read().pending,
                on_export,
                on_clear: move |()| clearing.trace().clear(),
                lines: wire.view.lines_or(lines),
                dropped_lines: wire.view.dropped_lines_or(dropped_lines()),
                wire,
                tab: tab(),
                on_select: move |chosen| navigation.console(chosen),
                revealed,
                decoded,
                on_seek: move |ordinal| navigation.seek(ordinal),
            } }
            }
        }

        // The way between the three regions on a window too narrow for them,
        // drawn by the stylesheet only below the width where they stop fitting.
        RegionBar { selected: focus(), waiting: blocked(), on_select: move |chosen| navigation.region(chosen) }

        ConsoleAnnouncement { status: status() }

        // Feedback belongs to the window rather than any one region.
        Toast { said: announcer.said() }
        }
    }
}

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod navigation_tests;
