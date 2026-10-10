//! The crash log, and what the window draws when a screen fails to render.

use dioxus::prelude::*;

/// Where panics are logged: core's state directory, resolved from the
/// environment because this crate does not depend on `dirs`.
fn crash_log() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home?.join("Library/Application Support")
    } else if cfg!(windows) {
        std::path::PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.map(|home| home.join(".local/state")))?
    };
    Some(base.join("acp-inspector").join("crash.log"))
}

/// One crash, as the log records it: when, where, what, and the stack.
fn crash_entry(at: jiff::Timestamp, info: &std::panic::PanicHookInfo<'_>) -> String {
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|said| (*said).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned());
    let place = info
        .location()
        .map(|place| format!("{}:{}:{}", place.file(), place.line(), place.column()))
        .unwrap_or_else(|| "an unknown place".to_owned());
    let thread = std::thread::current();
    crash_text(
        at,
        thread.name().unwrap_or("unnamed"),
        &place,
        &what,
        &std::backtrace::Backtrace::force_capture().to_string(),
    )
}

fn crash_text(at: jiff::Timestamp, thread: &str, place: &str, what: &str, stack: &str) -> String {
    format!(
        "--- {at} · acp-inspector {} · thread {thread}\npanicked at {place}: {what}\n{stack}\n",
        env!("CARGO_PKG_VERSION"),
    )
}

/// Adds a crash log to whatever the default panic hook does, which still runs.
pub(crate) fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = crash_log() {
            let entry = crash_entry(jiff::Timestamp::now(), info);
            let _ = path
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .and_then(|_| {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)?
                        .write_all(entry.as_bytes())
                });
        }
        default(info);
    }));
}

/// What the window draws when a screen fails to render, with the way to draw it
/// again. The connection and stores sit above the boundary and survive.
pub(crate) fn recovery(errors: ErrorContext) -> Element {
    let said = errors
        .error()
        .map(|error| error.to_string())
        .unwrap_or_else(|| "An unknown error.".to_owned());
    let log = crash_log().map(|path| path.display().to_string());
    rsx! {
        main { class: "recovery", "data-slot": "recovery", role: "alert",
            h1 { "Something went wrong" }
            p { "A screen failed to draw. The connection and the trace are kept; reloading draws the screens again." }
            pre { class: "mono", "{said}" }
            if let Some(log) = log {
                p { class: "hint", "Panics are logged to " code { class: "mono", "{log}" } "." }
            }
            button {
                class: "btn btn-md btn-filled",
                r#type: "button",
                onclick: move |_| errors.clear_errors(),
                "Reload"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_is_logged_with_when_where_and_what() {
        assert!(
            crash_log().is_none_or(|path| path.ends_with("acp-inspector/crash.log")),
            "the log is under the inspector's own state folder"
        );
        // The hook is process-wide, so the test checks what it writes.
        let entry = crash_text(
            jiff::Timestamp::UNIX_EPOCH,
            "main",
            "src/client.rs:12:5",
            "the probe panicked",
            "0: frame",
        );
        assert_eq!(
            entry,
            format!(
                "--- 1970-01-01T00:00:00Z · acp-inspector {} · thread main\npanicked at src/client.rs:12:5: the probe panicked\n0: frame\n",
                env!("CARGO_PKG_VERSION")
            )
        );
        assert!(include_str!("crash.rs").contains("        default(info);\n    }));"));
    }

    #[test]
    fn a_screen_that_fails_to_draw_is_replaced_by_a_way_back() {
        #[component]
        fn Broken() -> Element {
            Err(dioxus::core::CapturedError::from_display("the probe failed to draw").into())
        }
        #[component]
        fn Host() -> Element {
            rsx! {
                ErrorBoundary { handle_error: recovery, Broken {} }
            }
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"data-slot="recovery""#)
                && html.contains("the probe failed to draw")
                && html.contains(">Reload</button>"),
            "{html}"
        );
    }
}
