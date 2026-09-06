//! Every command this window has, by name (ADR 0009).
//!
//! **It is the menu bar's list, reached the other way.** The strip at the top of
//! the window is how a desktop application is read when you are looking for what
//! it can do; this is how one is driven when you already know. Every entry here
//! is a control that exists on screen and a menu item that exists in the strip —
//! the same rule `shell.rs` states, for the same reason: an affordance that
//! could only be reached from here would be an affordance a screenshot cannot
//! show and this project's own tests cannot render.
//!
//! **The platform owns the modality**, as it does for the other two dialogs
//! (`connect.rs`): `showModal` traps focus, dims the window behind it, closes on
//! Escape and gives focus back to whatever opened it. What is not the platform's
//! is the shortcut, because there is no such thing as a native one here — a
//! keystroke that works wherever the reader is has to be a listener on the
//! document, and this module's one script is it.
//!
//! **Nothing here decides anything.** A command is a label, a sentence and a
//! handler the window already had; this file filters a list and calls one.

use dioxus::prelude::*;

/// The dialog, by the name the scripts below reach it under.
const DIALOG: &str = "palette-dialog";

/// The field inside it, by the name those scripts find it under. A slot rather
/// than a class, because what this names is *the thing the keyboard belongs in*
/// rather than how it is painted.
const QUERY: &str = "[data-slot='palette-query']";

/// The shortcut, installed once at the root.
///
/// **A toggle rather than an open**, because that is what every application
/// with this control does: the keystroke that summoned it puts it away. Nothing
/// an agent said reaches this string — the only value interpolated is a `const`
/// this crate wrote — and the listener is on the document because the point of
/// the shortcut is that it works wherever the reader's focus is.
pub fn watch() -> String {
    format!(
        r#"
document.addEventListener("keydown", (event) => {{
  if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "k") {{
    return;
  }}
  const palette = document.getElementById({DIALOG:?});
  if (!palette) {{
    return;
  }}
  event.preventDefault();
  if (palette.open) {{
    palette.close();
  }} else {{
    palette.showModal();
    palette.querySelector({QUERY:?})?.focus();
  }}
}});
"#
    )
}

/// Open it, for the control at the foot of the rail.
///
/// **And put the keyboard in the field**, which is the whole of what this
/// surface is: `showModal` gives focus to the dialog itself, so a palette that
/// left it there opened onto a question nobody could type into and a list the
/// arrow keys did not move. The platform's own `autofocus` is on the field too
/// and is not enough on its own — it is honoured for the *first* showing of a
/// dialog and this one is opened over and over.
pub fn open() {
    document::eval(&opening());
}

/// The script [`open`] evaluates, so the test below reads the one that runs.
fn opening() -> String {
    format!(
        "const palette = document.getElementById({DIALOG:?});
palette?.showModal();
palette?.querySelector({QUERY:?})?.focus();"
    )
}

/// Shut it, which is what running a command does.
pub fn close() {
    document::eval(&format!("document.getElementById({DIALOG:?})?.close()"));
}

/// One thing the window can be asked to do.
///
/// The group is what it is *about* — the connection, the session, the wire, the
/// window itself — because a list of eighteen verbs read in one run is a list
/// nobody reads.
#[derive(Clone, PartialEq)]
pub struct Command {
    pub group: &'static str,
    pub label: String,
    /// What it does, in the words the control that does it uses. Not a
    /// paraphrase: a palette that renamed the affordances would be a second
    /// vocabulary for the same window.
    pub says: String,
    pub run: EventHandler<()>,
}

impl Command {
    pub fn new(group: &'static str, label: &str, says: &str, run: EventHandler<()>) -> Self {
        Self {
            group,
            label: label.to_owned(),
            says: says.to_owned(),
            run,
        }
    }

    fn found_by(&self, typed: &str) -> bool {
        found(self.group, &self.label, &self.says, typed)
    }
}

/// Whether a command answers to what has been typed.
///
/// **What it is about, what it is called, and what it does** — all three,
/// because a reader looking for a verb has the sentence and one looking for a
/// surface has the group. Case-insensitive substring and deliberately nothing
/// cleverer, which is the rule the Console's own filter already states.
fn found(group: &str, label: &str, says: &str, typed: &str) -> bool {
    if typed.is_empty() {
        return true;
    }

    format!("{group} {label} {says}")
        .to_lowercase()
        .contains(typed)
}

/// The palette.
#[component]
pub fn Palette(commands: Vec<Command>) -> Element {
    let mut typed = use_signal(String::new);
    let mut highlighted = use_signal(|| 0usize);
    let query = typed().trim().to_lowercase();
    let found: Vec<Command> = commands
        .iter()
        .filter(|command| command.found_by(&query))
        .cloned()
        .collect();
    let highlight = highlighted().min(found.len().saturating_sub(1));
    // What a command does when it is chosen: the thing, and then the way out.
    // The dialog is what the reader opened to *do* something, so it closes
    // itself once they have — and it forgets what was typed, because the next
    // question is a different one.
    let mut reset = move || {
        typed.set(String::new());
        highlighted.set(0);
    };

    rsx! {
        dialog {
            id: DIALOG,
            class: "modal palette",
            "data-slot": "palette",
            // **On the dialog rather than on the box inside it**, because a
            // keystroke is only this surface's if it arrives wherever the
            // reader's focus is — and `showModal` can leave that on the dialog
            // itself, where a listener one element further in never hears it.
            onkeydown: move |event| {
                    match event.key() {
                        Key::ArrowDown if !found.is_empty() => {
                            event.prevent_default();
                            highlighted.set((highlight + 1) % found.len());
                        }
                        Key::ArrowUp if !found.is_empty() => {
                            event.prevent_default();
                            highlighted.set((highlight + found.len() - 1) % found.len());
                        }
                        Key::Enter if !found.is_empty() => {
                            event.prevent_default();
                            let command = found[highlight].clone();
                            reset();
                            close();
                            command.run.call(());
                        }
                    // **Said here rather than left to the platform.** A
                    // `<dialog>` shown modally closes on Escape by itself, and
                    // this one did not: the field it opens onto took the key
                    // first and swallowed it, which is what a search input does
                    // with Escape. The field is an ordinary one now and this
                    // closes the dialog whatever the field does with the key.
                    Key::Escape => {
                        reset();
                        close();
                    }
                    _ => {}
                }
            },
            div {
                class: "modal-box dialog-box palette-box",
                input {
                    class: "palette-query",
                    "data-slot": "palette-query",
                    // Not `search`: the type comes with a clear affordance this
                    // window does not draw and with Escape already spoken for.
                    r#type: "text",
                    value: "{typed}",
                    spellcheck: false,
                    autofocus: true,
                    aria_label: "Run a command",
                    placeholder: "Run a command…",
                    oninput: move |event| {
                        typed.set(event.value());
                        highlighted.set(0);
                    },
                }
                div { class: "palette-list", role: "listbox", aria_label: "Commands",
                    if found.is_empty() {
                        p { class: "empty", "Nothing here answers to that." }
                    }
                    for (position, command) in found.iter().enumerate() {
                        {
                            let previous = position.checked_sub(1).map(|before| found[before].group);
                            let chosen = command.clone();
                            rsx! {
                                div { key: "{command.group} {command.label}",
                                // The group's name, over the first command in
                                // it: eighteen verbs read in one run is a list
                                // nobody reads, and what they are *about* is the
                                // way a reader narrows before they type.
                                if previous != Some(command.group) {
                                    div { class: "palette-group eyebrow", "{command.group}" }
                                }
                                button {
                                    class: "palette-row",
                                    r#type: "button",
                                    role: "option",
                                    aria_selected: position == highlight,
                                    onclick: move |_| {
                                        let command = chosen.clone();
                                        reset();
                                        close();
                                        command.run.call(());
                                    },
                                    span { class: "palette-label", "{command.label}" }
                                    span { class: "palette-desc", "{command.says}" }
                                }
                                }
                            }
                        }
                    }
                }
                div { class: "palette-foot",
                    span { "↑↓ navigate" }
                    span { "↵ run" }
                    span { "esc close" }
                    span { class: "palette-count", "{found.len()} of {commands.len()}" }
                }
            }

            // The click outside, which `showModal` does not give (`connect.rs`).
            form { method: "dialog", class: "modal-backdrop",
                button { "data-slot": "dismiss", onclick: move |_| reset(), "close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<Command> {
        vec![
            Command::new(
                "Connection",
                "Connect…",
                "Start an agent, or run one you have run before",
                EventHandler::new(move |()| {}),
            ),
            Command::new(
                "Session",
                "session/new",
                "Opens and switches to a new session",
                EventHandler::new(move |()| {}),
            ),
            Command::new(
                "Trace",
                "Export…",
                "Write every frame to a JSONL file",
                EventHandler::new(move |()| {}),
            ),
        ]
    }

    fn shown() -> String {
        #[component]
        fn Host() -> Element {
            rsx! {
                Palette { commands: commands() }
            }
        }

        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_platform_owns_the_modality_here_too() {
        // The same four behaviours the other two dialogs rely on it for: a focus
        // trap, a dimmed window, Escape, and focus handed back to whatever
        // opened it. Whether it is showing lives on the element and in no
        // signal (`connect.rs`).
        let html = shown();
        assert!(
            html.contains(r#"id="palette-dialog""#) && html.contains(r#"class="modal palette""#),
            "{html}"
        );
        assert!(
            html.contains(r#"<form method="dialog" class="modal-backdrop">"#),
            "the click outside, which `showModal` does not give: {html}"
        );
        assert!(
            !html.contains("open=true") && !html.contains(r#"open="#),
            "an attribute would draw the box in the page with no modality at all: {html}"
        );
    }

    #[test]
    fn opening_it_puts_the_keyboard_in_the_question() {
        // **Both ways in, because there are two**, and a palette that opened
        // without focus is a question nobody can type into and a list the arrow
        // keys do not move. `showModal` focuses the dialog, not the field in it.
        let html = shown();
        assert!(
            html.contains(r#"data-slot="palette-query""#) && html.contains("autofocus"),
            "the field says which one it is and asks the platform for the first focus: {html}"
        );
        // Not a search field: the type comes with a clear affordance this window
        // does not draw, and with Escape already spoken for — which is how a
        // dialog that closes on Escape came to be one that did not.
        assert!(
            !html.contains(r#"type="search""#),
            "and Escape belongs to the dialog: {html}"
        );

        for script in [watch(), opening()] {
            assert!(
                script.contains("showModal()")
                    && script.contains("[data-slot='palette-query']")
                    && script.contains(".focus()"),
                "the way in shows it and then puts the keyboard in it: {script}"
            );
        }
    }

    #[test]
    fn the_shortcut_is_a_toggle_and_carries_nothing_an_agent_said() {
        let watch = watch();
        assert!(
            watch.contains("metaKey") && watch.contains("ctrlKey") && watch.contains(r#""k""#),
            "the keystroke every application with this control uses: {watch}"
        );
        assert!(
            watch.contains("palette.close()") && watch.contains("palette.showModal()"),
            "and it puts away what it summoned: {watch}"
        );
        // The only value interpolated is a `const` this crate wrote.
        assert!(
            watch.contains(r#""palette-dialog""#),
            "the dialog is named by a literal: {watch}"
        );
    }

    #[test]
    fn every_command_is_named_grouped_and_says_what_it_does() {
        let html = shown();
        for (group, label, says) in [
            ("Connection", "Connect…", "Start an agent"),
            ("Session", "session/new", "Opens and switches"),
            ("Trace", "Export…", "Write every frame"),
        ] {
            assert!(
                html.contains(group) && html.contains(label) && html.contains(says),
                "{label} is on the list with what it is about and what it does: {html}"
            );
        }
        // A list of verbs read in one run is a list nobody reads, so each is
        // under the thing it is about — and the group is drawn once.
        assert_eq!(html.matches(r#"class="palette-group eyebrow""#).count(), 3);
        // The keyboard moves through it, and the row it is on says so.
        assert!(
            html.contains(r#"role="listbox""#)
                && html.contains(r#"role="option""#)
                && html.contains(r#"aria-selected=true"#),
            "{html}"
        );
        assert!(
            html.contains("3 of 3"),
            "and the count is the arithmetic: {html}"
        );
    }

    #[test]
    fn a_command_answers_to_what_it_is_about_what_it_is_called_and_what_it_does() {
        assert!(
            found("Connection", "Connect…", "Start an agent", ""),
            "an empty query is every command"
        );
        assert!(
            found("Connection", "Connect…", "Start an agent", "connect"),
            "a command is found by its label"
        );
        assert!(
            found("Session", "session/new", "Opens and switches", "switches"),
            "and by what it says it does, because a reader looking for a verb has the sentence"
        );
        assert!(
            found("Trace", "Export…", "Write every frame", "trace"),
            "and by what it is about, because one looking for a surface has the group"
        );
        assert!(!found(
            "Trace",
            "Export…",
            "Write every frame",
            "elicitation"
        ));
    }
}
