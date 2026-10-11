//! The ACP Inspector's screens: a window onto an [`Inspector`].
//!
//! Strictly presentational: everything here renders a store or calls core back,
//! with no protocol logic or connection lifecycle. See `docs/architecture.md` §5.
//!
//! One crate, a feature per renderer
//! (`docs/adr/0011-one-ui-crate-a-feature-per-renderer.md`): only `shell.rs`
//! imports `dioxus::desktop`, gated on the `desktop` feature.

#[cfg(not(any(feature = "desktop", feature = "web")))]
compile_error!(
    "acp-inspector draws in a renderer, and a build has to pick one: `--features desktop` or `--features web`"
);

mod agent;
mod app;
mod appearance;
mod attachment;
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
// The window icon's mark is reached only through the shell.
#[cfg(feature = "desktop")]
mod mark;
mod mcp;
#[cfg(test)]
mod mock;
mod navigation;
mod palette;
mod permission;
mod rail;
mod raw;
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
    #[cfg(feature = "desktop")]
    shell::launch(App);
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// The window: the rail, the timeline and the Console, with their dialogs.
#[component]
fn App() -> Element {
    // One inspector for the window's lifetime; core holds one connection at a
    // time.
    let inspector = use_hook(Inspector::new);
    use_drop({
        let inspector = inspector.clone();
        move || inspector.disconnect()
    });
    #[cfg(feature = "desktop")]
    use_context_provider(shell::export_retrieval);
    mcp::provide(inspector.clone());
    client_services::provide(inspector.clone());

    // The window's own feedback (a copy took, a file was set aside), on a
    // clock owned by the window so no row can carry it off.
    let announcer = toast::provide();
    let kept = app::kept::use_kept(announcer);
    // One capturing listener for every tailing list, since which lists exist
    // changes as tabs switch (`tail.rs`).
    use_effect(|| {
        document::eval(tail::WATCH);
    });
    // The palette's keystroke in a browser. On desktop it is the View menu's
    // accelerator (`shell.rs`), which the page never sees, so installing this
    // too could toggle the palette twice.
    #[cfg(not(feature = "desktop"))]
    use_effect(|| {
        document::eval(&palette::watch());
    });

    // Window-session navigation: never persisted, so a new window opens on the
    // defaults. A status change can switch the Console tab, so it lives here.
    let navigation = navigation::use_navigation();
    let spine = navigation.spine;
    let tab = navigation.console;
    let rail_tab = navigation.rail;
    // Which region a narrow window shows; CSS alone decides the breakpoint.
    let focus = navigation.region;
    let revealed = navigation.revealed;
    let sought = navigation.sought;
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
    let mut export_work = use_signal(export::Work::default);
    // Pairing, pending calls, sending by hand, and an opened trace file.
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

    // A session that opened shuts the Sessions dialog; one that ended leaves it
    // open, since that is managing the list. `peek`, because subscribing to the
    // previous value would make the effect its own cause.
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

    // Which captured frames became timeline entries, so a trace row can offer
    // the way back to the turn. Recomputed on each timeline revision.
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

    // The frame of the newest request waiting on the reader.
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

        // A screen that fails to draw is caught here; the stores and the
        // connection live above the boundary and survive it.
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
            collapsed: (navigation.collapsed)(),
            on_collapse: move |()| navigation.toggle_rail(),
            on_spine: move |chosen| navigation.layout(chosen),
            on_choose: move |chosen| kept.choose_appearance(chosen),
            on_indent: move |chosen| kept.choose_indentation(chosen),
        }

        view::Banner { view: wire.view }

        session::Sessions { claims: claims.clone() }
        rail::CapabilitiesSheet { claims: claims.clone() }

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

        QuitDialog { on_export, on_quit: quit.quit_now }

        // Every palette command is a control also on screen, built from the
        // same handler.
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

        div { class: format!(
                "{}{}{}",
                focus().class(),
                if drawer() { " drawer-open" } else { "" },
                if (navigation.collapsed)() { " rail-collapsed" } else { "" },
            ),
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
            // The resize edge and the drawer's shade; the stylesheet draws each
            // only where it applies.
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
                // Offered only once an agent has described itself (`live`). No
                // roots: none were asked for, which is not an empty control.
                on_new_session: live().map(|command| {
                    let inspector = opening_from_timeline.clone();
                    EventHandler::new(move |()| {
                        if !mcp::ready_to_open() { return; }
                        app::claims::open_session(inspector.clone(), command.clone(), None, failed, problem);
                    })
                }),
                // Not waited for: the stream has updated the store by the time
                // a call returns.
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
                // The request carries its own resolver; what became of it is
                // read back from the store.
                on_answer: move |answer: permission::Answer| {
                    spawn(async move {
                        let _ = answer.request.select(&answer.option).await;
                    });
                },
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
                turns: wire.view.turns_or(turns()),
                described: described.read().is_some(),
                sought: sought(),
                on_reveal: move |frames| navigation.reveal(frames),
                // The login the agent asked for, where the session would be.
                login: agent::login_guidance(
                    described.read().as_ref().map(|agent| agent.auth_methods.as_slice()).unwrap_or_default(),
                    &auth(),
                    &auth_work().pending,
                    connected,
                    claims.on_authenticate,
                ),
                // The first-run card's recent invocations. A row whose values
                // were not kept opens the launch form filled in instead.
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

        // Drawn by the stylesheet only on windows too narrow for three regions.
        RegionBar { selected: focus(), waiting: blocked(), on_select: move |chosen| navigation.region(chosen) }

        ConsoleAnnouncement { status: status() }

        Toast { said: announcer.said() }
        }
    }
}

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod navigation_tests;
