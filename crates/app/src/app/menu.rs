//! The menu bar, answered: every item is a control that is also on screen.

use acp_inspector_core::Inspector;
use dioxus::prelude::*;

use super::claims::{Lifecycle, open_session};
use super::launch::Launch;
use super::stores::Stores;
use crate::console::Tab;
use crate::navigation::Navigation;
use crate::{mcp, shell, wire};

/// What the menu's items call.
pub(crate) struct Menu {
    pub stores: Stores,
    pub wire: wire::Wire,
    pub navigation: Navigation,
    pub launch: Launch,
    pub lifecycle: Lifecycle,
    pub on_export: EventHandler<()>,
    pub on_cancel_turn: EventHandler<()>,
    pub on_quit: EventHandler<()>,
}

/// Answers the menu bar. Each arm does what its on-screen control does.
pub(crate) fn use_menu(inspector: &Inspector, menu: Menu) {
    let Menu {
        stores,
        wire,
        navigation,
        launch,
        lifecycle,
        on_export,
        on_cancel_turn,
        on_quit,
    } = menu;
    let Launch {
        launched,
        launching,
        on_disconnect,
        on_restart,
        ..
    } = launch;
    let spine = navigation.spine;
    let menu_open = inspector.clone();
    shell::use_menu(move |command| match command {
        shell::command::OPEN_TRACE => wire.view.choose(wire.live()),
        shell::command::SAVE_TRACE => {
            on_export.call(());
        }
        // Arms the Console's two-step Clear.
        shell::command::CLEAR_TRACE => navigation.clear(),
        shell::command::DISCONNECT => on_disconnect.call(()),
        shell::command::RESTART => on_restart.call(()),
        shell::command::CANCEL_TURN => on_cancel_turn.call(()),
        shell::command::QUIT => on_quit.call(()),
        shell::command::NEW_SESSION => {
            if !mcp::ready_to_open() {
                return;
            }
            let Some(command) = launched() else {
                return;
            };
            let inspector = menu_open.clone();
            open_session(
                inspector,
                command,
                None,
                lifecycle.failed,
                lifecycle.problem,
            );
        }
        shell::command::SHOW_TRACE => {
            navigation.console(Tab::Trace);
        }
        shell::command::SHOW_DIAGNOSTICS => {
            navigation.console(Tab::Diagnostics);
        }
        shell::command::TOGGLE_CONSOLE => navigation.layout(spine().other()),
        _ => {}
    });
    // An item that would do nothing is disabled rather than silent.
    let Stores {
        frames,
        turn,
        described,
        ..
    } = stores;
    use_effect(move || {
        let connected = stores.connected();
        shell::set_enabled(
            shell::command::NEW_SESSION,
            connected && described.read().is_some(),
        );
        shell::set_enabled(shell::command::CANCEL_TURN, turn.read().is_running());
        shell::set_enabled(
            shell::command::RESTART,
            launched.read().is_some() && !launching(),
        );
        shell::set_enabled(shell::command::DISCONNECT, connected);
        shell::set_enabled(shell::command::CLEAR_TRACE, !frames.read().is_empty());
    });
}
