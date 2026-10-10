//! The window itself: its frame, its menu bar and its icon.
//!
//! **The one module that speaks to the desktop renderer.** Everything here is
//! what the operating system draws around the screens — the window and its
//! size, the title bar, the menu strip, the icon in the task switcher — and
//! nothing else in this crate imports `dioxus::desktop`. That is what lets the
//! crate be one program with a feature per renderer: `main.rs` gates this
//! module and its calls into it, and the screens under them never learn which
//! window they are in (docs/adr/0011-one-ui-crate-a-feature-per-renderer.md).
//!
//! **What the framework gives is not an application's.** `dioxus-desktop`'s
//! default menu names none of this tool's verbs. So the strip is this tool's:
//! the two things done to a Trace, the things done to an Agent, and the
//! surfaces, each with the accelerator a reader would try — ⌘ on macOS and
//! Control elsewhere, because Control on a Mac is text editing (B5). `Edit` is
//! kept exactly as the framework builds it.
//!
//! **Every item here is a control that already exists on screen**, or the
//! platform's own. The menu is a second way to reach the window's affordances
//! and never the only one.
//!
//! **And the process around the window**: a Finder-launched app inherits a
//! `PATH` with none of the user's tools on it, so the login shell is asked for
//! the real one at startup ([`login_path`]); the window is painted the colour
//! of the canvas before the page loads, so a dark launch does not flash white;
//! and closing a window that holds frames nobody exported asks first (F9).

use std::{
    cell::RefCell,
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::{OnceLock, mpsc},
    time::Duration,
};

use dioxus::desktop::muda::{
    Menu, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};
use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder, WindowCloseBehaviour};
use dioxus::prelude::*;

use crate::mark;
use crate::palette::shortcut::{self, Chord, Key};

/// Opens the window and runs `app` in it, for the window's lifetime.
pub fn launch(app: fn() -> Element) {
    resolve_login_path();
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                // The window navigates only to its own pages (S1). Dioxus
                // serves its own and hands http(s) and mailto links to the
                // external browser before this is consulted; everything that
                // reaches here — file drops, data:, javascript:, custom
                // schemes — is refused.
                .with_navigation_handler(navigable)
                .with_menu(menu())
                // The canvas colour, before the page has painted anything (D4).
                .with_background_color(canvas(prefers_dark()))
                .with_icon(icon().unwrap_or_else(|| {
                    // A window with no icon is what every window without one
                    // looks like; a window that will not open because a rounded
                    // square could not be drawn is worse than that.
                    dioxus::desktop::default_icon().expect("the renderer has one")
                }))
                .with_window(titled(
                    WindowBuilder::new()
                        .with_title("ACP Inspector")
                        .with_inner_size(LogicalSize::new(1440.0, 880.0))
                        // The smallest window the rail, the timeline and a
                        // permission request still fit in.
                        .with_min_inner_size(LogicalSize::new(960.0, 640.0)),
                )),
        )
        .launch(app);
}

/// Whether the window's toolbar is drawn in the title bar (D17): on macOS the
/// title bar is transparent over a full-size content view, so the toolbar is
/// the top of the window, with the traffic lights inset into it. Everywhere
/// else the platform's decorated frame is kept as it is.
pub const TOOLBAR_IN_TITLE_BAR: bool = cfg!(target_os = "macos");

/// The traffic lights' inset from the window's top-left corner, in points:
/// centred on the 52px toolbar, and clear of the brand by the padding the
/// stylesheet gives the bar's leading edge (`.bar.titled`).
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS: (f64, f64) = (18.0, 20.0);

#[cfg(target_os = "macos")]
fn titled(window: WindowBuilder) -> WindowBuilder {
    use dioxus::desktop::LogicalPosition;
    use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
    window
        .with_titlebar_transparent(true)
        .with_fullsize_content_view(true)
        .with_title_hidden(true)
        .with_traffic_light_inset(LogicalPosition::new(TRAFFIC_LIGHTS.0, TRAFFIC_LIGHTS.1))
}

#[cfg(not(target_os = "macos"))]
fn titled(window: WindowBuilder) -> WindowBuilder {
    window
}

/// Moves the window with the pointer, for a press on the toolbar's empty
/// space where the toolbar is the title bar ([`TOOLBAR_IN_TITLE_BAR`]).
pub fn drag_window() {
    if TOOLBAR_IN_TITLE_BAR {
        dioxus::desktop::window().drag();
    }
}

/// Zooms or unzooms the window, for a double-click on the same empty space —
/// what a double-click on a title bar does.
pub fn zoom_window() {
    if TOOLBAR_IN_TITLE_BAR {
        let window = dioxus::desktop::window();
        window.set_maximized(!window.is_maximized());
    }
}

/// Whether the webview may navigate to `url` itself: only the app's own
/// pages, as the renderer serves them (`dioxus://index.html/`, or
/// `http(s)://dioxus.index.html/` where the custom scheme is spelled as a
/// host). External http(s) links never navigate the window; the renderer
/// opens them in the reader's browser instead. Anything else is denied.
fn navigable(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    match parsed.scheme() {
        "dioxus" => true,
        "http" | "https" => parsed
            .host_str()
            .is_some_and(|host| host == "dioxus.index.html"),
        _ => false,
    }
}

/// The canvas colour in each scheme: `--bg` in `theme.css`.
fn canvas(dark: bool) -> (u8, u8, u8, u8) {
    if dark {
        (0x0a, 0x0c, 0x0f, 0xff)
    } else {
        (0xf4, 0xf6, 0xf8, 0xff)
    }
}

/// Whether the first paint should be dark: the stored choice if there is one,
/// else the desktop's, else dark — a dark flash on a light desktop is the
/// smaller fault. Read on the main thread before the event loop starts, while
/// there is no window yet to keep responsive.
fn prefers_dark() -> bool {
    match acp_inspector_core::Settings::load().appearance() {
        acp_inspector_core::Appearance::Light => false,
        acp_inspector_core::Appearance::Dark => true,
        _ => desktop_is_dark().unwrap_or(true),
    }
}

#[cfg(target_os = "macos")]
fn desktop_is_dark() -> Option<bool> {
    let answer = Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    // The key is absent, and the command fails, in light mode.
    Some(
        String::from_utf8_lossy(&answer.stdout)
            .trim()
            .eq_ignore_ascii_case("dark"),
    )
}

#[cfg(not(target_os = "macos"))]
fn desktop_is_dark() -> Option<bool> {
    let theme = std::env::var("GTK_THEME").ok()?;
    Some(theme.to_ascii_lowercase().contains("dark"))
}

/// The user's login-shell `PATH`, once it has been asked for.
static LOGIN_PATH: OnceLock<Option<String>> = OnceLock::new();

/// How long the login shell may take to answer before it is given up on.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(2);

/// Ask the login shell for `PATH`, off the main thread, when the app was not
/// started from a terminal (X2).
///
/// A terminal-started app already has the reader's `PATH` — perhaps a richer
/// one than the login shell's, inside a project's dev shell — so it is left
/// alone. A Finder- or launcher-started one has the system's minimal `PATH`,
/// where `npx` and Homebrew agents are not found.
fn resolve_login_path() {
    use std::io::IsTerminal;
    if std::env::var_os("TERM").is_some() || std::io::stdin().is_terminal() {
        let _ = LOGIN_PATH.set(None);
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("login-path".to_owned())
        .spawn(|| {
            let _ = LOGIN_PATH.set(probe_login_path(&login_shell(), LOGIN_SHELL_TIMEOUT));
        });
    if spawned.is_err() {
        let _ = LOGIN_PATH.set(None);
    }
}

/// The `PATH` an agent should be spawned with, if it differs from this
/// process's: waits for the login shell, at most for its timeout.
pub async fn login_path() -> Option<String> {
    tokio::task::spawn_blocking(|| LOGIN_PATH.wait().clone())
        .await
        .ok()
        .flatten()
}

/// The `PATH` a spawn looks a bare command up in, as of now.
pub fn effective_path() -> Option<String> {
    LOGIN_PATH
        .get()
        .cloned()
        .flatten()
        .or_else(|| std::env::var("PATH").ok())
}

const PATH_MARK: &str = "__ACP_INSPECTOR_PATH__";

/// Runs `$SHELL -ilc` to print `PATH` between markers, and gives up after
/// `timeout`. Interactive, because that is where most people extend `PATH`
/// (`.zshrc`, `.bashrc`); the markers, because an interactive shell may print
/// a banner around it.
fn login_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_owned()
            } else {
                "/bin/sh".to_owned()
            }
        })
}

fn probe_login_path(shell: &str, timeout: Duration) -> Option<String> {
    let script = if shell.ends_with("fish") {
        format!("printf '\\n{PATH_MARK}%s{PATH_MARK}\\n' (string join : $PATH)")
    } else {
        format!("printf '\\n{PATH_MARK}%s{PATH_MARK}\\n' \"$PATH\"")
    };
    let mut child = Command::new(shell)
        .args(["-ilc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (found, answer) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(path) = between_marks(&line) {
                let _ = found.send(path);
                return;
            }
        }
    });
    let path = answer.recv_timeout(timeout).ok();
    let _ = child.kill();
    let _ = child.wait();
    path
}

fn between_marks(line: &str) -> Option<String> {
    let (_, rest) = line.split_once(PATH_MARK)?;
    let (path, _) = rest.split_once(PATH_MARK)?;
    (!path.trim().is_empty()).then(|| path.to_owned())
}

/// Names what the window is looking at in its title bar.
pub fn set_title(title: &str) {
    dioxus::desktop::window().set_title(title);
}

/// Native retrieval stays behind the renderer boundary. Observe launcher exit
/// errors off the UI executor; success means dispatch, not proof a window opened.
pub fn export_retrieval() -> crate::export::Retrieval {
    crate::export::Retrieval {
        open_folder: Some(|path| Box::pin(open_export_folder(path))),
        ..Default::default()
    }
}

async fn open_export_folder(path: std::path::PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let folder = path
            .parent()
            .ok_or_else(|| std::io::Error::other("Export has no containing folder"))?;
        open::that(folder)
    })
    .await
    .map_err(|error| format!("Could not open containing folder: {error}"))?
    .map_err(|error| {
        format!(
            "Could not open containing folder: {error}. Use Copy export path to retrieve the file."
        )
    })
}

/// Stops the owned Connection on the event-loop path itself, before tao can
/// exit the process — unless `guarded` says the close is being asked about
/// first, in which case `on_guarded` runs and the window comes back.
///
/// **How a close is held** (F9): `dioxus-desktop` can hide a window instead of
/// closing it but has no "do nothing". While there are unexported frames the
/// window is set to hide; a close request then hides it, and the task this
/// spawns shows it again with the question on it. The reader sees the window
/// blink; the frames are not lost to a reflexive ⌘W.
pub fn use_shutdown(
    inspector: acp_inspector_core::Inspector,
    guarded: ReadSignal<bool>,
    on_guarded: EventHandler<()>,
) {
    use dioxus::desktop::tao::event::{Event, WindowEvent};
    let window = dioxus::desktop::window();
    let id = window.id();
    use_effect(move || {
        dioxus::desktop::window().set_close_behavior(if guarded() {
            WindowCloseBehaviour::WindowHides
        } else {
            WindowCloseBehaviour::WindowCloses
        });
    });
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        let closing = matches!(event, Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } if *window_id == id);
        if closing && *guarded.peek() {
            spawn(async move {
                dioxus::desktop::window().set_visible(true);
                dioxus::desktop::window().set_focus();
                on_guarded.call(());
            });
            return;
        }
        if matches!(event, Event::LoopDestroyed)
            || closing
            || matches!(event, Event::WindowEvent { window_id, event: WindowEvent::Destroyed, .. } if *window_id == id)
        {
            inspector.disconnect();
        }
    });
}

/// Closes the window for good, whatever it holds: the answer to "quit anyway".
pub fn quit() {
    let window = dioxus::desktop::window();
    window.set_close_behavior(WindowCloseBehaviour::WindowCloses);
    window.close();
}

/// Runs `answer` with the id of every menu item the reader picks, for as long as
/// the calling component is mounted. The ids are the constants in [`command`].
pub fn use_menu(mut answer: impl FnMut(&str) + 'static) {
    dioxus::desktop::use_muda_event_handler(move |event| answer(event.id().as_ref()));
}

/// What a menu item asks for. The ids are matched in `main.rs`.
pub mod command {
    pub const OPEN_TRACE: &str = "trace-open";
    pub const SAVE_TRACE: &str = "trace-export";
    pub const CLEAR_TRACE: &str = "trace-clear";
    pub const NEW_SESSION: &str = "agent-new-session";
    pub const RESTART: &str = "agent-restart";
    pub const CANCEL_TURN: &str = "agent-cancel-turn";
    pub const DISCONNECT: &str = "agent-disconnect";
    pub const SHOW_TRACE: &str = "view-trace";
    pub const SHOW_DIAGNOSTICS: &str = "view-diagnostics";
    pub const TOGGLE_CONSOLE: &str = "view-console";
    pub const QUIT: &str = "app-quit";
}

thread_local! {
    /// The items whose availability follows the window's state, by id. Menus
    /// live on the main thread, and so does everything that changes them.
    static ITEMS: RefCell<HashMap<&'static str, MenuItem>> = RefCell::new(HashMap::new());
}

/// Enables or disables a menu item by its id. Items that do nothing in the
/// window's current state are drawn disabled rather than left to do nothing
/// silently (X13).
pub fn set_enabled(id: &str, enabled: bool) {
    ITEMS.with(|items| {
        if let Some(item) = items.borrow().get(id) {
            item.set_enabled(enabled);
        }
    });
}

/// The platform's primary modifier: ⌘ on macOS, Control elsewhere.
pub fn primary(mac: bool) -> Modifiers {
    if mac {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    }
}

/// The modifiers and key a [`Chord`] is pressed with on one platform.
pub fn keys(chord: Chord, mac: bool) -> (Modifiers, Code) {
    let mut modifiers = primary(mac);
    if chord.shift {
        modifiers |= Modifiers::SHIFT;
    }
    let code = match chord.key {
        Key::Letter('E') => Code::KeyE,
        Key::Letter('N') => Code::KeyN,
        Key::Letter('R') => Code::KeyR,
        Key::Letter('K') => Code::KeyK,
        Key::Letter('Q') => Code::KeyQ,
        Key::Letter('O') => Code::KeyO,
        Key::Letter(_) => Code::Unidentified,
        Key::Digit(1) => Code::Digit1,
        Key::Digit(2) => Code::Digit2,
        Key::Digit(_) => Code::Unidentified,
        Key::Period => Code::Period,
        Key::Backspace => Code::Backspace,
        Key::Backslash => Code::Backslash,
    };
    (modifiers, code)
}

fn accelerator(chord: Chord) -> Option<Accelerator> {
    let (modifiers, code) = keys(chord, shortcut::MAC);
    Some(Accelerator::new(Some(modifiers), code))
}

/// A menu item whose availability [`set_enabled`] can change.
fn item(id: &'static str, text: &str, chord: Option<Chord>) -> MenuItem {
    let item = MenuItem::with_id(id, text, true, chord.and_then(accelerator));
    ITEMS.with(|items| items.borrow_mut().insert(id, item.clone()));
    item
}

/// The window's menu bar.
///
/// Returns `None` where the platform builds no menu of its own to add to; the
/// caller passes that through to `Config::with_menu` unchanged.
pub fn menu() -> Option<Menu> {
    let menu = Menu::new();

    // On macOS the first submenu is the application menu, whatever it is
    // called, so it has to be the one a Mac reader expects there.
    #[cfg(target_os = "macos")]
    let application = {
        let application = Submenu::new("ACP Inspector", true);
        application
            .append_items(&[
                &PredefinedMenuItem::about(None, None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &item(command::QUIT, "Quit ACP Inspector", Some(shortcut::QUIT)),
            ])
            .ok()?;
        application
    };

    let trace = Submenu::new("Trace", true);
    trace
        .append_items(&[
            // Opening a file is the view the Console's own Open… shows;
            // saving is the Console's Export…, which asks where with the
            // platform's Save As (F5).
            &item(
                command::OPEN_TRACE,
                "Open trace…",
                Some(shortcut::OPEN_TRACE),
            ),
            &item(
                command::SAVE_TRACE,
                "Save trace as…",
                Some(shortcut::EXPORT),
            ),
            &PredefinedMenuItem::separator(),
            &item(command::CLEAR_TRACE, "Clear", Some(shortcut::CLEAR)),
        ])
        .ok()?;

    let agent = Submenu::new("Agent", true);
    agent
        .append_items(&[
            &item(
                command::NEW_SESSION,
                "New session",
                Some(shortcut::NEW_SESSION),
            ),
            &item(
                command::CANCEL_TURN,
                "Cancel turn",
                Some(shortcut::CANCEL_TURN),
            ),
            &PredefinedMenuItem::separator(),
            &item(command::RESTART, "Restart", Some(shortcut::RESTART)),
            &item(command::DISCONNECT, "Disconnect agent", None),
        ])
        .ok()?;

    let view = Submenu::new("View", true);
    view.append_items(&[
        &item(command::SHOW_TRACE, "Trace", Some(shortcut::SHOW_TRACE)),
        &item(
            command::SHOW_DIAGNOSTICS,
            "Diagnostics",
            Some(shortcut::SHOW_DIAGNOSTICS),
        ),
        &PredefinedMenuItem::separator(),
        &item(
            command::TOGGLE_CONSOLE,
            "Trace only",
            Some(shortcut::TRACE_ONLY),
        ),
    ])
    .ok()?;

    // The platform's own, unchanged: a window with four text fields and a
    // composer owes cut, copy, paste and select-all.
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
        ])
        .ok()?;
    #[cfg(not(target_os = "macos"))]
    window
        .append(&item(command::QUIT, "Quit", Some(shortcut::QUIT)))
        .ok()?;

    #[cfg(target_os = "macos")]
    menu.append(&application).ok()?;
    menu.append_items(&[&trace, &agent, &view, &edit, &window])
        .ok()?;
    Some(menu)
}

/// The size of the icon, in pixels a side.
const ICON: u32 = 64;

/// The window's icon: the tool's own mark, filling the canvas as a window icon
/// does.
pub fn icon() -> Option<Icon> {
    Icon::from_rgba(mark::rgba(ICON, 0.0), ICON, ICON).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_navigates_only_to_its_own_pages() {
        for own in [
            "dioxus://index.html/",
            "dioxus://index.html/assets/style.css",
            "http://dioxus.index.html/",
            "https://dioxus.index.html/",
        ] {
            assert!(navigable(own), "{own}");
        }
        for elsewhere in [
            "https://example.com/",
            "http://evil.example/dioxus.index.html",
            "https://dioxus.index.html.evil.example@example.com/",
            "file:///etc/passwd",
            "data:text/html,<script>alert(1)</script>",
            "javascript:alert(1)",
            "blob:https://example.com/1",
            "about:blank",
            "mailto:someone@example.com",
            "vscode://file/etc/passwd",
            "x-apple.systempreferences:com.apple.preference.security",
            "",
            "not a url",
        ] {
            assert!(!navigable(elsewhere), "{elsewhere}");
        }
    }

    // AppKit builds an `NSMenu` on the main thread and nowhere else, and the
    // test harness runs every test on a thread of its own. So on macOS the
    // strip cannot be built here at all; Linux and Windows build theirs on any
    // thread, and CI runs it there.
    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "muda builds an NSMenu only on the main thread, and the test harness has none to offer"
    )]
    fn the_menu_names_this_tools_verbs_and_keeps_the_platforms() {
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
        let agent = menu.items()[1].as_submenu().unwrap().items();
        let labels = agent
            .iter()
            .filter_map(|item| item.as_menuitem().map(|item| item.text()))
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            ["New session", "Cancel turn", "Restart", "Disconnect agent"]
        );
        set_enabled(command::NEW_SESSION, false);
        assert!(!agent[0].as_menuitem().unwrap().is_enabled());
    }

    #[test]
    fn shortcuts_use_command_on_macos_and_control_elsewhere() {
        assert_eq!(
            keys(shortcut::RESTART, true),
            (Modifiers::SUPER, Code::KeyR)
        );
        assert_eq!(
            keys(shortcut::RESTART, false),
            (Modifiers::CONTROL, Code::KeyR)
        );
        assert_eq!(
            keys(shortcut::CANCEL_TURN, true),
            (Modifiers::SUPER, Code::Period)
        );
        assert_eq!(
            keys(shortcut::CLEAR, false),
            (Modifiers::CONTROL | Modifiers::SHIFT, Code::Backspace),
            "Clear is off Ctrl+Shift+K"
        );
        for chord in [
            shortcut::EXPORT,
            shortcut::OPEN_TRACE,
            shortcut::CLEAR,
            shortcut::NEW_SESSION,
            shortcut::RESTART,
            shortcut::CANCEL_TURN,
            shortcut::SHOW_TRACE,
            shortcut::SHOW_DIAGNOSTICS,
            shortcut::TRACE_ONLY,
            shortcut::QUIT,
            shortcut::PALETTE,
        ] {
            assert_ne!(
                keys(chord, true).1,
                Code::Unidentified,
                "{chord:?} maps to a key"
            );
        }
    }

    #[test]
    fn every_menu_id_is_matched_by_the_window() {
        let answered = include_str!("main.rs");
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
        assert_eq!(declared.len(), 11, "every command is found: {declared:?}");

        for name in declared {
            assert!(
                answered.contains(&format!("command::{name}")),
                "the window answers {name}"
            );
        }
    }

    #[test]
    fn the_login_shell_answer_is_read_between_its_marks() {
        assert_eq!(
            between_marks(&format!("{PATH_MARK}/opt/homebrew/bin:/usr/bin{PATH_MARK}")),
            Some("/opt/homebrew/bin:/usr/bin".to_owned())
        );
        assert_eq!(between_marks("Welcome to zsh"), None);
        assert_eq!(between_marks(&format!("{PATH_MARK}{PATH_MARK}")), None);
    }

    #[test]
    #[cfg(unix)]
    fn a_login_shell_that_never_answers_is_given_up_on() {
        // A shell that hangs is the case the timeout exists for: startup must
        // not wait on it for longer than it says.
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("acp-login-shell-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let hanging = directory.join("hanging-shell");
        std::fs::write(&hanging, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&hanging, std::fs::Permissions::from_mode(0o755)).unwrap();
        let answering = directory.join("answering-shell");
        std::fs::write(
            &answering,
            format!(
                "#!/bin/sh\necho 'a banner'\necho '{PATH_MARK}/login/bin:/usr/bin{PATH_MARK}'\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&answering, std::fs::Permissions::from_mode(0o755)).unwrap();

        let started = std::time::Instant::now();
        assert_eq!(
            probe_login_path(hanging.to_str().unwrap(), Duration::from_millis(200)),
            None
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(
            probe_login_path(answering.to_str().unwrap(), Duration::from_secs(5)).as_deref(),
            Some("/login/bin:/usr/bin")
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn the_canvas_is_the_themes_ground() {
        assert_eq!(canvas(true), (0x0a, 0x0c, 0x0f, 0xff));
        assert_eq!(canvas(false), (0xf4, 0xf6, 0xf8, 0xff));
        let theme = include_str!("../style/theme.css");
        assert!(theme.contains("--bg: #0a0c0f") && theme.contains("--bg: #f4f6f8"));
    }

    #[test]
    fn the_icon_is_this_windows_own_and_not_the_renderers() {
        assert!(icon().is_some());
    }
}
