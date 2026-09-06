//! The window itself: its frame, its menu bar and its icon.
//!
//! **The one module that speaks to the desktop renderer.** Everything here is
//! what the operating system draws around the screens — the window and its
//! size, the title bar, the menu strip, the icon in the task switcher — and
//! nothing else in this crate imports `dioxus::desktop`. That is what lets the
//! crate be one program with a feature per renderer: `main.rs` gates this
//! module and its three calls into it, and the screens under them never learn
//! which window they are in
//! (docs/adr/0011-one-ui-crate-a-feature-per-renderer.md).
//!
//! **What the framework gives is not an application's.** `dioxus-desktop`'s
//! default menu is `Window` and `Edit` and, in a debug build, a `Help` holding
//! two of its own developer commands — a strip that names none of this tool's
//! verbs and one of which offers to open the renderer's inspector inside an
//! inspector. A menu bar that says nothing about the program under it is the
//! fastest thing on a desktop window to read as unfinished, and on Linux and
//! Windows it is drawn *inside* the window where it cannot be ignored.
//!
//! So the strip is this tool's: the two things done to a Trace, the two done to
//! an Agent, and the surfaces, each with the accelerator a reader would try.
//! `Edit` is kept exactly as the framework builds it, because cut, copy, paste
//! and select-all are the platform's own items and a window with four text
//! fields owes them.
//!
//! **Every item here is a control that already exists on screen.** The menu is
//! a second way to reach the window's affordances and never the only one: an
//! affordance that could only be reached from a menu would be an affordance a
//! screenshot cannot show and this project's own tests cannot render.
//!
//! The icon is the same refusal of the framework's default, made the same way:
//! the tool's own mark (`mark`) rather than the renderer's logo.

use dioxus::desktop::muda::{
    Menu, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};
use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use crate::mark;

/// Opens the window and runs `app` in it, for the window's lifetime.
pub fn launch(app: fn() -> Element) {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                // This tool's own strip rather than the framework's, whose
                // three submenus name none of this window's verbs and one of
                // which offers to open a renderer inspector inside an inspector
                // (`menu`).
                .with_menu(menu())
                // And its own mark rather than the renderer's logo, which is
                // what `default_icon()` would put in the task switcher.
                .with_icon(icon().unwrap_or_else(|| {
                    // A window with no icon is what every window without one
                    // looks like; a window that will not open because a rounded
                    // square could not be drawn is worse than that.
                    dioxus::desktop::default_icon().expect("the renderer has one")
                }))
                .with_window(
                    WindowBuilder::new()
                        .with_title("ACP Inspector")
                        // Two screens side by side and the Console under both: the
                        // agent, the turn, and the wire beneath them. The turn is
                        // the one that needs the room across, and everything in the
                        // Console is a long line, which is what it takes the whole
                        // width for.
                        .with_inner_size(LogicalSize::new(1440.0, 880.0))
                        // And the size below which the window stops being those
                        // three things. The stylesheet holds the screens open at a
                        // floor and lets the Console give way to it, but a floor is
                        // a promise the window has to be big enough to keep: drag
                        // this shell short enough and the regions stop fitting,
                        // whichever of them is holding its ground. This is the
                        // smallest window they all still fit in — a 280px rail and
                        // a timeline beside it, and a permission request with room
                        // to be read above the composer.
                        .with_min_inner_size(LogicalSize::new(960.0, 640.0)),
                ),
        )
        .launch(app);
}

/// Names what the window is looking at in its title bar.
///
/// The one piece of the window's chrome the window does not paint: the task
/// switcher, the dock and the window list all read it, and a tool that said
/// only its own name there was a tool whose four open windows were four
/// identical entries.
pub fn set_title(title: &str) {
    dioxus::desktop::window().set_title(title);
}

/// Runs `answer` with the id of every menu item the reader picks, for as long as
/// the calling component is mounted. The ids are the constants in [`command`].
pub fn use_menu(mut answer: impl FnMut(&str) + 'static) {
    dioxus::desktop::use_muda_event_handler(move |event| answer(event.id().as_ref()));
}

/// What a menu item asks for. The ids are matched in `main.rs`.
pub mod command {
    pub const EXPORT_TRACE: &str = "trace-export";
    pub const CLEAR_TRACE: &str = "trace-clear";
    pub const NEW_SESSION: &str = "agent-new-session";
    pub const STOP_AGENT: &str = "agent-stop";
    pub const SHOW_TRACE: &str = "view-trace";
    pub const SHOW_DIAGNOSTICS: &str = "view-diagnostics";
    pub const TOGGLE_CONSOLE: &str = "view-console";
}

fn accelerator(modifiers: Modifiers, code: Code) -> Option<Accelerator> {
    Some(Accelerator::new(Some(modifiers), code))
}

/// The window's menu bar.
///
/// Returns `None` where the platform builds no menu of its own to add to, which
/// is every platform this shell does not run on; the caller passes that through
/// to `Config::with_menu` unchanged.
pub fn menu() -> Option<Menu> {
    let menu = Menu::new();

    let trace = Submenu::new("Trace", true);
    trace
        .append_items(&[
            &MenuItem::with_id(
                command::EXPORT_TRACE,
                "Export…",
                true,
                accelerator(Modifiers::CONTROL, Code::KeyE),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                command::CLEAR_TRACE,
                "Clear",
                true,
                accelerator(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyK),
            ),
        ])
        .ok()?;

    let agent = Submenu::new("Agent", true);
    agent
        .append_items(&[
            &MenuItem::with_id(
                command::NEW_SESSION,
                "New session",
                true,
                accelerator(Modifiers::CONTROL, Code::KeyN),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                command::STOP_AGENT,
                "Stop",
                true,
                accelerator(Modifiers::CONTROL, Code::Period),
            ),
        ])
        .ok()?;

    let view = Submenu::new("View", true);
    view.append_items(&[
        &MenuItem::with_id(
            command::SHOW_TRACE,
            "Trace",
            true,
            accelerator(Modifiers::CONTROL, Code::Digit1),
        ),
        &MenuItem::with_id(
            command::SHOW_DIAGNOSTICS,
            "Diagnostics",
            true,
            accelerator(Modifiers::CONTROL, Code::Digit2),
        ),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(
            command::TOGGLE_CONSOLE,
            "Console",
            true,
            accelerator(Modifiers::CONTROL, Code::Backquote),
        ),
    ])
    .ok()?;

    // The platform's own, unchanged: a window with four text fields and a
    // composer owes cut, copy, paste and select-all, and these are the items
    // the operating system expects to find under this name.
    let edit = Submenu::new("Edit", true);
    edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::select_all(None),
    ])
    .ok()?;

    let window = Submenu::new("Window", true);
    window
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
            &PredefinedMenuItem::quit(None),
        ])
        .ok()?;

    menu.append_items(&[&trace, &agent, &view, &edit, &window])
        .ok()?;
    Some(menu)
}

/// The size of the icon, in pixels a side.
const ICON: u32 = 64;

/// The window's icon: the tool's own mark, filling the canvas as a window icon
/// does. Drawn rather than shipped — `mark` says why, and holds the drawing;
/// this is the framework's shape put around it.
pub fn icon() -> Option<Icon> {
    Icon::from_rgba(mark::rgba(ICON, 0.0), ICON, ICON).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // AppKit builds an `NSMenu` on the main thread and nowhere else, and the
    // test harness runs every test on a thread of its own — there is no flag
    // that hands one the main thread. So on macOS the strip cannot be built
    // here at all, and the test says so rather than fail or vanish; Linux and
    // Windows build theirs on any thread, and CI runs it there.
    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "muda builds an NSMenu only on the main thread, and the test harness has none to offer"
    )]
    fn the_menu_names_this_tools_verbs_and_keeps_the_platforms() {
        // Built without a window, which is the whole of what a test here can
        // ask: that the strip is constructible and that its ids are the ones
        // the window matches on. What a click does is `main.rs`'s, and that it
        // arrives at all is the framework's.
        let menu = menu().expect("the menu bar is built");
        let names = menu
            .items()
            .iter()
            .filter_map(|item| item.as_submenu().map(|submenu| submenu.text()))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            ["Trace", "Agent", "View", "Edit", "Window"],
            "this tool's verbs first, then the platform's"
        );
    }

    #[test]
    fn every_menu_id_is_matched_by_the_window() {
        // The two halves of a menu item are its id and the arm that answers it,
        // and they are in different files. A command added here and nowhere
        // else is an item that does nothing when it is clicked.
        let answered = include_str!("main.rs");
        // Every name the module declares, read out of the module rather than
        // listed again here: a test that repeated the list would be a third
        // place to forget the command, which is the failure it exists to catch.
        let declared = include_str!("shell.rs")
            .split_once("pub mod command {")
            .expect("the commands are a module")
            .1
            .split_once('}')
            .expect("the module closes")
            .0
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub const "))
            .filter_map(|line| line.split_once(':').map(|(name, _)| name))
            .collect::<Vec<_>>();
        assert_eq!(declared.len(), 7, "every command is found: {declared:?}");

        for name in declared {
            assert!(
                answered.contains(&format!("command::{name}")),
                "the window answers {name}"
            );
        }
    }

    #[test]
    fn the_icon_is_this_windows_own_and_not_the_renderers() {
        assert!(
            icon().is_some(),
            "the mark is drawn from the window's own colours rather than resolved"
        );
    }
}
